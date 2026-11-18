//! What the painter may hand to an accelerator: the float canvas's work
//! (laying layers by their blend modes, mixing, cutting, moving) and the
//! effects it has kernels for, on pixels it holds in its own memory
//! ([D74]).
//!
//! The CPU code in this crate is the reference, and it stays the only code
//! that draws unless a caller passes an [`Accelerator`] to
//! [`render_on`](crate::render_on). Paths, glyphs and pictures are always
//! drawn by the rasteriser on the CPU; an accelerator only takes what the
//! float canvas does with their pixels. The painter's pixels are a
//! [`Surface`]: on the CPU or in the accelerator, wherever they were last
//! changed. An operation runs in the accelerator when it has a kernel for
//! it and one of its operands is already there (an effect with a kernel
//! moves its layer's pixels there); any other operation brings its pixels
//! back and runs on the CPU, so an effect without a kernel costs a copy each
//! way and nothing else.
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

use std::any::Any;
use std::fmt;

use nakkash_format::Result;
use vello_cpu::kurbo::Affine;

use crate::blend::BlendMode;
use crate::effect::{Effect, Space};
use crate::list::MatteMode;
use crate::par;
use crate::pixels::Pixels;

/// A frame-sized grid of premultiplied RGBA float pixels held in an
/// accelerator's memory. What it holds is the accelerator's own type; the
/// painter only moves it between calls.
pub struct Resident {
    width: u16,
    height: u16,
    held: Box<dyn Any + Send + Sync>,
}

impl Resident {
    /// `width` by `height` pixels the accelerator holds as `held`.
    pub fn new(width: u16, height: u16, held: Box<dyn Any + Send + Sync>) -> Resident {
        Resident {
            width,
            height,
            held,
        }
    }

    /// Its width in pixels.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Its height in pixels.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// What the accelerator holds, if it is a `T`.
    pub fn held<T: 'static>(&self) -> Option<&T> {
        self.held.downcast_ref()
    }

    /// What the accelerator holds, to change, if it is a `T`.
    pub fn held_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.held.downcast_mut()
    }
}

