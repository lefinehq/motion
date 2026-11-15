//! Effects as a registry ([D34]): each is a row of [`EFFECTS`] with a name,
//! typed parameters and what it does to a layer, as an [`Apply`]: move the
//! layer's content as it is drawn, or change the pixels it drew. Adding an
//! effect is adding a row and its function; the renderer never names an
//! effect.
//!
//! A layer whose effects all move it, or can paint their change over its
//! content without reading it, is drawn as it is. Any other is drawn on a
//! canvas of its own, and its effects run in order over the pixels.
//!
//! An [`Effect`] in a display list is built with [`Effect::new`], which checks
//! its parameters against the row, so drawing it cannot fail.
//!
//! [D34]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

use std::fmt;

use motion_format::{Diagnostic, Kind, Result};
use vello_cpu::RenderContext;
use vello_cpu::kurbo::{Affine, Rect, Vec2};
use vello_cpu::peniko::{BlendMode, Compose, Mix};

use crate::pixels::Pixels;
use crate::style::Color;

/// The type of one effect parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    /// A number.
    Number,
    /// A colour.
    Color,
    /// A point, in px of the layer's own coordinates.
    Point,
}

/// A value of an effect parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    /// A number.
    Number(f32),
    /// A colour.
    Color(Color),
    /// A point, in px of the layer's own coordinates.
    Point([f32; 2]),
}

impl ParamValue {
    /// The type of the value.
    pub fn param_type(&self) -> ParamType {
        match self {
            ParamValue::Number(_) => ParamType::Number,
            ParamValue::Color(_) => ParamType::Color,
            ParamValue::Point(_) => ParamType::Point,
        }
    }
}

/// One parameter of an effect: its name, its default and the values it accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    /// The name display lists use.
    pub name: &'static str,
    /// Its value when a display list leaves it out; also gives its type.
    pub default: ParamValue,
    /// The allowed numbers, inclusive, for a number parameter.
    pub range: Option<(f32, f32)>,
    /// What it does.
    pub doc: &'static str,
}

/// One row of the registry.
pub struct EffectSpec {
    /// The name display lists use.
    pub name: &'static str,
    /// Its parameters, in order.
    pub params: &'static [ParamSpec],
    /// What it does.
    pub doc: &'static str,
    /// What it does to the layer, given the parameter values in the order of
    /// `params`.
    pub apply: Apply,
}

/// What an effect does to the layer it is on.
#[derive(Debug, Clone, Copy)]
pub enum Apply {
    /// Moves the layer's content: a transform in the layer's own
    /// coordinates and an opacity. Before any effect that reads pixels, it
    /// applies as the content is drawn; after one, it moves the pixels.
    Move(fn(&[ParamValue]) -> Move),
    /// Changes the pixels the layer drew.
    Pixels {
        /// The same change painted over the content without reading it,
        /// for a layer drawn as it is; `None` when the change needs the
        /// pixels.
        over: Option<fn(&mut EffectContext<'_>, &[ParamValue])>,
        /// The change, in place.
        pixels: fn(&mut Pixels, &[ParamValue], &Space),
    },
}

/// Where an effect's pixels are: lengths and points of its parameters are
/// in the layer's own coordinates, which `transform` takes to pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Space {
    /// From the layer's own coordinates to pixels of the grid.
    pub transform: Affine,
}

/// How a moving effect moves the layer's content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Move {
    /// From the content's coordinates to the layer's.
    pub transform: Affine,
    /// What the content's opacity is multiplied by, 0 to 1.
    pub opacity: f32,
}

impl fmt::Debug for EffectSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EffectSpec")
            .field("name", &self.name)
            .finish()
    }
}

impl PartialEq for EffectSpec {
    fn eq(&self, other: &EffectSpec) -> bool {
        self.name == other.name
    }
}

