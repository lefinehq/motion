//! Draws a [`Frame`] into an [`Image`] with `vello_cpu`: one render context
//! per call and no state kept between calls, so frames can be drawn on many
//! threads at once.

use nakkash_format::{Diagnostic, Kind, Result};
use std::sync::Arc;
use vello_cpu::kurbo::{Affine, Point, Rect, Shape};

use vello_cpu::peniko::{ImageAlphaType, ImageQuality, ImageSampler};
use vello_cpu::{
    ImageSource, Level, Mask, Pixmap, RasterizerSettings, RenderContext, RenderMode,
    RenderSettings, Resources, TargetInit,
};

use crate::accel::{Accelerator, Amount, Share, Surface, Texels, held_by};
use crate::blend::BlendMode;
use crate::effect::{Drawing, Effect, EffectContext, Input, Move, Space};
use crate::homography::Homography;
use crate::image::Image;
use crate::list::{
    Blending, Clip, Frame, GlyphDraw, ImageDraw, Layer, LayerMask, MaskMode, Matte, MatteMode,
    Node, PathDraw, Projection,
};
use crate::par;
use crate::pixels::Pixels;
use crate::projected::{self, Buffer, Extent, Plan};
use crate::reads::Reads;
use crate::style::{Color, DrawStyle};

/// How the rasteriser computes colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// 32-bit float per channel while compositing: no banding from repeated
    /// blending. Pixels effects make stay floats to the end of the frame:
    /// filter layers, compositions, mattes and planes read them, and they
    /// are laid on a float canvas, rounded to 8 bits once. A layer shrunk
    /// by what holds it, whose effects read only a few px where it lands,
    /// is drawn at its own size and laid down ([D76]). For delivered
    /// frames and golden images.
    ///
    /// [D76]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d76-a-composition-is-drawn-at-the-size-it-lands-at
    #[default]
    Precise,
    /// 8- and 16-bit integers, and every layer drawn at the size it lands
    /// at: faster, for previews.
    Fast,
    /// [`Quality::Fast`], and blurs whose passes reach 16 px or more each
    /// way worked out on a copy shrunk by 2 to 8 and sampled back up
    /// ([D93]): approximate, the fastest preview. No other quality draws
    /// an approximate blur.
    ///
    /// [D93]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d93-glow-and-blurs-do-less-of-the-same-work
    Draft,
}

/// Which instruction set the rasteriser uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Simd {
    /// The fastest the processor has. Frames are identical on one kind of
    /// processor, and may differ in the last bit between kinds ([D12]).
    ///
    /// [D12]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d12-what-repeatable-means
    #[default]
    Native,
    /// Plain scalar code: slower, and the same bits on every processor, so
    /// golden images compare exactly across machines.
    Portable,
}

/// How [`render_with`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderOptions {
    /// How colours are computed.
    pub quality: Quality,
    /// Which instruction set is used.
    pub simd: Simd,
    /// Whether the float canvas is rounded to 8 bits through an ordered
    /// dither (a 4 by 4 Bayer matrix, half a level either way), which
    /// breaks the steps a dim gradient bands into. Off by default: it
    /// changes every frame drawn through the float canvas by up to a level.
    pub dither: bool,
}

impl RenderOptions {
    /// Precise and portable: what golden images are drawn with.
    pub const GOLDEN: RenderOptions = RenderOptions {
        quality: Quality::Precise,
        simd: Simd::Portable,
        dither: false,
    };

    fn level(self) -> Level {
        match self.simd {
            Simd::Native => Level::try_detect().unwrap_or(Level::baseline()),
            Simd::Portable => Level::fallback(),
        }
    }

    fn mode(self) -> RenderMode {
        match self.quality {
            Quality::Precise => RenderMode::OptimizeQuality,
            Quality::Fast | Quality::Draft => RenderMode::OptimizeSpeed,
        }
    }
}

/// Draws `frame` into a `width` by `height` image with the default options.
///
/// # Errors
/// See [`render_with`].
pub fn render(frame: &Frame, width: u32, height: u32) -> Result<Image> {
    render_with(frame, width, height, RenderOptions::default())
}

/// Draws `frame` into a `width` by `height` image.
///
/// # Errors
/// An out-of-range diagnostic for a size of zero or above 65535 pixels, an
/// invalid-value diagnostic for linear-light blending, which the rasteriser
/// cannot do yet, one for glyphs a font cannot draw, and a conflict for a
/// projected layer that also has affine motion-blur moves.
pub fn render_with(
    frame: &Frame,
    width: u32,
    height: u32,
    options: RenderOptions,
) -> Result<Image> {
    render_on(frame, width, height, options, None)
}

/// Draws `frame` into a `width` by `height` image, handing the float
/// canvas's work to `accel` where it has kernels for it ([D74]). Without one
/// this is [`render_with`], the reference; with one the frame approximates
/// it, within the tolerances D74 sets.
///
/// # Errors
/// As [`render_with`], or a failure of the accelerator.
///
/// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
pub fn render_on(
    frame: &Frame,
    width: u32,
    height: u32,
    options: RenderOptions,
    accel: Option<&dyn Accelerator>,
) -> Result<Image> {
    if !subnormals_kept() {
        return Err(Diagnostic::new(
            Kind::Invalid,
            "this thread flushes subnormal floats to zero, as code built with fast math may set it to: frames would differ from the reference",
        )
        .expected("the processor's default float mode (D90)"));
    }
    // Blurs are approximate only inside a draft frame.
    let _approximate = crate::pyramid::approximate(options.quality == Quality::Draft);
    drawn_at(frame, width, height, options, options.level(), accel)
}

/// Whether this thread's floats keep subnormal numbers, as the reference
/// arithmetic does. A processor set to flush them to zero, as a host built
/// with fast math may set it, gives other bits; the check is a multiply
/// whose result is subnormal and one that reads it back, for both widths.
pub(crate) fn subnormals_kept() -> bool {
    use std::hint::black_box;
    let least = black_box(f32::MIN_POSITIVE);
    let half = black_box(least * black_box(0.5));
    let wide = black_box(f64::MIN_POSITIVE);
    let wide_half = black_box(wide * black_box(0.5));
    half != 0.0
        && black_box(half) * black_box(2.0) == least
        && wide_half != 0.0
        && black_box(wide_half) * black_box(2.0) == wide
}