impl fmt::Debug for Resident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resident")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// One factor of how much of a source is laid or mixed at each pixel.
#[derive(Debug, Clone, Copy)]
pub enum Share<'a> {
    /// One value per pixel, row by row, worked out on the CPU: a clip's or
    /// masks' coverage, or a matte read there.
    Values(&'a [f32]),
    /// The alpha of pixels held in the accelerator: where a filter layer's
    /// children draw.
    Alpha(&'a Resident),
    /// A matte read from pixels held in the accelerator, by its mode, as
    /// the painter reads one on the CPU.
    Matte(&'a Resident, MatteMode),
}

/// How much of a source is laid or mixed at each pixel: `opacity` times
/// each share in turn, the product kept within 0 to 1.
#[derive(Debug, Clone, Default)]
pub struct Amount<'a> {
    /// The constant factor.
    pub opacity: f32,
    /// The factors that change from pixel to pixel, multiplied in order.
    pub shares: Vec<Share<'a>>,
}

impl<'a> Amount<'a> {
    /// `opacity` everywhere.
    pub fn constant(opacity: f32) -> Amount<'a> {
        Amount {
            opacity,
            shares: Vec::new(),
        }
    }

    /// This amount, also multiplied by `share`.
    pub fn and(mut self, share: Share<'a>) -> Amount<'a> {
        self.shares.push(share);
        self
    }

    /// This amount, also multiplied by `values` when there are any.
    pub fn and_values(self, values: Option<&'a [f32]>) -> Amount<'a> {
        match values {
            Some(values) => self.and(Share::Values(values)),
            None => self,
        }
    }

    /// Whether every share is worked out on the CPU, so the amount can be
    /// read there pixel by pixel.
    fn on_cpu(&self) -> bool {
        self.shares.iter().all(|s| matches!(s, Share::Values(_)))
    }

    /// The amount at pixel `i`, when every share is on the CPU: the
    /// product in the order the shares stand, which is the order the
    /// painter has always multiplied them in.
    fn at(&self, i: usize) -> f32 {
        self.shares
            .iter()
            .fold(self.opacity, |k, share| match share {
                Share::Values(values) => k * values[i],
                Share::Alpha(_) | Share::Matte(..) => unreachable!("an amount read on the CPU"),
            })
    }
}

/// What an accelerator did over a render, for the caller to report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    /// Effects run by a kernel of the accelerator, by name, with how many
    /// times each ran.
    pub accelerated: Vec<(String, u64)>,
    /// Effects run on the CPU because the accelerator has no kernel for
    /// them, by name, with how many times each ran there.
    pub fell_back: Vec<(String, u64)>,
    /// Painter operations other than effects that ran in the accelerator:
    /// lays, mixes, cuts and moves.
    pub operations: u64,
    /// Pixel grids copied into the accelerator, and their bytes.
    pub uploads: (u64, u64),
    /// Pixel grids copied back to the CPU, and their bytes.
    pub downloads: (u64, u64),
}

/// Something that does the float canvas's work in its own memory, such as
/// a GPU ([D74]). Every method may fail, for example when the device is
/// lost; the render then fails with that diagnostic.
///
/// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
pub trait Accelerator: Send + Sync {
    /// Whether it can hold a `width` by `height` grid. A painter of a grid
    /// it cannot hold draws on the CPU.
    fn holds(&self, width: u16, height: u16) -> bool;

    /// `pixels`, copied into its memory.
    ///
    /// # Errors
    /// When the copy fails.
    fn upload(&self, pixels: &Pixels) -> Result<Resident>;

    /// What the rasteriser drew, premultiplied RGBA at 8 bits a channel
    /// row by row, copied into its memory as floats from 0 to 1.
    ///
    /// # Errors
    /// When the copy fails.
    fn upload_bytes(&self, rgba: &[u8], width: u16, height: u16) -> Result<Resident>;

    /// `width` by `height` transparent pixels in its memory.
    ///
    /// # Errors
    /// When it cannot hold them.
    fn transparent(&self, width: u16, height: u16) -> Result<Resident>;

    /// A copy of `pixels`, in its memory.
    ///
    /// # Errors
    /// When it cannot hold them.
    fn duplicate(&self, pixels: &Resident) -> Result<Resident>;

    /// `pixels`, copied back to the CPU.
    ///
    /// # Errors
    /// When the copy fails.
    fn download(&self, pixels: &Resident) -> Result<Pixels>;

    /// Lays `source` on `canvas` by `mode`, each source pixel first scaled
    /// by `amount`, as [`Pixels::lay`] does.
    ///
    /// # Errors
    /// When the work fails.
    fn lay(
        &self,
        canvas: &mut Resident,
        source: &Resident,
        mode: BlendMode,
        amount: &Amount<'_>,
    ) -> Result<()>;

    /// Moves each pixel of `pixels` towards `other`'s by `amount`, as
    /// [`Pixels::mix`] does.
    ///
    /// # Errors
    /// When the work fails.
    fn mix(&self, pixels: &mut Resident, other: &Resident, amount: &Amount<'_>) -> Result<()>;

    /// Multiplies every channel of each pixel by `kept`, one value a
    /// pixel: what masks and clips leave of a layer.
    ///
    /// # Errors
    /// When the work fails.
    fn cut(&self, pixels: &mut Resident, kept: &[f32]) -> Result<()>;

    /// `pixels` moved by `transform`, in pixels of the grid, as
    /// [`Pixels::transformed`] gives.
    ///
    /// # Errors
    /// When the work fails.
    fn transformed(&self, pixels: &Resident, transform: Affine) -> Result<Resident>;

    /// Whether it has a kernel for `effect` with its values. An effect
    /// without one runs on the CPU.
    fn runs(&self, effect: &Effect) -> bool;

    /// Runs `effect` on `pixels` in `space`, as [`Effect::apply`] does;
    /// only called when [`Accelerator::runs`] says it can.
    ///
    /// # Errors
    /// When the work fails.
    fn effect(&self, pixels: &mut Resident, effect: &Effect, space: &Space) -> Result<()>;

    /// Notes that `effect` ran on the CPU for want of a kernel.
    fn fell_back(&self, effect: &Effect);

    /// What it did so far.
    fn tally(&self) -> Tally;
}

impl fmt::Debug for dyn Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Accelerator")
    }
}

/// A layer's or a canvas's pixels while the painter works on them: on the
/// CPU, or held in an accelerator.
#[derive(Debug)]
pub(crate) enum Surface {
    /// On the CPU.
    Cpu(Pixels),
    /// In the accelerator.
    Held(Resident),
}

impl Surface {
    /// Whether the pixels are in the accelerator.
    pub(crate) fn is_held(&self) -> bool {
        matches!(self, Surface::Held(_))
    }

    /// The pixels on the CPU, brought back from `accel` if they are there.
    pub(crate) fn into_pixels(self, accel: Option<&dyn Accelerator>) -> Result<Pixels> {
        match self {
            Surface::Cpu(pixels) => Ok(pixels),
            Surface::Held(held) => held_by(accel).download(&held),
        }
    }