/// Every effect the renderer can draw.
pub const EFFECTS: &[EffectSpec] = &[
    EffectSpec {
        name: "fill",
        params: &[
            ParamSpec {
                name: "color",
                default: ParamValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
                range: None,
                doc: "the colour the content takes",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                doc: "how much of the colour replaces the content's own, 0 to 1",
            },
        ],
        doc: "recolours the content with one colour, keeping its alpha",
        apply: Apply::Pixels {
            over: Some(fill),
            pixels: fill_pixels,
        },
    },
    EffectSpec {
        name: "transform",
        params: &[
            ParamSpec {
                name: "position",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                doc: "where `anchor` goes, in px of the layer's own coordinates",
            },
            ParamSpec {
                name: "anchor",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                doc: "the point of the content that moves to `position`, and that it scales \
                      and turns about",
            },
            ParamSpec {
                name: "scale",
                default: ParamValue::Point([1.0, 1.0]),
                range: None,
                doc: "size factor on each axis",
            },
            ParamSpec {
                name: "rotate",
                default: ParamValue::Number(0.0),
                range: None,
                doc: "rotation in degrees, clockwise",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                doc: "what the content's opacity is multiplied by, 0 to 1",
            },
        ],
        doc: "moves, scales, turns and fades the content inside the layer",
        apply: Apply::Move(transform),
    },
];

/// An effect with its parameter values, checked against its row.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    spec: &'static EffectSpec,
    values: Vec<ParamValue>,
}

impl Effect {
    /// The effect called `name` with the parameters given by name; the
    /// others take their defaults.
    ///
    /// # Errors
    /// A diagnostic for an unknown effect or parameter, a value of the wrong
    /// type, or a number outside its range; its path names the parameter.
    pub fn new(name: &str, params: &[(&str, ParamValue)]) -> Result<Effect> {
        let spec = EFFECTS
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| Diagnostic::unknown("effect", name, EFFECTS.iter().map(|s| s.name)))?;
        let mut values: Vec<ParamValue> = spec.params.iter().map(|p| p.default).collect();
        for &(param, value) in params {
            let i = spec
                .params
                .iter()
                .position(|p| p.name == param)
                .ok_or_else(|| {
                    Diagnostic::unknown(
                        "effect parameter",
                        param,
                        spec.params.iter().map(|p| p.name),
                    )
                    .at(param)
                })?;
            values[i] = checked(&spec.params[i], value).map_err(|e| e.at(param))?;
        }
        Ok(Effect { spec, values })
    }

    /// The effect's row.
    pub fn spec(&self) -> &'static EffectSpec {
        self.spec
    }

    /// The value of every parameter, in the order of the row's `params`.
    pub fn values(&self) -> &[ParamValue] {
        &self.values
    }

    /// Paints the effect over the content drawn so far in `ctx`'s layer, if
    /// it can without reading it.
    pub(crate) fn paint_over(&self, ctx: &mut EffectContext<'_>) {
        if let Apply::Pixels {
            over: Some(over), ..
        } = self.spec.apply
        {
            over(ctx, &self.values);
        }
    }

    /// Whether it needs the pixels its layer drew.
    pub fn reads_pixels(&self) -> bool {
        matches!(self.spec.apply, Apply::Pixels { over: None, .. })
    }

    /// How it moves the content, if it is a moving one.
    pub fn movement(&self) -> Option<Move> {
        match self.spec.apply {
            Apply::Move(movement) => Some(movement(&self.values)),
            Apply::Pixels { .. } => None,
        }
    }

    /// Applies the effect to `pixels`, a layer's drawn in `space`: a move
    /// resamples them, any other changes them in place.
    pub fn apply(&self, pixels: &mut Pixels, space: &Space) {
        match self.spec.apply {
            Apply::Move(movement) => {
                let Move { transform, opacity } = movement(&self.values);
                let inside = space.transform * transform * space.transform.inverse();
                if inside != Affine::IDENTITY {
                    *pixels = pixels.transformed(inside);
                }
                if opacity < 1.0 {
                    for pixel in pixels.data_mut() {
                        *pixel = pixel.map(|c| c * opacity);
                    }
                }
            }
            Apply::Pixels { pixels: change, .. } => change(pixels, &self.values, space),
        }
    }
}

