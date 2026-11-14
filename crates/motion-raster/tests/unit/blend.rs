//! Unit tests for `src/blend.rs`.

use super::*;
use motion_format::Kind;

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
    // Every variant: separable, non-separable and add.
    assert_eq!(BLEND_MODES.len(), 17);
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
