//! Compositing inside an effect: how an effect's result, such as a glow's
//! light, a generator's colours or a solid, combines with the layer's own
//! pixels, one pixel at a time. The ways it may combine are one table,
//! [`OPERATIONS`], that effects pick from by name.
//!
//! Both sides are premultiplied [`Pixel`]s. A blend mode lays the result
//! over exactly as the float canvas lays a layer by that mode
//! ([`crate::BLEND_MODES`]): the straight colours blended, then laid over
//! the way the compositing specification of the web does it, so where only
//! one side covers a pixel that side shows, and where both do the blend of
//! the two; `add` is the plain sum of both premultiplied pixels. The other
//! operations do not blend colours: they keep one side, cut one side by
//! the other's alpha or luma, or add alphas.
//!
//! Effects run on pixels they already hold, on a canvas of their own
//! ([D49]); this is how what they make meets those pixels.
//!
//! [D49]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d49-effects-that-read-pixels-run-on-a-canvas-of-their-own

use nakkash_format::{Diagnostic, Result};

use crate::blend::BlendMode;
use crate::colour::LUMA;
use crate::pixels::{Pixel, Pixels};

/// How an effect's result (`over`) combines with the pixels under it
/// (`under`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// The result alone: what was under it is gone.
    Replace,
    /// The colours blended by a blend mode, then laid over.
    Blend(BlendMode),
    /// The result shows, opaque, at a share of the pixels as large as its
    /// alpha, scattered by a fixed pattern; what is under it shows at the
    /// others.
    Dissolve,
    /// What is under it, kept where the result has alpha.
    StencilAlpha,
    /// What is under it, kept as far as the result is bright.
    StencilLuma,
    /// What is under it, cut out where the result has alpha.
    SilhouetteAlpha,
    /// What is under it, cut out as far as the result is bright.
    SilhouetteLuma,
    /// The result's premultiplied colours laid over as they are, without
    /// holding them to its alpha first, so light past its alpha adds.
    PremultipliedOver,
    /// The alphas add, so two edges that cover a pixel between them close
    /// it; the result covers what it can and what is under it fills the
    /// rest.
    AlphaAdd,
}

/// One operation and the name scenes give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationSpec {
    /// The name.
    pub name: &'static str,
    /// The operation.
    pub operation: Operation,
}

const fn op(name: &'static str, operation: Operation) -> OperationSpec {
    OperationSpec { name, operation }
}

const fn blend(name: &'static str, mode: BlendMode) -> OperationSpec {
    op(name, Operation::Blend(mode))
}

/// Every operation by name. A blend mode keeps the name of its layer blend
/// mode. The first row of an operation gives its name.
pub const OPERATIONS: &[OperationSpec] = &[
    op("replace", Operation::Replace),
    blend("normal", BlendMode::Normal),
    // The name generators gave the normal mode before they had others.
    blend("over", BlendMode::Normal),
    blend("add", BlendMode::Add),
    blend("multiply", BlendMode::Multiply),
    blend("screen", BlendMode::Screen),
    blend("overlay", BlendMode::Overlay),
    blend("soft-light", BlendMode::SoftLight),
    blend("hard-light", BlendMode::HardLight),
    blend("color-dodge", BlendMode::ColorDodge),
    blend("color-burn", BlendMode::ColorBurn),
    blend("darken", BlendMode::Darken),
    blend("lighten", BlendMode::Lighten),
    blend("difference", BlendMode::Difference),
    blend("exclusion", BlendMode::Exclusion),
    blend("hue", BlendMode::Hue),
    blend("saturation", BlendMode::Saturation),
    blend("color", BlendMode::Color),
    blend("luminosity", BlendMode::Luminosity),
    op("dissolve", Operation::Dissolve),
    op("stencil-alpha", Operation::StencilAlpha),
    op("stencil-luma", Operation::StencilLuma),
    op("silhouette-alpha", Operation::SilhouetteAlpha),
    op("silhouette-luma", Operation::SilhouetteLuma),
    op("premultiplied-over", Operation::PremultipliedOver),
    op("alpha-add", Operation::AlphaAdd),
];

