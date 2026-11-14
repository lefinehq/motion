//! A layer's `fx` as the rasteriser's effects, joined by name: the scene
//! format's effect kinds (`motion_format::item::EFFECTS`) and the
//! rasteriser's rows (`motion_raster::EFFECTS`) share the kind's name and
//! each parameter's name and type, which a test checks row by row. Adding an
//! effect is a row in each table and nothing here.

use motion_format::{Diagnostic, Kind, Result, Value};
use motion_raster::{Effect, ParamValue};
use motion_scene::ItemState;

use super::color;

/// The rasteriser's effects for `fx`, in order.
///
/// # Errors
/// A diagnostic at `fx[i]` for an effect or parameter the rasteriser has no
/// row for, or a value it does not accept.
pub(super) fn effects(fx: &[ItemState]) -> Result<Vec<Effect>> {
    fx.iter()
        .enumerate()
        .map(|(i, item)| effect(item).map_err(|e| e.under(format!("fx[{i}]"))))
        .collect()
}

/// One effect, every parameter set from the row of the same name.
fn effect(item: &ItemState) -> Result<Effect> {
    let params = item
        .kind
        .registry
        .rows
        .iter()
        .zip(&item.values)
        .map(|(row, value)| Ok((row.name, param(value).map_err(|e| e.at(row.name))?)))
        .collect::<Result<Vec<_>>>()?;
    Effect::new(item.kind.name, &params)
}

/// A scene value as an effect parameter.
fn param(value: &Value) -> Result<ParamValue> {
    match value {
        Value::Number(n) => Ok(ParamValue::Number(*n as f32)),
        Value::Color(rgba) => Ok(ParamValue::Color(color(*rgba))),
        other => Err(Diagnostic::new(
            Kind::WrongType,
            format!("an effect parameter holding a {:?}", other.value_type()),
        )
        .expected("a number or a colour")),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/effect.rs"]
mod tests;