/// `value` if it has the parameter's type and lies in its range.
fn checked(spec: &ParamSpec, value: ParamValue) -> Result<ParamValue> {
    let expected = spec.default.param_type();
    if value.param_type() != expected {
        return Err(Diagnostic::new(
            Kind::WrongType,
            format!(
                "`{}` takes a {expected:?}, not a {:?}",
                spec.name,
                value.param_type()
            ),
        )
        .expected(format!("{expected:?}").to_lowercase()));
    }
    match (value, spec.range) {
        (ParamValue::Number(n), Some((lo, hi))) if !(lo..=hi).contains(&n) => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("{n} is outside {lo}..={hi}"),
        )
        .expected(format!("a number from {lo} to {hi}"))),
        _ => Ok(value),
    }
}

/// What an effect may draw with: the render context, inside the isolated
/// layer it applies to, and the size of the frame.
pub struct EffectContext<'a> {
    pub(crate) ctx: &'a mut RenderContext,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl EffectContext<'_> {
    /// Paints `color` over the whole frame, combined with the layer's content
    /// by `compose`. Leaves the context's transform, paint and blend mode as
    /// it found them.
    pub fn cover(&mut self, color: Color, compose: Compose) {
        let ctx = &mut *self.ctx;
        let (transform, paint, blend) = (*ctx.transform(), ctx.paint().clone(), ctx.blend_mode());
        ctx.set_transform(Affine::IDENTITY);
        ctx.set_paint(color.to_vello());
        ctx.set_blend_mode(BlendMode {
            mix: Mix::Normal,
            compose,
        });
        ctx.fill_rect(&Rect::new(
            0.0,
            0.0,
            f64::from(self.width),
            f64::from(self.height),
        ));
        ctx.set_transform(transform);
        ctx.set_paint(paint);
        ctx.set_blend_mode(blend);
    }
}

/// The `fill` effect: the content's colour moves towards `color` by
/// `opacity`, and its alpha stays.
///
/// Painting the colour with alpha `opacity` "source-atop" the content gives,
/// per premultiplied pixel, `opacity * color * a + (1 - opacity) * content`,
/// with alpha `a` unchanged: exactly that mix, done by the rasteriser's own
/// compositing rather than a pass over the pixels.
fn fill(ctx: &mut EffectContext<'_>, values: &[ParamValue]) {
    let [ParamValue::Color(color), ParamValue::Number(opacity)] = values else {
        unreachable!("`Effect::new` checks the fill parameters");
    };
    ctx.cover(color.with_opacity(*opacity), Compose::SrcAtop);
}

/// The `fill` effect on pixels: the colour, at its alpha times `opacity`,
/// laid "source-atop" each pixel, as [`fill`] paints it.
fn fill_pixels(pixels: &mut Pixels, values: &[ParamValue], _: &Space) {
    let [ParamValue::Color(color), ParamValue::Number(opacity)] = values else {
        unreachable!("`Effect::new` checks the fill parameters");
    };
    let share = color.a * opacity;
    let rgb = [color.r, color.g, color.b];
    for pixel in pixels.data_mut() {
        let alpha = pixel[3];
        for c in 0..3 {
            pixel[c] = rgb[c] * share * alpha + pixel[c] * (1.0 - share);
        }
    }
}

/// The `transform` effect: the content's anchor goes to `position`, scaled
/// and turned about it, as a shape group's transform places its items.
fn transform(values: &[ParamValue]) -> Move {
    let [
        ParamValue::Point(position),
        ParamValue::Point(anchor),
        ParamValue::Point(scale),
        ParamValue::Number(rotate),
        ParamValue::Number(opacity),
    ] = values
    else {
        unreachable!("`Effect::new` checks the transform parameters");
    };
    let point = |[x, y]: [f32; 2]| Vec2::new(f64::from(x), f64::from(y));
    Move {
        transform: Affine::translate(point(*position))
            * Affine::rotate(f64::from(*rotate).to_radians())
            * Affine::scale_non_uniform(f64::from(scale[0]), f64::from(scale[1]))
            * Affine::translate(-point(*anchor)),
        opacity: *opacity,
    }
}

#[cfg(test)]
#[path = "../tests/unit/effect.rs"]
mod tests;
