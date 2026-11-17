//! Unit tests for `src/render/effect.rs`.

use nakkash_format::item::EFFECTS as SCENE_EFFECTS;
use nakkash_format::{Path, ValueType};
use nakkash_raster::{EFFECTS as RASTER_EFFECTS, ParamType};
use pretty_assertions::assert_eq;

use super::*;
use crate::render::tests::{list, only_layer, scene, session};

/// The rasteriser's parameter type for a scene value type, if it has one.
fn raster_type(ty: ValueType) -> Option<ParamType> {
    match ty {
        ValueType::Number => Some(ParamType::Number),
        ValueType::Color => Some(ParamType::Color),
        ValueType::Point => Some(ParamType::Point),
        ValueType::Path => Some(ParamType::Curve),
        ValueType::Orientation => None,
    }
}

#[test]
fn every_scene_effect_has_a_raster_row_with_the_same_parameters() {
    for kind in &SCENE_EFFECTS {
        let spec = RASTER_EFFECTS
            .iter()
            .find(|spec| spec.name == kind.name)
            .unwrap_or_else(|| panic!("no raster row for effect `{}`", kind.name));
        let scene: Vec<&str> = kind
            .registry
            .rows
            .iter()
            .map(|row| row.name)
            .chain(kind.settings.iter().map(|setting| setting.name))
            .collect();
        let raster: Vec<&str> = spec.params.iter().map(|param| param.name).collect();
        assert_eq!(scene, raster, "the parameters of `{}`", kind.name);
        for (setting, param) in kind
            .settings
            .iter()
            .zip(&spec.params[kind.registry.rows.len()..])
        {
            assert_eq!(
                setting.choices, param.choices,
                "the choices of `{}.{}`",
                kind.name, setting.name
            );
            assert_eq!(
                setting.default_index(),
                0,
                "`{}.{}` defaults to its first choice",
                kind.name,
                setting.name
            );
        }
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
            r##"{"id":"a","rect":"10x10","fx":[{"fill":"#FF0000","opacity":0.25},{"fill":"#0000FF80","inverted":"on"}]}"##,
        ),
        0.0,
    );
    let effects = &only_layer(&frame).effects;
    let values: Vec<&[ParamValue]> = effects.iter().map(Effect::values).collect();
    assert_eq!(
        values,
        [
            &[
                ParamValue::Color(nakkash_raster::Color::rgba(1.0, 0.0, 0.0, 1.0)),
                ParamValue::Number(0.25),
                ParamValue::Choice(0)
            ][..],
            &[
                ParamValue::Color(nakkash_raster::Color::rgba(0.0, 0.0, 1.0, 128.0 / 255.0)),
                ParamValue::Number(1.0),
                ParamValue::Choice(1)
            ][..],
        ]
    );
    assert!(effects.iter().all(|e| e.spec().name == "fill"));
}

/// A path through `points`, with no handles.
fn path(points: &[[f64; 2]]) -> Value {
    Value::Path(Path::new(
        false,
        points.iter().map(|&point| nakkash_format::Vertex {
            point,
            arriving: [0.0; 2],
            leaving: [0.0; 2],
        }),
    ))
}

#[test]
fn points_and_curves_become_their_parameters() {
    assert_eq!(
        param(&Value::Point([1.0, 2.0])).unwrap(),
        ParamValue::Point([1.0, 2.0])
    );
    assert_eq!(
        param(&path(&[])).unwrap(),
        ParamValue::Curve(Curve::IDENTITY)
    );
    assert_eq!(
        param(&path(&[[0.0, 0.0], [0.5, 0.75], [1.0, 1.0]])).unwrap(),
        ParamValue::Curve(Curve::new(&[[0.0, 0.0], [0.5, 0.75], [1.0, 1.0]]).unwrap())
    );
}

#[test]
fn a_path_that_is_no_curve_is_reported() {
    let backwards = param(&path(&[[0.5, 0.0], [0.25, 1.0]])).unwrap_err();
    assert_eq!(backwards.kind, Kind::OutOfRange);
    let outside = param(&path(&[[0.0, 0.0], [1.0, 2.0]])).unwrap_err();
    assert_eq!(outside.kind, Kind::OutOfRange);
    let points: Vec<[f64; 2]> = (0..17).map(|i| [f64::from(i) / 16.0, 0.0]).collect();
    assert_eq!(param(&path(&points)).unwrap_err().kind, Kind::OutOfRange);
}

/// How many pixels of `scene` at `frame` show any colour.
fn lit(scene: &str, frame: f64) -> usize {
    let image = session(scene).render(frame).expect("a frame");
    image
        .pixels()
        .chunks_exact(4)
        .filter(|p| p[..3].iter().any(|&c| c > 0))
        .count()
}

/// A burst: `rate` keyed from 1000 a second down to 0 after 0.2 s. The
/// rate acts on births only, so the burst stays lit after the drop and
/// lives out its `longevity` of 1 s, and a rate that holds draws as before.
#[rstest::rstest]
#[case::while_born(1.0, true)]
#[case::after_the_drop(5.0, true)]
#[case::near_the_end_of_its_life(11.0, true)]
#[case::after_its_longevity(13.0, false)]
fn a_rate_keyed_to_zero_leaves_the_burst_to_live_out_its_longevity(
    #[case] frame: f64,
    #[case] shows: bool,
) {
    let burst = |anim: &str| {
        format!(
            r##"{{"nakkash":1,"size":"64x64","fps":10,"dur":"2s","bg":"#000000","layers":[
              {{"id":"p","rect":"64x64","fill":"#000000","anchor":[0,0],"position":[0,0],
                "fx":[{{"particles":1000,"longevity":1,"position":[32,32],"radius-x":0,
                  "radius-y":0,"velocity":0,"gravity":0,"birth-size":8,"death-size":8,
                  "size-variation":0,"max-opacity":1,"birth-color":"#FFFFFF",
                  "death-color":"#FFFFFF","shape":"disc","opacity-map":"constant"{anim}}}]}}]}}"##
        )
    };
    let cut = burst(r#","anim":{"rate":[[0,1000],[1,1000,"hold"],[2,0]]}"#);
    assert_eq!(lit(&cut, frame) > 0, shows, "frame {frame}");
    // Until the cut, the burst is the rate alone.
    if frame <= 2.0 {
        assert_eq!(lit(&cut, frame), lit(&burst(""), frame));
    }
}