/// Draws `frame` at the instruction set `level`: [`render_on`]'s work, and
/// in tests any level the processor has, forced.
pub(crate) fn drawn_at(
    frame: &Frame,
    width: u32,
    height: u32,
    options: RenderOptions,
    level: Level,
    accel: Option<&dyn Accelerator>,
) -> Result<Image> {
    let (w, h) = (side(width, "width")?, side(height, "height")?);
    if frame.blending == Blending::Linear {
        return Err(Diagnostic::new(
            Kind::Invalid,
            "linear-light blending is not supported yet: the rasteriser blends gamma-encoded values",
        )
        .expected("gamma blending"));
    }
    // A frame with nothing for the accelerator is drawn on the CPU, as
    // without one: its pixels would only be copied there and back.
    let accel = accel.filter(|accel| has_work(*accel, &frame.nodes));
    let floats = options.quality == Quality::Precise;
    let reads = Arc::new(Reads::of(&frame.nodes, floats));
    let mut painter = Painter::new(w, h, options, level, reads, accel);
    // The background is drawn as the bottom-most shape, not used to clear the
    // target: the rasteriser keeps a cleared target out of blending, and a
    // layer that multiplies must multiply with the background too.
    if let Some(background) = frame.background {
        painter.ctx.set_paint(background.to_vello());
        painter
            .ctx
            .fill_rect(&Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
    }
    painter.pending = true;
    painter.nodes(&frame.nodes, Affine::IDENTITY)?;
    let rgba = match painter.finish()? {
        Drawn::Bytes(pixmap) => pixmap.take_rgba8(ImageAlphaType::Alpha),
        Drawn::Floats(surface) if options.dither => surface.into_pixels(accel)?.to_rgba8_dithered(),
        Drawn::Floats(Surface::Held(held)) => held_by(accel).download_rgba8(&held)?,
        Drawn::Floats(Surface::Cpu(pixels)) => pixels.to_rgba8(),
    };
    Image::new(width, height, rgba)
}

/// Whether `nodes` hold work `accel` does better than the CPU: an effect
/// it has a kernel for, or a projected layer, anywhere in them, the layers
/// effects read and mattes included.
fn has_work(accel: &dyn Accelerator, nodes: &[Node]) -> bool {
    nodes.iter().any(|node| {
        let Node::Layer(layer) = node else {
            return false;
        };
        layer.projection.is_some()
            || layer.pinned()
            || layer.effects.iter().any(|effect| {
                accel.runs(effect)
                    || effect
                        .input()
                        .is_some_and(|input| has_work(accel, std::slice::from_ref(&**input)))
            })
            || layer
                .matte
                .as_ref()
                .is_some_and(|matte| has_work(accel, std::slice::from_ref(&*matte.source)))
            || has_work(accel, &layer.children)
    })
}

/// What a painter drew: the rasteriser's 8-bit target when nothing it drew
/// made pixels of its own, or else the float canvas ([`Quality::Precise`]).
enum Drawn {
    /// As the rasteriser drew it.
    Bytes(Pixmap),
    /// The float canvas, on the CPU or in the accelerator.
    Floats(Surface),
}

impl Drawn {
    /// Its pixels, as floats, where they are; what the rasteriser drew is
    /// on the CPU.
    fn into_surface(self) -> Surface {
        match self {
            Drawn::Bytes(pixmap) => Surface::Cpu(Pixels::from_pixmap(&pixmap)),
            Drawn::Floats(surface) => surface,
        }
    }

    /// Its pixels, as floats, in `accel`: what the rasteriser drew is
    /// copied there as its bytes.
    fn into_held(self, accel: &dyn Accelerator) -> Result<Surface> {
        match self {
            Drawn::Bytes(pixmap) => accel
                .upload_bytes(bytes_of(&pixmap), pixmap.width(), pixmap.height())
                .map(Surface::Held),
            Drawn::Floats(mut surface) => {
                surface.held_mut(accel)?;
                Ok(surface)
            }
        }
    }

    /// Its pixels, as floats, where the float canvas's work is done: in
    /// `accel` when there is one, so what is laid, mixed and changed after
    /// stays there ([D74]), else on the CPU.
    ///
    /// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
    fn into_working(self, accel: Option<&dyn Accelerator>) -> Result<Surface> {
        match accel {
            Some(accel) => self.into_held(accel),
            None => Ok(self.into_surface()),
        }
    }

    /// Its pixels, as floats, on the CPU.
    fn into_pixels(self, accel: Option<&dyn Accelerator>) -> Result<Pixels> {
        self.into_surface().into_pixels(accel)
    }
}

/// A pixmap's premultiplied RGBA bytes, row by row.
fn bytes_of(pixmap: &Pixmap) -> &[u8] {
    pixmap.data_as_u8_slice()
}

/// Whether drawing `layer` lays pixels made by the rasteriser's own code
/// rather than the vello context: it adjusts what is below, is a plane, has
/// masks, motion blur of a drawing made once, or an effect that reads its
/// pixels, or blends by a mode only the float canvas lays, or a layer under
/// it does. On the float canvas such a layer is drawn alone and laid in
/// floats, and so is every layer it sits in.
fn makes_pixels(layer: &Layer) -> bool {
    layer.blend.laid_in_floats()
        || layer.adjust
        || layer.projection.is_some()
        || !layer.masks.is_empty()
        || !layer.motion.is_empty()
        || layer.effects.iter().any(Effect::reads_pixels)
        || layer.pinned()
        || children_make_pixels(layer)
}

/// Whether a layer among `layer`'s children makes pixels ([`makes_pixels`]).
fn children_make_pixels(layer: &Layer) -> bool {
    layer
        .children
        .iter()
        .any(|child| matches!(child, Node::Layer(layer) if makes_pixels(layer)))
}

/// Whether `layer`, or a layer under it, blends by a mode only the float
/// canvas lays ([`BlendMode::laid_in_floats`]). At either quality, such a
/// layer is drawn alone and laid in floats, and so is every layer it sits
/// in, so that it is laid on a canvas that holds what is below it.
fn laid_in_floats(layer: &Layer) -> bool {
    layer.blend.laid_in_floats() || children_laid_in_floats(layer)
}

/// Whether a layer among `layer`'s children is laid in floats
/// ([`laid_in_floats`]).
fn children_laid_in_floats(layer: &Layer) -> bool {
    layer
        .children
        .iter()
        .any(|child| matches!(child, Node::Layer(layer) if laid_in_floats(layer)))
}

/// Whether `layer`'s children are laid on a float canvas of their own:
/// with float colours (`floats`) when they make pixels
/// ([`makes_pixels`]), and at either quality when one blends by a mode
/// only the float canvas lays.
fn children_floated(layer: &Layer, floats: bool) -> bool {
    (floats && children_make_pixels(layer)) || children_laid_in_floats(layer)
}

/// Whether drawing `layer`'s content draws its children on a canvas of
/// their own, as [`Painter::content`] does: it is isolated, and moves its
/// content as pixels (effects that read them, masks that cut, motion blur
/// of a drawing made once); also when its children are laid on a float
/// canvas ([`children_floated`]). A pinned layer draws them on its plane
/// instead.
/// The frame's [`Reads`] counts these uses.
pub(crate) fn draws_children_alone(layer: &Layer, floats: bool) -> bool {
    if !layer.is_isolated() {
        return false;
    }
    let steps = Steps::of(layer, children_floated(layer, floats));
    if steps.on_pixels.iter().any(Effect::pins) {
        return false;
    }
    let cuts = layer.masks.iter().any(|m| m.mode != MaskMode::None);
    !steps.on_pixels.is_empty() || cuts || !steps.motion.is_empty()
}

/// One side of the image, which the rasteriser holds in 16 bits.
fn side(pixels: u32, name: &str) -> Result<u16> {
    match u16::try_from(pixels) {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("an image {name} of {pixels} pixels"),
        )
        .expected("1 to 65535 pixels")),
    }
}

/// The render context of one frame and what drawing it needs.
///
/// With [`Quality::Fast`] everything is drawn by the vello context, and
/// pixels effects make are laid into it as 8-bit pictures. With
/// [`Quality::Precise`] they are laid on a float canvas instead, the
/// painter's `canvas`: what the context drew is laid on it first, then the
/// context goes on drawing over a clear target. Laying source-over is
/// associative, so a run of paths and normal layers drawn by the context
/// and then laid gives what drawing them on the canvas would; a layer that
/// blends otherwise with what the canvas holds is drawn alone and laid in
/// floats.
struct Painter<'a> {
    ctx: RenderContext,
    resources: Resources,
    width: u16,
    height: u16,
    options: RenderOptions,
    /// The instruction set the rasteriser uses, chosen once per frame.
    level: Level,
    /// The float canvas, once something was laid on it.
    canvas: Option<Surface>,
    /// What the float canvas's work is handed to, if anything ([D74]).
    ///
    /// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
    accel: Option<&'a dyn Accelerator>,
    /// Whether the context drew anything since it was last laid on the
    /// canvas.
    pending: bool,
    /// How many layers are open in the context.
    open: usize,
    /// The drawings of read layers, shared by the frame's painters.
    reads: Arc<Reads>,
}

