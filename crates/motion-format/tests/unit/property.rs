//! Unit tests for `src/property.rs`.

use super::*;
use crate::shorthand::Coord;

#[test]
fn named_returns_the_row() {
    let opacity = Prop::named("opacity").unwrap();
    assert_eq!(opacity, Prop::OPACITY);
    assert_eq!(opacity.spec().rest, Rest::Value(Value::Number(1.0)));
    assert_eq!(opacity.spec().range, Some((0.0, 1.0)));
    assert!(Prop::named("zoom").is_none());
    // A component is a target, not a property.
    assert!(Prop::named("x").is_none());
}

#[test]
fn each_constant_is_the_row_of_its_name() {
    for (prop, name) in [
        (Prop::POSITION, "position"),
        (Prop::ANCHOR, "anchor"),
        (Prop::SCALE, "scale"),
        (Prop::ROTATE, "rotate"),
        (Prop::OPACITY, "opacity"),
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
        // A property names every component it splits into, or none.
        let named = prop.spec().components.len();
        assert!(
            named == 0 || Some(named) == prop.spec().ty.dims(),
            "{}",
            prop.name()
        );
    }
    let all: Vec<&str> = target_names().collect();
    let mut unique = all.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), all.len(), "{all:?}");
}

#[test]
fn rest_comes_from_at_the_box_or_the_row() {
    let at = Position {
        x: Coord::Px(100.0),
        y: Coord::Center,
    };
    let rests: Vec<Value> = PROPERTIES
        .iter()
        .map(|p| {
            p.rest(
                &at,
                kurbo::Rect::new(0.0, 0.0, 1920.0, 1080.0),
                kurbo::Rect::new(0.0, 0.0, 40.0, 20.0),
            )
        })
        .collect();
    assert_eq!(
        rests,
        [
            Value::Point([100.0, 540.0]),
            Value::Point([20.0, 10.0]),
            Value::Point([1.0, 1.0]),
            Value::Number(0.0),
            Value::Number(1.0),
        ]
    );
}

#[test]
fn names_list_follows_registry_order() {
    assert_eq!(
        property_names().collect::<Vec<_>>(),
        ["position", "anchor", "scale", "rotate", "opacity"]
    );
    assert_eq!(
        target_names().skip(5).collect::<Vec<_>>(),
        ["x", "y", "anchor-x", "anchor-y", "scale-x", "scale-y"]
    );
}

#[test]
fn targets_name_a_whole_property_or_one_component() {
    let scale = Target::named("scale").unwrap();
    assert_eq!(scale, Target::whole(Prop::SCALE));
    assert_eq!(scale.value_type(), ValueType::Point);
    let y = Target::named("y").unwrap();
    assert_eq!((y.prop, y.component), (Prop::POSITION, Some(1)));
    assert_eq!(y.value_type(), ValueType::Number);
    assert_eq!(y.name(), "y");
    assert_eq!(Target::named("scale-x").unwrap().name(), "scale-x");
    assert!(Target::named("z").is_none());
}

#[test]
fn targets_overlap_when_they_write_the_same_numbers() {
    let t = |name| Target::named(name).unwrap();
    assert!(t("position").overlaps(t("x")));
    assert!(t("x").overlaps(t("position")));
    assert!(t("x").overlaps(t("x")));
    assert!(!t("x").overlaps(t("y")));
    assert!(!t("x").overlaps(t("anchor-x")));
    assert!(!t("scale").overlaps(t("rotate")));
}

#[test]
fn rest_of_picks_the_component() {
    let rest = Value::Point([3.0, 4.0]);
    assert_eq!(
        Target::named("y").unwrap().rest_of(rest.clone()),
        Value::Number(4.0)
    );
    assert_eq!(
        Target::named("position").unwrap().rest_of(rest.clone()),
        rest
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
