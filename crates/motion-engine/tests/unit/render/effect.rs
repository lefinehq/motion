//! Unit tests for `src/render/effect.rs`.

use motion_format::item::EFFECTS as SCENE_EFFECTS;
use motion_format::{Path, ValueType};
use motion_raster::{EFFECTS as RASTER_EFFECTS, ParamType};
use pretty_assertions::assert_eq;

use super::*;
use crate::render::tests::{list, only_layer, scene};

/// The rasteriser's parameter type for a scene value type, if it has one.
fn raster_type(ty: ValueType) -> Option<ParamType> {
    match ty {
        ValueType::Number => Some(ParamType::Number),
        ValueType::Color => Some(ParamType::Color),
        ValueType::Point | ValueType::Path => None,
    }
}

#[test]
fn every_scene_effect_has_a_raster_row_with_the_same_parameters() {
    for kind in &SCENE_EFFECTS {
        let spec = RASTER_EFFECTS
            .iter()
            .find(|spec| spec.name == kind.name)
            .unwrap_or_else(|| panic!("no raster row for effect `{}`", kind.name));
        let scene: Vec<&str> = kind.registry.rows.iter().map(|row| row.name).collect();
        let raster: Vec<&str> = spec.params.iter().map(|param| param.name).collect();
        assert_eq!(scene, raster, "the parameters of `{}`", kind.name);
        for (row, param) in kind.registry.rows.iter().zip(spec.params) {
            let what = format!("`{}.{}`", kind.name, row.name);
            assert_eq!(
                raster_type(row.ty),
                Some(param.default.param_type()),
                "the type of {what}"
            );
            if row.ty == ValueType::Number {
                assert_eq!(
                    row.range.map(|(lo, hi)| (lo as f32, hi as f32)),
                    param.range,
                    "the range of {what}"
                );
            }
        }
    }
}

#[test]
fn a_layers_effects_keep_their_order_and_values() {
    let frame = list(
        &scene(
            r##"{"id":"a","rect":"10x10","fx":[{"fill":"#FF0000","opacity":0.25},{"fill":"#0000FF80"}]}"##,
        ),
        0.0,
    );
    let effects = &only_layer(&frame).effects;
    let values: Vec<&[ParamValue]> = effects.iter().map(Effect::values).collect();
    assert_eq!(
        values,
        [
            &[
                ParamValue::Color(motion_raster::Color::rgba(1.0, 0.0, 0.0, 1.0)),
                ParamValue::Number(0.25)
            ][..],
            &[
                ParamValue::Color(motion_raster::Color::rgba(0.0, 0.0, 1.0, 128.0 / 255.0)),
                ParamValue::Number(1.0)
            ][..],
        ]
    );
    assert!(effects.iter().all(|e| e.spec().name == "fill"));
}

#[test]
fn a_value_the_rasteriser_has_no_type_for_is_reported() {
    let point = param(&Value::Point([1.0, 2.0])).unwrap_err();
    assert_eq!(point.kind, Kind::WrongType);
    assert!(param(&Value::Path(Path::new(false, []))).is_err());
}