impl<'a> Painter<'a> {
    /// A painter for a `width` by `height` target, drawing with `options`
    /// at `level`, sharing the frame's `reads` and handing work to `accel`.
    fn new(
        width: u16,
        height: u16,
        options: RenderOptions,
        level: Level,
        reads: Arc<Reads>,
        accel: Option<&'a dyn Accelerator>,
    ) -> Painter<'a> {
        Painter {
            ctx: RenderContext::new_with(
                width,
                height,
                RenderSettings {
                    level,
                    num_threads: 0,
                },
            ),
            resources: Resources::new(),
            width,
            height,
            options,
            level,
            canvas: None,
            accel: accel.filter(|accel| accel.holds(width, height)),
            pending: false,
            open: 0,
            reads,
        }
    }

    /// A painter of this one's size and options, to draw something alone.
    fn alone(&self) -> Painter<'a> {
        Painter::new(
            self.width,
            self.height,
            self.options,
            self.level,
            Arc::clone(&self.reads),
            self.accel,
        )
    }

    /// Whether pixels are laid on a float canvas.
    fn floats(&self) -> bool {
        self.options.quality == Quality::Precise
    }

    /// Renders what the context drew into a cleared pixmap of the target's
    /// size.
    fn rendered(&mut self) -> Pixmap {
        self.ctx.flush();
        let mut pixmap = Pixmap::new(self.width, self.height);
        self.ctx.render_with(
            &mut pixmap,
            &mut self.resources,
            RasterizerSettings {
                render_mode: self.options.mode(),
                target_init: TargetInit::Clear(Color::TRANSPARENT.to_vello()),
                ..RasterizerSettings::default()
            },
        );
        pixmap
    }

    /// What was drawn: the float canvas with what the context drew since
    /// laid on it, or what the context drew when nothing was laid.
    fn finish(mut self) -> Result<Drawn> {
        match self.canvas.is_some() {
            true => {
                self.canvas_mut()?;
                Ok(Drawn::Floats(
                    self.canvas.take().expect("the canvas was laid on"),
                ))
            }
            false => Ok(Drawn::Bytes(self.rendered())),
        }
    }

    /// The float canvas, with what the context drew since laid on it; a
    /// clear one if nothing was. On a canvas held in the accelerator, what
    /// the context drew is copied there as its bytes.
    fn canvas_mut(&mut self) -> Result<&mut Surface> {
        debug_assert_eq!(self.open, 0, "the canvas is laid on outside layers");
        if self.pending {
            let rendered = Drawn::Bytes(self.rendered());
            self.ctx.reset();
            self.pending = false;
            match (&mut self.canvas, self.accel) {
                (Some(canvas), Some(accel)) if canvas.is_held() => {
                    let drawn = rendered.into_held(accel)?;
                    canvas.lay(
                        &drawn,
                        BlendMode::Normal,
                        &Amount::constant(1.0),
                        Some(accel),
                    )?;
                }
                (Some(canvas), accel) => {
                    let drawn = rendered.into_surface();
                    canvas.lay(&drawn, BlendMode::Normal, &Amount::constant(1.0), accel)?;
                }
                (None, accel) => self.canvas = Some(rendered.into_working(accel)?),
            }
        }
        if self.canvas.is_none() {
            self.canvas = Some(match self.accel {
                Some(accel) => Surface::Held(accel.transparent(self.width, self.height)?),
                None => Surface::Cpu(Pixels::transparent(self.width, self.height)),
            });
        }
        Ok(self.canvas.as_mut().expect("made above"))
    }

    /// What a layer that draws nothing does by `mode`: nothing, but for a
    /// stencil, which keeps nothing of what is below either.
    fn nothing_laid(&mut self, mode: BlendMode) -> Result<()> {
        if !mode.keeps_below() || mode.lay([1.0; 4], [0.0; 4]) == [1.0; 4] {
            return Ok(());
        }
        let clear = Surface::Cpu(Pixels::transparent(self.width, self.height));
        self.lay_on_canvas(&clear, mode, &Amount::constant(1.0))
    }

    /// Lays `source` on the float canvas by `mode`, scaled by `amount`.
    fn lay_on_canvas(
        &mut self,
        source: &Surface,
        mode: BlendMode,
        amount: &Amount<'_>,
    ) -> Result<()> {
        let accel = self.accel;
        self.canvas_mut()?.lay(source, mode, amount, accel)
    }

    /// The mask `matte` makes: its source drawn on its own, in coordinates
    /// `parent` maps to the frame, each pixel read as `matte.mode` says.
    fn mask(&self, matte: &Matte, parent: Affine) -> Result<Mask> {
        let data = self
            .matte_values(matte, parent)?
            .into_iter()
            .map(|v| (v * 255.0).round() as u8)
            .collect();
        Ok(Mask::from_parts(data, self.width, self.height))
    }

    /// The drawing of `matte`'s source alone, in coordinates `parent` maps
    /// to the frame, where the float canvas's work is done: drawn once a
    /// frame for each identity and transform ([`Reads`]).
    fn matte_drawn(&self, matte: &Matte, parent: Affine) -> Result<Surface> {
        let source = std::slice::from_ref(&*matte.source);
        let size = (self.width, self.height);
        self.reads.drawn(source, parent, size, self.accel, || {
            self.drawn_alone_surface(source, parent)
        })
    }

    /// How much of a layer `matte` lets show at each pixel, 0 to 1.
    fn matte_values(&self, matte: &Matte, parent: Affine) -> Result<Vec<f32>> {
        let pixels = self.matte_drawn(matte, parent)?.into_pixels(self.accel)?;
        Ok(matte_read(&pixels, matte.mode))
    }

    /// How much of a layer `matte` lets show at each pixel: read on the
    /// CPU, or, when its source's pixels are in the accelerator, read
    /// there as the layer is laid.
    fn matte_share(&self, matte: &Matte, parent: Affine) -> Result<Matted> {
        Ok(match self.matte_drawn(matte, parent)? {
            Surface::Held(held) => Matted::Held(held, matte.mode),
            Surface::Cpu(pixels) => Matted::Values(matte_read(&pixels, matte.mode)),
        })
    }

    /// How much of a layer its `masks` keep at each pixel, in coordinates
    /// `transform` maps to the frame; `None` when they cut nothing. Worked
    /// out in the accelerator when `held` and it holds the frame, so it
    /// cuts pixels held there without a copy back, else on the CPU.
    fn cut(&self, masks: &[LayerMask], transform: Affine, held: bool) -> Result<Option<Kept>> {
        let Some((start, steps)) = crate::mask::steps(masks, transform) else {
            return Ok(None);
        };
        let draw = |mask: &LayerMask| {
            let mut painter = self.alone();
            painter.ctx.set_transform(transform);
            painter.ctx.set_paint(Color::WHITE.to_vello());
            painter.ctx.set_fill_rule(vello_cpu::peniko::Fill::NonZero);
            painter.ctx.fill_path(&mask.path);
            painter.rendered()
        };
        let (width, height) = (self.width, self.height);
        match self.accel.filter(|a| held && a.holds(width, height)) {
            Some(accel) => {
                let mut kept = None;
                for (mask, step) in steps {
                    let drawn = draw(mask);
                    let mask = crate::mask::MaskCoverage {
                        drawn: bytes_of(&drawn),
                        step,
                    };
                    kept = Some(accel.mask(kept, start, &mask, width, height)?);
                }
                Ok(kept.map(Kept::Held))
            }
            None => {
                let mut kept = vec![start; usize::from(width) * usize::from(height)];
                for (mask, step) in steps {
                    let drawn = draw(mask);
                    let mask = crate::mask::MaskCoverage {
                        drawn: bytes_of(&drawn),
                        step,
                    };
                    crate::mask::combine_on_cpu(&mut kept, &mask, width, height);
                }
                Ok(Some(Kept::Values(kept)))
            }
        }
    }

    /// How much of each pixel of the grid lies inside `clip`, drawn in
    /// coordinates `transform` takes to the grid.
    fn coverage(&self, clip: &Clip, transform: Affine) -> Vec<f32> {
        let (path, rule) = clip.outline();
        let mut painter = self.alone();
        painter.ctx.set_transform(transform);
        painter.ctx.set_paint(Color::WHITE.to_vello());
        painter.ctx.set_fill_rule(rule.to_vello());
        painter.ctx.fill_path(&path);
        par::map(painter.rendered().data(), |_, p| f32::from(p.a) / 255.0)
    }

    /// Draws `nodes` on a canvas of their own and returns its pixels, on
    /// the CPU.
    fn drawn_alone(&self, nodes: &[Node], transform: Affine) -> Result<Pixels> {
        let mut painter = self.alone();
        painter.nodes(nodes, transform)?;
        painter.finish()?.into_pixels(self.accel)
    }

    /// Draws `nodes` on a canvas of their own and returns its pixels where
    /// the float canvas's work is done.
    fn drawn_alone_surface(&self, nodes: &[Node], transform: Affine) -> Result<Surface> {
        let mut painter = self.alone();
        painter.nodes(nodes, transform)?;
        painter.finish()?.into_working(self.accel)
    }

    /// Where `effect` runs on a layer whose own coordinates `transform`
    /// takes to the frame: with the drawing of the layer it reads, if any,
    /// laid on those coordinates.
    fn space(
        &self,
        effect: &Effect,
        transform: Affine,
        layer: &Layer,
        bounds: Option<Rect>,
    ) -> Result<Space> {
        let input = effect
            .input()
            .map(|node| {
                let bounds = match &**node {
                    Node::Layer(layer) => layer.bounds,
                    _ => None,
                };
                // Drawn once a frame for each transform it is read at:
                // left in the accelerator for an effect that runs there,
                // brought to the CPU for one that runs on the CPU.
                let nodes = std::slice::from_ref(&**node);
                let size = (self.width, self.height);
                let drawn = match self.accel.filter(|accel| accel.runs(effect)) {
                    Some(accel) => {
                        match self.reads.drawn(nodes, transform, size, Some(accel), || {
                            self.drawn_alone_surface(nodes, transform)
                        })? {
                            Surface::Held(held) => Drawing::Held(Arc::new(held)),
                            Surface::Cpu(pixels) => Drawing::Cpu(pixels),
                        }
                    }
                    None => Drawing::Cpu(
                        self.reads
                            .drawn(nodes, transform, size, self.accel, || {
                                self.drawn_alone(nodes, transform).map(Surface::Cpu)
                            })?
                            .into_pixels(self.accel)?,
                    ),
                };
                Ok(Input { drawn, bounds })
            })
            .transpose()?;
        Ok(Space {
            transform,
            bounds,
            input,
            time: layer.time,
            sound: effect.sound().cloned(),
            outline: effect.outline().cloned(),
            totals: effect.totals().to_vec(),
        })
    }

    /// Lays `pixels`, the size of the frame, over what is drawn so far: on
    /// the float canvas, or as a picture the context draws.
    fn draw_pixels(&mut self, pixels: Surface) -> Result<()> {
        if self.floats() {
            return self.lay_on_canvas(&pixels, BlendMode::Normal, &Amount::constant(1.0));
        }
        let pixels = pixels.into_pixels(self.accel)?;
        self.pending = true;
        let paint = vello_cpu::Image {
            image: ImageSource::Pixmap(Arc::new(pixels.to_pixmap())),
            // Pixel for pixel: the grid lines up with the frame's.
            sampler: ImageSampler::new().with_quality(ImageQuality::Low),
        };
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.reset_paint_transform();
        self.ctx.set_paint(paint);
        self.ctx.fill_rect(&Rect::new(
            0.0,
            0.0,
            f64::from(self.width),
            f64::from(self.height),
        ));
        Ok(())
    }

    /// Draws `nodes` back to front, in coordinates `transform` maps to the
    /// frame. A filter layer among them applies its effects to what the
    /// nodes before it drew.
    fn nodes(&mut self, nodes: &[Node], transform: Affine) -> Result<()> {
        let filter = nodes
            .iter()
            .rposition(|node| matches!(node, Node::Layer(layer) if layer.adjust));
        if let Some(at) = filter {
            let Node::Layer(filter) = &nodes[at] else {
                unreachable!("the position of a filter layer");
            };
            self.filter(&nodes[..at], filter, transform)?;
            return self.nodes(&nodes[at + 1..], transform);
        }
        nodes.iter().try_for_each(|node| match node {
            Node::Layer(layer) => self.layer(layer, transform),
            Node::Path(path) => {
                self.pending = true;
                self.path(path, transform);
                Ok(())
            }
            Node::Glyphs(glyphs) => {
                self.pending = true;
                self.glyphs(glyphs, transform)
            }
            Node::Image(image) => {
                self.pending = true;
                self.image(image, transform);
                Ok(())
            }
        })
    }

    /// Draws `below`, then `filter`'s effects on it: what they give is
    /// mixed in by the filter's opacity, its matte, and the alpha of what
    /// its children draw, its region; without children, everywhere.
    ///
    /// A projected filter's region is drawn on its plane, masks included,
    /// and laid onto the canvas; its effects run on the canvas, measured by
    /// the affine map that matches its projection where the canvas sees
    /// the middle of it.
    fn filter(&mut self, below: &[Node], filter: &Layer, parent: Affine) -> Result<()> {
        let drawn = self.drawn_alone_surface(below, parent)?;
        if filter.opacity <= 0.0 || filter.effects.is_empty() {
            return self.draw_pixels(drawn);
        }
        let (transform, region, cut) = match &filter.projection {
            None => {
                let transform = parent * filter.transform;
                // Where it draws, by its alpha; a filter layer drawing
                // nothing applies everywhere.
                let region = match filter.children.is_empty() {
                    true => None,
                    false => Some(self.drawn_alone_surface(&filter.children, transform)?),
                };
                let cut = self.cut(&filter.masks, transform, drawn.is_held())?;
                (transform, region, cut)
            }
            Some(projection) => {
                let maps = [Homography::from(parent) * projection.matrix];
                let extent = match (filter.children.is_empty(), filter.bounds) {
                    (false, _) => projected::nodes_extent(&filter.children),
                    (true, Some(bounds)) => Extent::Box(bounds),
                    (true, None) => Extent::Everywhere,
                };
                let reach = projected::mask_reach(&filter.masks);
                let Some(plan) = Plan::new(extent, Some(reach), &maps, self.width, self.height)
                else {
                    // None of it is seen, so it applies nowhere.
                    return self.draw_pixels(drawn);
                };
                let region = self.laid(&plan, &maps, |sub| {
                    let mut pixels = match filter.children.is_empty() {
                        true => {
                            let mut whole = sub.alone();
                            whole.ctx.set_transform(plan.local);
                            whole.ctx.set_paint(Color::WHITE.to_vello());
                            whole.ctx.fill_rect(&filter.bounds.unwrap_or(plan.region));
                            Pixels::from_pixmap(&whole.rendered())
                        }
                        false => sub.drawn_alone(&filter.children, plan.local)?,
                    };
                    if let Some(Kept::Values(cut)) = sub.cut(&filter.masks, plan.local, false)? {
                        par::each_with(pixels.data_mut(), &cut, |_, pixel, &kept| {
                            *pixel = pixel.map(|c| c * kept);
                        });
                    }
                    sub.draw_pixels(Surface::Cpu(pixels))
                })?;
                let transform = maps[0]
                    .tangent(plan.seen)
                    .unwrap_or(parent * filter.transform);
                (transform, Some(region), None)
            }
        };
        let mut changed = drawn.duplicate(self.accel)?;
        let mut bounds = filter.bounds;
        for effect in &filter.effects {
            let space = self.space(effect, transform, filter, bounds)?;
            changed.effect(effect, &space, self.accel)?;
            bounds = bounds.map(|b| effect.grown(b));
        }
        let matte = filter
            .matte
            .as_ref()
            .map(|matte| self.matte_share(matte, parent))
            .transpose()?;
        // The region's alpha, read on the CPU when its pixels are there.
        let region_alpha = match &region {
            Some(Surface::Cpu(pixels)) => {
                Some(pixels.data().iter().map(|p| p[3]).collect::<Vec<f32>>())
            }
            _ => None,
        };
        let region_share = match (&region, &region_alpha) {
            (Some(Surface::Held(held)), _) => Some(Share::Alpha(held)),
            (_, Some(alpha)) => Some(Share::Values(alpha.as_slice())),
            _ => None,
        };
        let amount = [
            cut.as_ref().map(Kept::share),
            matte.as_ref().map(Matted::share),
            region_share,
        ]
        .into_iter()
        .flatten()
        .fold(Amount::constant(filter.opacity), Amount::and);
        let mut mixed = drawn;
        mixed.mix(&changed, &amount, self.accel)?;
        drop(amount);
        self.draw_pixels(mixed)
    }

    /// Draws a layer: its children, in a buffer of their own when it is
    /// isolated, then its effects on that buffer, then lays it on what is
    /// below with its opacity, blend mode, clip and matte. A projected
    /// layer's buffer is in its own coordinates and laid through its
    /// projection.
    fn layer(&mut self, layer: &Layer, parent: Affine) -> Result<()> {
        let steps = Steps::of(layer, children_floated(layer, self.floats()));
        // A transparent layer draws nothing, effects included.
        let opacity = layer.opacity * steps.moved.opacity;
        if opacity <= 0.0 {
            return self.nothing_laid(layer.blend);
        }
        if let Some(projection) = &layer.projection {
            return self.projected(layer, projection, &steps, opacity, parent, 0.0);
        }
        if let Some(flat) = self.whole_box(layer, parent) {
            return self.projected(layer, &flat, &steps, opacity, parent, 0.0);
        }
        let transform = parent * layer.transform;
        if !layer.is_isolated() {
            return self.nodes(&layer.children, transform);
        }
        // Precise colours draw a shrunk layer whose effects read a few px
        // at its own size, as a plane laid down through its transform
        // ([D76]), when it is laid on the float canvas itself rather than
        // inside a layer the context holds open.
        let on_canvas = self.floats() && self.open == 0;
        if let Some(own) = projected::whole_scale(layer, parent).filter(|_| on_canvas) {
            let flat = Projection::new(Homography::from(layer.transform));
            return self.projected(layer, &flat, &steps, opacity, parent, own);
        }
        // On the float canvas, a layer that makes pixels is laid in floats,
        // and so is one that blends otherwise than source-over with what
        // the canvas holds; one inside a layer open in the context blends
        // with that layer's buffer, which the context holds. A layer that
        // blends by a mode only the canvas lays, or holds one, is laid in
        // floats at either quality: every layer it sits in is too, so it
        // is never inside a layer open in the context.
        let blends_with_canvas =
            self.canvas.is_some() && self.open == 0 && layer.blend != BlendMode::Normal;
        let floated = laid_in_floats(layer);
        debug_assert!(
            !floated || self.open == 0,
            "a layer laid in floats sits in no layer open in the context"
        );
        if (self.floats() && makes_pixels(layer)) || blends_with_canvas || floated {
            return self.floated(layer, &steps, opacity, parent);
        }
        self.pending = true;
        // The matte is drawn in the coordinates the layer's parent gives, as
        // the layer's siblings are; the rasteriser applies masks untransformed.
        let mask = layer
            .matte
            .as_ref()
            .map(|matte| self.mask(matte, parent))
            .transpose()?;
        self.ctx.set_transform(transform);
        let clip = layer.shown_clip().as_ref().map(Clip::outline);
        if let Some((_, rule)) = &clip {
            self.ctx.set_fill_rule(rule.to_vello());
        }
        self.ctx.push_layer(
            clip.as_ref().map(|(path, _)| path),
            Some(layer.blend.to_vello()),
            Some(opacity),
            mask,
            None,
        );
        self.open += 1;
        let drawn = self.content(layer, &steps, transform, parent);
        // Pop even after a failure, so the context's layers stay balanced.
        self.ctx.pop_layer();
        self.open -= 1;
        drawn
    }

    /// The plane a flat layer is drawn on when its effects read anywhere in
    /// its box and the box leaves the canvas: its own transform, as a
    /// projection, so it draws on a buffer of its whole box as a projected
    /// layer does. On the canvas's grid, the part of the box off the canvas
    /// would never be drawn, and an effect that reads it (a polar
    /// conversion, a wrap, a lens) would read nothing there. `None` for any
    /// other layer, and for one with motion-blur moves, which a projected
    /// layer takes as samples of its projection instead.
    fn whole_box(&self, layer: &Layer, parent: Affine) -> Option<Projection> {
        if !layer.motion.is_empty() || projected::grid_reach(layer).is_some() {
            return None;
        }
        let landed = (parent * layer.transform).transform_rect_bbox(layer.bounds?);
        let canvas = Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height));
        let inside = landed.union(canvas) == canvas;
        (!inside).then(|| Projection::new(Homography::from(layer.transform)))
    }

    /// Draws an isolated flat layer on the float canvas: its content and
    /// effects alone, then laid in floats with its opacity, clip, matte
    /// and blend mode, as the context would lay its buffer.
    fn floated(
        &mut self,
        layer: &Layer,
        steps: &Steps<'_>,
        opacity: f32,
        parent: Affine,
    ) -> Result<()> {
        let transform = parent * layer.transform;
        let mut alone = self.alone();
        alone.content(layer, steps, transform, parent)?;
        let drawn = alone.finish()?.into_working(self.accel)?;
        let clip = layer
            .shown_clip()
            .map(|clip| self.coverage(&clip, transform));
        let matte = layer
            .matte
            .as_ref()
            .map(|matte| self.matte_share(matte, parent))
            .transpose()?;
        let amount = [
            clip.as_deref().map(Share::Values),
            matte.as_ref().map(Matted::share),
        ]
        .into_iter()
        .flatten()
        .fold(Amount::constant(opacity), Amount::and);
        self.lay_on_canvas(&drawn, layer.blend, &amount)
    }

    /// Draws a projected layer ([D58]): its clipped content, masks and
    /// effects on a buffer of its own in its own coordinates, laid onto the
    /// canvas through each of its maps, then blended with `opacity`, its
    /// blend mode and its matte, as a flat layer is.
    ///
    /// [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
    fn projected(
        &mut self,
        layer: &Layer,
        projection: &Projection,
        steps: &Steps<'_>,
        opacity: f32,
        parent: Affine,
        least: f64,
    ) -> Result<()> {
        if !layer.motion.is_empty() {
            return Err(Diagnostic::new(
                Kind::Conflict,
                "a projected layer also has affine motion-blur moves",
            )
            .expected("motion blur of a projected layer as `Projection::samples`"));
        }
        let to_canvas = Homography::from(parent);
        let maps: Vec<Homography> = match projection.samples.is_empty() {
            true => vec![to_canvas * projection.matrix],
            false => projection.samples.iter().map(|s| to_canvas * *s).collect(),
        };
        let extent = projected::extent(layer);
        let reach = projected::read_reach(layer);
        let Some(plan) = Plan::at_least(extent, reach, &maps, self.width, self.height, least)
        else {
            return self.nothing_laid(layer.blend);
        };
        let clip = layer.shown_clip();
        let laid = self.laid(&plan, &maps, |sub| {
            sub.on_plane(layer, steps, clip.as_ref(), plan.local)
        })?;
        if self.floats() || layer.blend.laid_in_floats() {
            let matte = layer
                .matte
                .as_ref()
                .map(|matte| self.matte_share(matte, parent))
                .transpose()?;
            let amount = matte
                .as_ref()
                .map(Matted::share)
                .into_iter()
                .fold(Amount::constant(opacity), Amount::and);
            return self.lay_on_canvas(&laid, layer.blend, &amount);
        }
        let mask = layer
            .matte
            .as_ref()
            .map(|matte| self.mask(matte, parent))
            .transpose()?;
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.push_layer(
            None,
            Some(layer.blend.to_vello()),
            Some(opacity),
            mask,
            None,
        );
        let drawn = self.draw_pixels(laid);
        self.ctx.pop_layer();
        drawn
    }

    /// Draws `layer`'s content with `steps` on this painter, the buffer of
    /// a plane, in coordinates `local` maps to its px, cut to `clip`.
    fn on_plane(
        &mut self,
        layer: &Layer,
        steps: &Steps<'_>,
        clip: Option<&Clip>,
        local: Affine,
    ) -> Result<()> {
        // The buffer holds the layer alone, so only a clip needs a layer
        // of its own in it: what effects paint over the content composes
        // with the buffer's own px either way.
        let Some((path, rule)) = clip.map(Clip::outline) else {
            return self.content(layer, steps, local, Affine::IDENTITY);
        };
        if self.floats() && makes_pixels(layer) {
            // Its content is laid on the buffer's float canvas, cut to the
            // clip there.
            let mut alone = self.alone();
            alone.content(layer, steps, local, Affine::IDENTITY)?;
            let drawn = alone.finish()?.into_working(self.accel)?;
            let clip = self.coverage(&Clip::Path(path, rule), local);
            let amount = Amount::constant(1.0).and(Share::Values(&clip));
            return self.lay_on_canvas(&drawn, BlendMode::Normal, &amount);
        }
        self.pending = true;
        self.ctx.set_transform(local);
        self.ctx.set_fill_rule(rule.to_vello());
        self.ctx.push_layer(Some(&path), None, None, None, None);
        self.open += 1;
        let drawn = self.content(layer, steps, local, Affine::IDENTITY);
        self.ctx.pop_layer();
        self.open -= 1;
        drawn
    }

    /// Draws a layer's content whose effect `steps.on_pixels[at]` pins it
    /// onto four corners ([D89]), in coordinates `transform` maps to this
    /// painter's grid. Its content, masks and the effects before the pin
    /// are drawn on a plane of their own, at the most the pin magnifies
    /// them, and laid onto the grid through `transform` and the pin's map,
    /// as a projected layer is. The effects after the pin then run on what
    /// was laid, with the box the pin grew. With motion blur, the pin is
    /// laid through each moment's map, paired with the layer's move then
    /// when nothing follows it; when effects follow it, what they drew is
    /// laid through each moment's map relative to the frame's.
    ///
    /// [D89]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography
    fn pinned(
        &mut self,
        layer: &Layer,
        steps: &Steps<'_>,
        at: usize,
        transform: Affine,
        parent: Affine,
    ) -> Result<()> {
        let pin = &steps.on_pixels[at];
        let after = &steps.on_pixels[at + 1..];
        let before_effects = &layer.effects[..steps.drawing.len() + at];
        let before = Steps {
            drawing: steps.drawing,
            on_pixels: &steps.on_pixels[..at],
            moved: steps.moved,
            motion: &[],
            grows: crate::list::grow(before_effects),
        };
        // The box it pins: its own, the layer's, or what the layer draws.
        let from = pin.pin_box(layer.bounds).or_else(|| {
            match projected::extent_of(layer, before_effects) {
                Extent::Box(b) => Some(b),
                Extent::Nothing | Extent::Everywhere => None,
            }
        });
        let Some(from) = from else {
            return Ok(());
        };
        let frame = pin.pin_map(from);
        let moments = pin.pin_moments(from);
        let moves = steps.motion;
        if !moves.is_empty() && !moments.is_empty() && moves.len() != moments.len() {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!(
                    "a corner pin with {} moments on a layer with {} moves",
                    moments.len(),
                    moves.len()
                ),
            )
            .expected("as many moments of the pin as motion-blur moves of its layer"));
        }
        let to_grid = Homography::from(transform);
        let count = moves.len().max(moments.len());
        // The layer's own coordinates to the grid at moment `k`.
        let at_moment = |k: usize| -> Option<Homography> {
            let pin_k = match moments.is_empty() {
                true => frame,
                false => moments[k],
            }?;
            let move_k = moves.get(k).map_or(Homography::IDENTITY, |m| {
                Homography::from(parent * *m * parent.inverse())
            });
            Some(move_k * to_grid * pin_k)
        };
        let blurred = count > 0;
        let maps: Vec<Homography> = match blurred && after.is_empty() {
            true => (0..count).filter_map(at_moment).collect(),
            false => frame.map(|map| to_grid * map).into_iter().collect(),
        };
        let extent = projected::extent_of(layer, before_effects);
        let reach = projected::reach_of(layer, before_effects);
        let clip = layer.clip_after(before_effects);
        let mut pixels = match Plan::pinned(extent, reach, &maps, self.width, self.height) {
            Some(plan) => self.laid(&plan, &maps, |sub| {
                sub.on_plane(layer, &before, clip.as_ref(), plan.local)
            })?,
            None => match (self.accel, self.floats()) {
                (Some(accel), true) => Surface::Held(accel.transparent(self.width, self.height)?),
                _ => Surface::Cpu(Pixels::transparent(self.width, self.height)),
            },
        };
        // Each effect after it sees the box it and those before it grew.
        let mut bounds = layer.bounds.map(|b| {
            layer.effects[..=steps.drawing.len() + at]
                .iter()
                .fold(b, |b, e| e.grown(b))
        });
        for effect in after {
            let space = self.space(effect, transform, layer, bounds)?;
            pixels.effect(effect, &space, self.accel)?;
            bounds = bounds.map(|b| effect.grown(b));
        }
        if blurred && !after.is_empty() {
            let back = frame.and_then(|map| (to_grid * map).inverse());
            let on_grid: Vec<Homography> = (0..count)
                .filter_map(|k| Some(at_moment(k)? * back?))
                .collect();
            pixels = pixels.warped(&on_grid, self.accel)?;
        }
        self.draw_pixels(pixels)
    }

    /// What `draw` paints on a painter of `plan`'s buffer, laid onto this
    /// painter's grid through each of `maps` and averaged: in the
    /// accelerator when the float canvas's work is done there.
    fn laid(
        &self,
        plan: &Plan,
        maps: &[Homography],
        draw: impl FnOnce(&mut Painter<'a>) -> Result<()>,
    ) -> Result<Surface> {
        let mut sub = Painter::new(
            plan.width,
            plan.height,
            self.options,
            self.level,
            Arc::clone(&self.reads),
            self.accel,
        );
        draw(&mut sub)?;
        let (width, height) = (self.width, self.height);
        let landings = plan.landings(maps);
        let share = 1.0 / maps.len() as f32;
        let drawn = sub.finish()?;
        if let (Some(accel), true) = (self.accel, self.floats()) {
            let texels = match &drawn {
                Drawn::Bytes(pixmap) => Texels::Bytes {
                    rgba: bytes_of(pixmap),
                    width: pixmap.width(),
                    height: pixmap.height(),
                },
                Drawn::Floats(Surface::Held(held)) => Texels::Held(held),
                Drawn::Floats(Surface::Cpu(pixels)) => Texels::Floats(pixels),
            };
            return accel
                .project(texels, &landings, share, width, height)
                .map(Surface::Held);
        }
        Ok(Surface::Cpu(match drawn {
            Drawn::Bytes(pixmap) => projected::lay(
                Buffer::Bytes(bytes_of(&pixmap), pixmap.width(), pixmap.height()),
                &landings,
                share,
                width,
                height,
            ),
            Drawn::Floats(pixels) => {
                let pixels = pixels.into_pixels(self.accel)?;
                projected::lay(Buffer::Floats(&pixels), &landings, share, width, height)
            }
        }))
    }

    /// Draws a layer's content inside the buffer it was given, in
    /// coordinates `transform` maps to this painter's grid: its children,
    /// then its effects. Moves before the first effect that reads pixels
    /// apply as the children are drawn; from that effect on, the children
    /// are drawn on a canvas of their own and every effect runs in order
    /// on its pixels. `parent` places its motion-blur moves.
    fn content(
        &mut self,
        layer: &Layer,
        steps: &Steps<'_>,
        transform: Affine,
        parent: Affine,
    ) -> Result<()> {
        if let Some(at) = steps.on_pixels.iter().position(Effect::pins) {
            return self.pinned(layer, steps, at, transform, parent);
        }
        let moved = steps.moved.transform;
        let cuts = layer.masks.iter().any(|m| m.mode != MaskMode::None);
        if steps.on_pixels.is_empty() && !cuts && steps.motion.is_empty() {
            self.pending = true;
            let drawn = self.nodes(&layer.children, transform * moved);
            let mut effects = EffectContext {
                ctx: &mut self.ctx,
                width: self.width,
                height: self.height,
                bounds: layer.bounds.map(|b| transform * b.to_path(0.1)),
            };
            for effect in steps.drawing {
                effect.paint_over(&mut effects);
            }
            return drawn;
        }
        let at = transform * moved;
        // Children that draw the same, in this layer or another, are drawn
        // once at each transform ([`Reads`]). What they drew goes to the
        // accelerator as drawn when the first effect runs there, or when no
        // effect or motion blur needs it on the CPU before it is laid.
        let held = self.accel.filter(|accel| match steps.on_pixels.first() {
            Some(effect) => accel.runs(effect),
            None => steps.motion.is_empty(),
        });
        let size = (self.width, self.height);
        let mut pixels = self
            .reads
            .drawn(&layer.children, at, size, self.accel, || {
                let mut painter = self.alone();
                painter.nodes(&layer.children, at)?;
                match held {
                    Some(accel) => painter.finish()?.into_held(accel),
                    None => Ok(painter.finish()?.into_surface()),
                }
            })?;
        // An effect that draws beyond the box grows the clip the layer
        // shows within, so the content is cut to its own clip first.
        let clipped = match (&layer.clip, steps.grows) {
            (Some(clip), true) => Some(self.coverage(clip, transform)),
            _ => None,
        };
        match (self.cut(&layer.masks, at, pixels.is_held())?, self.accel) {
            (Some(Kept::Held(kept)), Some(accel)) => pixels.cut_by(&kept, accel)?,
            (Some(Kept::Values(kept)), _) => pixels.cut(&kept, self.accel)?,
            _ => {}
        }
        if let Some(clipped) = &clipped {
            pixels.cut(clipped, self.accel)?;
        }
        // Each effect sees the box the ones before it grew.
        let mut bounds = layer.bounds;
        for effect in steps.on_pixels {
            let space = self.space(effect, transform, layer, bounds)?;
            pixels.effect(effect, &space, self.accel)?;
            bounds = bounds.map(|b| effect.grown(b));
        }
        if !steps.motion.is_empty() {
            let drawn = pixels.into_pixels(self.accel)?;
            pixels = Surface::Cpu(moved_average(&drawn, steps.motion, parent));
        }
        self.draw_pixels(pixels)
    }

    fn path(&mut self, draw: &PathDraw, transform: Affine) {
        self.ctx.set_transform(transform);
        self.set_paint(&draw.paint);
        match &draw.style {
            DrawStyle::Fill(rule) => {
                self.ctx.set_fill_rule(rule.to_vello());
                self.ctx.fill_path(&draw.path);
            }
            DrawStyle::Stroke(stroke) => {
                self.ctx.set_stroke(stroke.to_vello());
                self.ctx.stroke_path(&draw.path);
            }
        }
        self.ctx.reset_paint_transform();
    }

    /// Paints what is drawn next with `paint`, placed in the coordinates
    /// of the leaf drawn; the caller resets the paint transform after.
    fn set_paint(&mut self, paint: &crate::style::Paint) {
        let (paint, placed) = paint.to_vello();
        self.ctx.set_paint_transform(placed);
        self.ctx.set_paint(paint);
    }

    /// Fills the draw's rectangle with its picture, whose pixels the paint
    /// transform stretches over it, sampled from the copy that suits the
    /// size it lands at.
    fn image(&mut self, draw: &ImageDraw, transform: Affine) {
        let rect = draw.rect;
        let stretch = |width: u32, height: u32| {
            Affine::translate((rect.x0, rect.y0))
                * Affine::scale_non_uniform(
                    rect.width() / f64::from(width),
                    rect.height() / f64::from(height),
                )
        };
        // How far one picture pixel reaches on the target, along whichever
        // of its sides is drawn longer.
        let [a, b, c, d, _, _] =
            (transform * stretch(draw.picture.width(), draw.picture.height())).as_coeffs();
        let (paint, width, height) = draw.picture.paint(a.hypot(b).max(c.hypot(d)));
        self.ctx.set_transform(transform);
        self.ctx.set_paint_transform(stretch(width, height));
        self.ctx.set_paint(paint);
        self.ctx.fill_rect(&rect);
        // Later paints are placed by their geometry alone.
        self.ctx.reset_paint_transform();
    }

    fn glyphs(&mut self, draw: &GlyphDraw, transform: Affine) -> Result<()> {
        self.ctx.set_transform(transform);
        self.set_paint(&draw.paint);
        match &draw.style {
            DrawStyle::Fill(rule) => self.ctx.set_fill_rule(rule.to_vello()),
            DrawStyle::Stroke(stroke) => self.ctx.set_stroke(stroke.to_vello()),
        }
        let mut run = self
            .ctx
            .glyph_run(&mut self.resources, &draw.font)
            .font_size(draw.size)
            .hint(false)
            .normalized_coords(&draw.coords);
        if let Some(glyph_transform) = draw.glyph_transform {
            run = run.glyph_transform(glyph_transform);
        }
        let glyphs = draw.glyphs.iter().map(|g| vello_cpu::Glyph {
            id: g.id,
            x: g.x,
            y: g.y,
        });
        let drawn = match &draw.style {
            DrawStyle::Fill(_) => run.fill_glyphs(glyphs),
            DrawStyle::Stroke(_) => run.stroke_glyphs(glyphs),
        };
        self.ctx.reset_paint_transform();
        drawn.map_err(|error| {
            Diagnostic::new(Kind::Invalid, format!("glyphs could not be drawn: {error}"))
                .expected("glyph ids the font holds")
        })
    }
}

