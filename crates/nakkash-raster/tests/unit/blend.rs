//! Unit tests for `src/blend.rs`.

use super::*;
use nakkash_format::Kind;

#[test]
fn names_are_unique_and_round_trip() {
    let mut names: Vec<&str> = BLEND_MODES.iter().map(|s| s.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), BLEND_MODES.len());
    for spec in BLEND_MODES {
        assert_eq!(BlendMode::named(spec.name).unwrap(), spec.mode);
        assert_eq!(spec.mode.name(), spec.name);
        assert_eq!(spec.mode.spec(), spec);
    }
}

#[test]
fn every_mode_has_exactly_one_row() {
    for spec in BLEND_MODES {
        let rows = BLEND_MODES.iter().filter(|s| s.mode == spec.mode).count();
        assert_eq!(rows, 1, "{}", spec.name);
    }
    // Every variant: separable, non-separable, add, linear light, subtract,
    // and the stencils and silhouettes.
    assert_eq!(BLEND_MODES.len(), 23);
}

#[test]
fn only_the_float_canvas_lays_the_modes_vello_lacks() {
    let floats: Vec<&str> = BLEND_MODES
        .iter()
        .filter(|s| s.mode.laid_in_floats())
        .map(|s| s.name)
        .collect();
    assert_eq!(
        floats,
        [
            "linear-light",
            "subtract",
            "stencil-alpha",
            "stencil-luma",
            "silhouette-alpha",
            "silhouette-luma"
        ]
    );
    let keeping: Vec<&str> = BLEND_MODES
        .iter()
        .filter(|s| s.mode.keeps_below())
        .map(|s| s.name)
        .collect();
    assert_eq!(keeping, floats[2..]);
}

#[test]
fn stencils_and_silhouettes_keep_below_by_alpha_or_luma() {
    let below = [0.8, 0.4, 0.2, 0.8];
    let close = |a: [f32; 4], b: [f32; 4]| {
        assert!(
            a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6),
            "{a:?} {b:?}"
        );
    };
    // Half-transparent mid grey, premultiplied: alpha 0.5, luma 0.25.
    let source = [0.25, 0.25, 0.25, 0.5];
    close(
        BlendMode::StencilAlpha.lay(below, source),
        below.map(|c| c * 0.5),
    );
    close(
        BlendMode::SilhouetteAlpha.lay(below, source),
        below.map(|c| c * 0.5),
    );
    close(
        BlendMode::StencilLuma.lay(below, source),
        below.map(|c| c * 0.25),
    );
    close(
        BlendMode::SilhouetteLuma.lay(below, source),
        below.map(|c| c * 0.75),
    );
    // Nothing laid: a stencil keeps nothing, a silhouette everything.
    close(BlendMode::StencilAlpha.lay(below, [0.0; 4]), [0.0; 4]);
    close(BlendMode::SilhouetteLuma.lay(below, [0.0; 4]), below);
}

#[test]
fn linear_light_adds_twice_the_layer_less_one() {
    let lay = |b: f32, s: f32| BlendMode::LinearLight.lay([b, b, b, 1.0], [s, s, s, 1.0])[0];
    assert!((lay(0.5, 0.25) - 0.0).abs() < 1e-6);
    assert!((lay(0.5, 0.6) - 0.7).abs() < 1e-6);
    assert!((lay(0.9, 0.9) - 1.0).abs() < 1e-6);
}

#[test]
fn subtract_takes_the_layer_away_down_to_black() {
    let lay = |b: f32, s: f32| BlendMode::Subtract.lay([b, b, b, 1.0], [s, s, s, 1.0])[0];
    assert!((lay(0.75, 0.25) - 0.5).abs() < 1e-6);
    assert_eq!(lay(0.25, 0.75), 0.0);
}

#[test]
fn modes_map_to_their_vello_mix_and_compose() {
    assert_eq!(
        BlendMode::Multiply.to_vello(),
        peniko::BlendMode {
            mix: Mix::Multiply,
            compose: Compose::SrcOver
        }
    );
    assert_eq!(
        BlendMode::Add.to_vello(),
        peniko::BlendMode {
            mix: Mix::Normal,
            compose: Compose::Plus
        }
    );
    assert_eq!(BlendMode::default(), BlendMode::Normal);
}

#[test]
fn an_unknown_mode_lists_every_name() {
    let e = BlendMode::named("glow").unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    let expected = e.expected.unwrap();
    for spec in BLEND_MODES {
        assert!(expected.contains(spec.name), "{}", spec.name);
    }
}
