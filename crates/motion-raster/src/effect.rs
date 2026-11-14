//! Effects as a registry ([D34]): each is a row of [`EFFECTS`] with a name,
//! typed parameters and a function that draws it onto a layer's isolated
//! content. Adding an effect is adding a row and its function; the renderer
//! never names an effect.
//!
//! An [`Effect`] in a display list is built with [`Effect::new`], which checks
//! its parameters against the row, so drawing it cannot fail.
//!
//! [D34]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

use std::fmt;

use motion_format::{Diagnostic, Kind, Result};
use vello_cpu::RenderContext;
use vello_cpu::kurbo::{Affine, Rect};
use vello_cpu::peniko::{BlendMode, Compose, Mix};

use crate::style::Color;

/// The type of one effect parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    /// A number.
    Number,
    /// A colour.
    Color,
}

/// A value of an effect parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    /// A number.
    Number(f32),
    /// A colour.
    Color(Color),
}

impl ParamValue {
    /// The type of the value.
    pub fn param_type(&self) -> ParamType {
        match self {
            ParamValue::Number(_) => ParamType::Number,
            ParamValue::Color(_) => ParamType::Color,
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
    /// Draws the effect onto the isolated content of the layer, given the
    /// parameter values in the order of `params`.
    pub draw: fn(&mut EffectContext<'_>, &[ParamValue]),
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
pub const EFFECTS: &[EffectSpec] = &[EffectSpec {
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
    draw: fill,
}];

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

    /// Draws the effect onto the content drawn so far in `ctx`'s layer.
    pub(crate) fn draw(&self, ctx: &mut EffectContext<'_>) {
        (self.spec.draw)(ctx, &self.values);
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

#[cfg(test)]
#[path = "../tests/unit/effect.rs"]
mod tests;
