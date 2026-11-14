//! Unit tests for `src/property.rs`.

use super::*;
use crate::shorthand::Coord;

#[test]
fn named_returns_the_row() {
    let opacity = Prop::named("opacity").unwrap();
    assert_eq!(opacity, Prop::OPACITY);
    assert_eq!(opacity.spec().default, 1.0);
    assert_eq!(opacity.spec().range, Some((0.0, 1.0)));
    assert!(Prop::named("zoom").is_none());
}

#[test]
fn each_constant_is_the_row_of_its_name() {
    for (prop, name) in [
        (Prop::X, "x"),
        (Prop::Y, "y"),
        (Prop::OPACITY, "opacity"),
        (Prop::SCALE, "scale"),
        (Prop::ROTATE, "rotate"),
    ] {
        assert_eq!(prop.name(), name);
        assert_eq!(Prop::named(name), Some(prop));
    }
}

#[test]
fn registry_names_are_unique_and_documented() {
    for prop in Prop::all() {
        assert_eq!(Prop::named(prop.name()), Some(prop));
        assert!(!prop.spec().doc.is_empty(), "{}", prop.name());
    }
    assert_eq!(Prop::all().count(), PROPERTIES.len());
}

#[test]
fn only_x_and_y_rest_on_at() {
    let at = Position {
        x: Coord::Px(100.0),
        y: Coord::Center,
    };
    let rests: Vec<f64> = PROPERTIES
        .iter()
        .map(|p| p.rest(&at, (1920.0, 1080.0)))
        .collect();
    assert_eq!(rests, [100.0, 540.0, 1.0, 1.0, 0.0]);
    assert_eq!(Prop::X.spec().rest_from_at, Some(Axis::X));
    assert_eq!(Prop::Y.spec().rest_from_at, Some(Axis::Y));
}

#[test]
fn names_list_follows_registry_order() {
    assert_eq!(
        property_names().collect::<Vec<_>>(),
        ["x", "y", "opacity", "scale", "rotate"]
    );
}

#[test]
fn scene_format_doc_lists_exactly_the_registry_names() {
    let doc = include_str!("../../../../docs/design/scene-format.md");
    // Rows of the table that starts at the `| Property |` header.
    let documented: Vec<&str> = doc
        .lines()
        .skip_while(|l| !l.starts_with("| Property |"))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .filter_map(|l| l.split('|').nth(1))
        .map(|cell| cell.trim().trim_matches('`'))
        .collect();
    let registry: Vec<&str> = PROPERTIES.iter().map(|p| p.name).collect();
    assert_eq!(documented, registry);
}
