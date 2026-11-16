//! A layer's `fx` as the rasteriser's effects, joined by name: the scene
//! format's effect kinds (`motion_format::item::EFFECTS`) and the
//! rasteriser's rows (`motion_raster::EFFECTS`) share the kind's name and
//! each parameter's name and type, then each setting as a choice parameter
//! with the same choices, which a test checks row by row. Adding an
//! effect is a row in each table and nothing here.

use motion_format::{Diagnostic, Kind, Result, Value};
use motion_raster::{Curve, Effect, ParamValue};
use motion_scene::{ItemState, NodeState};

use super::color;

/// The rasteriser's effects for `node`'s `fx`, in order, each following
/// the outline of the mask of `node`'s that `outlines` gives for it.
///
/// # Errors
/// A diagnostic at `fx[i]` for an effect or parameter the rasteriser has no
/// row for, or a value it does not accept.
pub(super) fn effects(node: &NodeState, outlines: &[Option<usize>]) -> Result<Vec<Effect>> {
    node.fx
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let effect = effect(item).map_err(|e| e.under(format!("fx[{i}]")))?;
            let mask = outlines.get(i).copied().flatten();
            Ok(match mask.and_then(|m| node.mask.get(m)) {
                Some(mask) => effect.with_outline(super::mask::outline(mask)),
                None => effect,
            })
        })
        .collect()
}

/// One effect, every parameter set from the row of the same name, then
/// each setting as the choice parameter of the same name.
fn effect(item: &ItemState) -> Result<Effect> {
    let mut params = item
        .kind
        .registry
        .rows
        .iter()
        .zip(&item.values)
        .map(|(row, value)| Ok((row.name, param(value).map_err(|e| e.at(row.name))?)))
        .collect::<Result<Vec<_>>>()?;
    for (setting, &choice) in item.kind.settings.iter().zip(&item.settings) {
        let choice = u8::try_from(choice).map_err(|_| {
            Diagnostic::new(Kind::OutOfRange, format!("choice {choice}"))
                .at(setting.name)
                .expected("fewer than 256 choices")
        })?;
        params.push((setting.name, ParamValue::Choice(choice)));
    }
    Effect::new(item.kind.name, &params)
}

/// A scene value as an effect parameter: a path as a curve of its
/// vertices' points.
///
/// # Errors
/// A path the rasteriser does not take as a curve.
fn param(value: &Value) -> Result<ParamValue> {
    match value {
        Value::Number(n) => Ok(ParamValue::Number(*n as f32)),
        Value::Color(rgba) => Ok(ParamValue::Color(color(*rgba))),
        Value::Point([x, y]) => Ok(ParamValue::Point([*x as f32, *y as f32])),
        Value::Path(path) => {
            let points: Vec<[f32; 2]> =
                path.vertices().map(|v| v.point.map(|c| c as f32)).collect();
            Ok(ParamValue::Curve(Curve::new(&points)?))
        }
        Value::Orientation(_) => Err(motion_format::Diagnostic::new(
            motion_format::Kind::WrongType,
            "an effect takes no orientation",
        )
        .expected("a number, a colour, a point or a path")),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/effect.rs"]
mod tests;