impl Operation {
    /// The operation called `name` in [`OPERATIONS`].
    ///
    /// # Errors
    /// An unknown-name diagnostic listing every name.
    pub fn named(name: &str) -> Result<Operation> {
        OPERATIONS
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.operation)
            .ok_or_else(|| {
                Diagnostic::unknown("operation", name, OPERATIONS.iter().map(|s| s.name))
            })
    }

    /// The operation's name: that of its first row in [`OPERATIONS`].
    pub fn name(self) -> &'static str {
        OPERATIONS
            .iter()
            .find(|s| s.operation == self)
            .map(|s| s.name)
            .expect("every operation has a row")
    }

    /// `over` combined with `under`, both premultiplied, at the pixel `at`
    /// (column, row) of the grid, where the pattern of [`Operation::Dissolve`]
    /// is read. Both are first held to 0 to 1 and their colours to their
    /// alpha (but for the colours `over` carries with
    /// [`Operation::PremultipliedOver`]), and so is the result.
    pub fn pixel(self, under: Pixel, over: Pixel, at: [u32; 2]) -> Pixel {
        let under = held(under);
        let lit = over.map(|c| c.clamp(0.0, 1.0));
        let over = held(over);
        let (ab, a_s) = (under[3], over[3]);
        let out = match self {
            Operation::Replace => over,
            // The float canvas's own laying of one pixel by the mode.
            Operation::Blend(mode) => mode.lay(under, over),
            Operation::Dissolve => match dither(at) < a_s {
                true => {
                    let [r, g, b] = straight(over);
                    [r, g, b, 1.0]
                }
                false => under,
            },
            Operation::StencilAlpha => under.map(|c| c * a_s),
            Operation::StencilLuma => under.map(|c| c * luma(over)),
            Operation::SilhouetteAlpha => under.map(|c| c * (1.0 - a_s)),
            Operation::SilhouetteLuma => under.map(|c| c * (1.0 - luma(over))),
            Operation::PremultipliedOver => {
                let a = lit[3];
                std::array::from_fn(|c| lit[c] + under[c] * (1.0 - a))
            }
            Operation::AlphaAdd => {
                // What is under it fills only the alpha the result leaves,
                // up to all of its own.
                let share = match ab > 0.0 {
                    true => ((1.0 - a_s) / ab).min(1.0),
                    false => 0.0,
                };
                std::array::from_fn(|c| over[c] + under[c] * share)
            }
        };
        held(out)
    }

    /// Each pixel of `over` combined with the one under it in `under`, in
    /// place, as [`Operation::pixel`] does.
    pub fn pixels(self, under: &mut Pixels, over: &Pixels) {
        let width = u32::from(under.width()).max(1);
        for (i, (u, o)) in under.data_mut().iter_mut().zip(over.data()).enumerate() {
            let i = i as u32;
            *u = self.pixel(*u, *o, [i % width, i / width]);
        }
    }
}

/// A premultiplied pixel held to 0 to 1, its colours to its alpha.
fn held(p: Pixel) -> Pixel {
    let a = p[3].clamp(0.0, 1.0);
    [
        p[0].clamp(0.0, a),
        p[1].clamp(0.0, a),
        p[2].clamp(0.0, a),
        a,
    ]
}

/// The straight colour of a premultiplied pixel; black where it is clear.
fn straight(p: Pixel) -> [f32; 3] {
    match p[3] > 0.0 {
        true => [p[0] / p[3], p[1] / p[3], p[2] / p[3]],
        false => [0.0; 3],
    }
}

/// The luma of a premultiplied pixel: its straight luma times its alpha.
fn luma(p: Pixel) -> f32 {
    p[0] * LUMA[0] + p[1] * LUMA[1] + p[2] * LUMA[2]
}

/// A share from 0 to 1 that looks random from pixel to pixel but is the
/// same for a pixel every time: an integer hash of its position.
fn dither([x, y]: [u32; 2]) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77).rotate_left(16);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
#[path = "../tests/unit/composite.rs"]
mod tests;