/// How much of a layer `pixels`, a matte's source, lets show at each
/// pixel, 0 to 1, read as `mode` says.
fn matte_read(pixels: &Pixels, mode: MatteMode) -> Vec<f32> {
    par::map(pixels.data(), |_, &[r, g, b, a]| {
        // Premultiplied, so transparent pixels read as black; Rec.
        // 709 weights, as the CSS masking module uses.
        let luma = || (r * 0.2126 + g * 0.7152 + b * 0.0722).clamp(0.0, 1.0);
        let a = a.clamp(0.0, 1.0);
        match mode {
            MatteMode::Alpha => a,
            MatteMode::AlphaInverted => 1.0 - a,
            MatteMode::Luma => luma(),
            MatteMode::LumaInverted => 1.0 - luma(),
        }
    })
}

/// What a layer's masks keep of it at each pixel: worked out on the CPU,
/// one value a pixel, or in the accelerator, in every channel of a pixel.
enum Kept {
    Values(Vec<f32>),
    Held(crate::accel::Resident),
}

impl Kept {
    /// What it scales an amount by.
    fn share(&self) -> Share<'_> {
        match self {
            Kept::Values(values) => Share::Values(values),
            Kept::Held(held) => Share::Alpha(held),
        }
    }
}

/// A matte, read: its values on the CPU, or its source's pixels in the
/// accelerator with the mode to read them by.
enum Matted {
    /// Read on the CPU.
    Values(Vec<f32>),
    /// To be read in the accelerator.
    Held(crate::accel::Resident, MatteMode),
}