    /// The pixels on the CPU, to read or change in place, brought back
    /// from `accel` first if they are there.
    pub(crate) fn pixels_mut(&mut self, accel: Option<&dyn Accelerator>) -> Result<&mut Pixels> {
        if let Surface::Held(held) = self {
            *self = Surface::Cpu(held_by(accel).download(held)?);
        }
        match self {
            Surface::Cpu(pixels) => Ok(pixels),
            Surface::Held(_) => unreachable!("brought back above"),
        }
    }

    /// The pixels in `accel`, copied there first if they are on the CPU.
    pub(crate) fn held_mut(&mut self, accel: &dyn Accelerator) -> Result<&mut Resident> {
        if let Surface::Cpu(pixels) = self {
            *self = Surface::Held(accel.upload(pixels)?);
        }
        match self {
            Surface::Held(held) => Ok(held),
            Surface::Cpu(_) => unreachable!("copied above"),
        }
    }

    /// A copy, where these pixels are.
    pub(crate) fn duplicate(&self, accel: Option<&dyn Accelerator>) -> Result<Surface> {
        match self {
            Surface::Cpu(pixels) => Ok(Surface::Cpu(pixels.clone())),
            Surface::Held(held) => Ok(Surface::Held(held_by(accel).duplicate(held)?)),
        }
    }

    /// Lays `source` on these pixels by `mode`, scaled by `amount`: in the
    /// accelerator when either is there or `amount` reads pixels that are,
    /// and on the CPU otherwise.
    pub(crate) fn lay(
        &mut self,
        source: &Surface,
        mode: BlendMode,
        amount: &Amount<'_>,
        accel: Option<&dyn Accelerator>,
    ) -> Result<()> {
        match (
            accel,
            self.is_held() || source.is_held() || !amount.on_cpu(),
        ) {
            (Some(accel), true) => {
                let uploaded;
                let source = match source {
                    Surface::Held(held) => held,
                    Surface::Cpu(pixels) => {
                        uploaded = accel.upload(pixels)?;
                        &uploaded
                    }
                };
                accel.lay(self.held_mut(accel)?, source, mode, amount)
            }
            _ => {
                let source = match source {
                    Surface::Cpu(pixels) => pixels,
                    Surface::Held(_) => unreachable!("held pixels come with an accelerator"),
                };
                self.pixels_mut(accel)?.lay(source, mode, |i| amount.at(i));
                Ok(())
            }
        }
    }

    /// Moves each pixel towards `other`'s by `amount`, where they are.
    pub(crate) fn mix(
        &mut self,
        other: &Surface,
        amount: &Amount<'_>,
        accel: Option<&dyn Accelerator>,
    ) -> Result<()> {
        match (accel, self.is_held() || other.is_held() || !amount.on_cpu()) {
            (Some(accel), true) => {
                let uploaded;
                let other = match other {
                    Surface::Held(held) => held,
                    Surface::Cpu(pixels) => {
                        uploaded = accel.upload(pixels)?;
                        &uploaded
                    }
                };
                accel.mix(self.held_mut(accel)?, other, amount)
            }
            _ => {
                let other = match other {
                    Surface::Cpu(pixels) => pixels,
                    Surface::Held(_) => unreachable!("held pixels come with an accelerator"),
                };
                self.pixels_mut(accel)?.mix(other, |i| amount.at(i));
                Ok(())
            }
        }
    }

    /// Multiplies each pixel by `kept`, where the pixels are.
    pub(crate) fn cut(&mut self, kept: &[f32], accel: Option<&dyn Accelerator>) -> Result<()> {
        match (self, accel) {
            (Surface::Held(held), Some(accel)) => accel.cut(held, kept),
            (surface, accel) => {
                par::each_with(
                    surface.pixels_mut(accel)?.data_mut(),
                    kept,
                    |_, pixel, &kept| {
                        *pixel = pixel.map(|c| c * kept);
                    },
                );
                Ok(())
            }
        }
    }

    /// Runs `effect` on these pixels in `space`: with the accelerator's
    /// kernel when it has one, moving the pixels there, or else on the
    /// CPU, bringing them back.
    pub(crate) fn effect(
        &mut self,
        effect: &Effect,
        space: &Space,
        accel: Option<&dyn Accelerator>,
    ) -> Result<()> {
        let Some(accel) = accel else {
            effect.apply(self.pixels_mut(None)?, space);
            return Ok(());
        };
        if accel.runs(effect) {
            return accel.effect(self.held_mut(accel)?, effect, space);
        }
        accel.fell_back(effect);
        effect.apply(self.pixels_mut(Some(accel))?, space);
        Ok(())
    }
}

/// The accelerator that holds pixels the painter has in one.
fn held_by(accel: Option<&dyn Accelerator>) -> &dyn Accelerator {
    accel.expect("held pixels come with the accelerator that holds them")
}

#[cfg(test)]
#[path = "../tests/unit/accel.rs"]
mod tests;