impl Matted {
    /// The matte as a share of an amount.
    fn share(&self) -> Share<'_> {
        match self {
            Matted::Values(values) => Share::Values(values),
            Matted::Held(held, mode) => Share::Matte(held, *mode),
        }
    }
}

/// How a layer's effects split between drawing its content and running on
/// its pixels after.
struct Steps<'a> {
    /// The effects applied as the content is drawn: moves, and changes
    /// painted over it.
    drawing: &'a [Effect],
    /// The effects run in order on the content's pixels.
    on_pixels: &'a [Effect],
    /// What the moves among `drawing` do together.
    moved: Move,
    /// The layer's motion-blur moves of a drawing made once, in its
    /// parent's coordinates; none for a drawing laid once.
    motion: &'a [Affine],
    /// Whether an effect among them draws beyond the layer's box, so its
    /// content is cut to its clip before they run.
    grows: bool,
}

impl Steps<'_> {
    /// How `layer`'s effects split. Masks cut the content before any
    /// effect but moves, on its pixels, and a drawing moved for motion
    /// blur is made on pixels too, so with either only the moves before
    /// the first effect that reads pixels apply as it is drawn.
    /// With `floated_children`, its children make pixels on the float
    /// canvas ([`makes_pixels`]), which the context cannot paint over, so
    /// its effects run on its pixels too.
    fn of(layer: &Layer, floated_children: bool) -> Steps<'_> {
        // A corner pin lays what came before it as a plane, which is made
        // of pixels, as an effect that reads them is.
        let reads = layer
            .effects
            .iter()
            .position(|e| e.reads_pixels() || e.pins())
            .unwrap_or(layer.effects.len());
        let on_canvas = !layer.masks.is_empty() || !layer.motion.is_empty() || floated_children;
        let (drawing, on_pixels) = match reads < layer.effects.len() || on_canvas {
            true => {
                let moves = layer.effects[..reads]
                    .iter()
                    .take_while(|e| e.movement().is_some())
                    .count();
                layer.effects.split_at(moves)
            }
            false => (layer.effects.as_slice(), &[][..]),
        };
        // Moving effects apply as the content is drawn, in the order they
        // stand: each moves what the ones before it moved.
        let moved = drawing.iter().filter_map(Effect::movement).fold(
            Move {
                transform: Affine::IDENTITY,
                opacity: 1.0,
            },
            |done, next| Move {
                transform: next.transform * done.transform,
                opacity: done.opacity * next.opacity,
            },
        );
        Steps {
            drawing,
            on_pixels,
            moved,
            motion: &layer.motion,
            grows: layer.effects_grow(),
        }
    }
}

/// `pixels` laid at each of `moves`, given in the coordinates `parent`
/// takes to the frame, and averaged: motion blur of a drawing made once.
/// Only the pixels where a move can bring something are worked out.
fn moved_average(pixels: &Pixels, moves: &[Affine], parent: Affine) -> Pixels {
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let mut sum = Pixels::transparent(pixels.width(), pixels.height());
    let Some(drawn) = drawn_box(pixels) else {
        return sum;
    };
    let share = 1.0 / moves.len() as f32;
    for &step in moves {
        let to_frame = parent * step * parent.inverse();
        let back = to_frame.inverse();
        // Where the drawn box lands, a pixel wider for the bilinear reach.
        let landed = to_frame.transform_rect_bbox(drawn).inflate(1.0, 1.0);
        let xs =
            (landed.x0.floor().max(0.0) as usize)..(landed.x1.ceil().min(width as f64) as usize);
        let ys =
            (landed.y0.floor().max(0.0) as usize)..(landed.y1.ceil().min(height as f64) as usize);
        if ys.is_empty() || xs.is_empty() {
            continue;
        }
        let rows = &mut sum.data_mut()[ys.start * width..ys.end * width];
        par::rows(
            rows,
            width,
            || (),
            |row, data, ()| {
                let y = ys.start + row;
                for x in xs.clone() {
                    let from = back * Point::new(x as f64 + 0.5, y as f64 + 0.5);
                    let pixel = pixels.sample(from.x, from.y);
                    let total = &mut data[x];
                    for c in 0..4 {
                        total[c] += pixel[c] * share;
                    }
                }
            },
        );
    }
    sum
}

/// The box of the pixels of `pixels` that are not clear, in pixels of the
/// grid, or `None` when every one is.
pub(crate) fn drawn_box(pixels: &Pixels) -> Option<Rect> {
    let width = usize::from(pixels.width());
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, pixel) in pixels.data().iter().enumerate() {
        if pixel[3] > 0.0 {
            let (x, y) = (i % width, i / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x0 < x1).then(|| Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64))
}

#[cfg(test)]
#[path = "../tests/unit/render.rs"]
mod tests;
