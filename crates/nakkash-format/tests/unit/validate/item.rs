//! Unit tests for `src/validate/item.rs`: reading shape items and effects.

use crate::item::{Role, props};
use crate::{Content, Item, Value, parse_document};

/// The items of the one shape layer of a scene whose `shapes` holds `items`.
fn shapes(items: &str) -> Vec<Item> {
    let json = format!(r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"a","shapes":[{items}]}}]}}"#);
    let doc = parse_document(&json).unwrap();
    match &doc.scene().root.layers[0].content {
        Content::Shapes(items) => items.clone(),
        other => panic!("not shapes: {other:?}"),
    }
}

#[test]
fn the_kind_key_holds_the_primary_value_first() {
    let items = shapes(r##"{"round":4,"rect":[300,100]},{"fill":"#FF0000"}"##);
    assert_eq!(items[0].kind.name, "rect");
    assert_eq!(
        items[0].values,
        [
            (props::RECT_SIZE, Value::Point([300.0, 100.0])),
            (props::RECT_ROUND, Value::Number(4.0)),
        ]
    );
    assert_eq!(
        items[1].rest(props::FILL_COLOR),
        Value::Color([1.0, 0.0, 0.0, 1.0])
    );
    assert_eq!(items[1].rest(props::FILL_OPACITY), Value::Number(1.0));
}

#[test]
fn groups_nest_and_settings_default() {
    let items = shapes(
        r##"{"group":[{"ellipse":[10,10]},{"stroke":"#000","cap":"round","width":3}],"opacity":0.5,"id":"dot"}"##,
    );
    let group = &items[0];
    assert_eq!(group.kind.role, Role::Group);
    assert_eq!(group.id.as_deref(), Some("dot"));
    assert_eq!(group.items.len(), 2);
    let stroke = &group.items[1];
    assert_eq!(
        (stroke.setting("cap"), stroke.setting("join")),
        ("round", "miter")
    );
    assert_eq!(stroke.rest(props::STROKE_WIDTH), Value::Number(3.0));
}

#[test]
fn a_star_keys_its_points_and_rests_as_the_original_draws_one() {
    let items = shapes(
        r#"{"star":6,"type":"polygon","outer-roundness":0.5,"anim":{"rotate":"0 -> 60 @0-5f"}},{"star":3}"#,
    );
    let (polygon, star) = (&items[0], &items[1]);
    assert_eq!(polygon.values[0], (props::STAR_POINTS, Value::Number(6.0)));
    assert_eq!(
        (polygon.setting("type"), polygon.setting("direction")),
        ("polygon", "clockwise")
    );
    assert_eq!(polygon.anim[0].target.prop, props::STAR_ROTATE);
    assert_eq!(star.setting("type"), "star");
    let rests = [
        (props::STAR_POSITION, Value::Point([0.0, 0.0])),
        (props::STAR_ROTATE, Value::Number(0.0)),
        (props::STAR_OUTER_RADIUS, Value::Number(100.0)),
        (props::STAR_INNER_RADIUS, Value::Number(50.0)),
        (props::STAR_OUTER_ROUNDNESS, Value::Number(0.0)),
        (props::STAR_INNER_ROUNDNESS, Value::Number(0.0)),
    ];
    for (prop, rest) in rests {
        assert_eq!(star.rest(prop), rest, "{}", prop.name());
    }
    // Roundness may go past either end: a negative one loops inwards.
    let looped = shapes(r#"{"star":5,"outer-roundness":-2,"inner-roundness":3}"#);
    assert_eq!(
        looped[0].rest(props::STAR_OUTER_ROUNDNESS),
        Value::Number(-2.0)
    );
}

#[test]
fn paths_animate_with_key_lists_of_one_shape() {
    let items = shapes(
        r#"{"path":"M0,0 L10,0 L10,10 Z","anim":{"path":[{"t":0,"v":"M0,0 L10,0 L10,10 Z"},{"t":10,"v":"M0,0 L20,0 L20,20 Z"}]}}"#,
    );
    let track = &items[0].anim[0];
    assert_eq!(track.target.prop, props::PATH_PATH);
    assert_eq!(track.keys.len(), 2);
    assert!(
        track.keys[1]
            .value
            .set()
            .unwrap()
            .as_path()
            .unwrap()
            .closed()
    );
}

#[test]
fn hidden_items_and_effects_are_kept() {
    let json = r##"{"nakkash":1,"dur":"1s","layers":[{"id":"a","rect":"10x10","fx":[{"fill":"#00FF00","hidden":true,"anim":{"opacity":"0 -> 1 @0-5f"}}]}]}"##;
    let doc = parse_document(json).unwrap();
    let fx = &doc.scene().root.layers[0].fx;
    assert_eq!(fx[0].kind.role, Role::Effect);
    assert!(fx[0].hidden);
    assert_eq!(fx[0].anim[0].target.prop, props::FILL_EFFECT_OPACITY);
}

mod never_panics {
    use crate::parse_document;
    use proptest::prelude::*;

    /// A JSON value built from the words items use, so most inputs reach
    /// deep rules rather than failing on their shape.
    fn json() -> impl Strategy<Value = String> {
        let short = [
            "group", "rect", "ellipse", "star", "path", "fill", "stroke", "size", "position",
            "round", "points", "type", "polygon", "rotate", "color", "opacity", "width", "miter",
            "cap", "join", "rule", "id", "hidden", "anim", "x", "width", "round", "evenodd",
            "butt", "t", "v", "curve",
        ];
        let long = ["direction", "outer-radius", "inner-roundness"];
        let word = prop::sample::select([&short[..], &long[..]].concat());
        let leaf = prop_oneof![
            Just("1".to_owned()),
            Just("-3".to_owned()),
            Just("true".to_owned()),
            Just(r##""#FFE000""##.to_owned()),
            Just(r#""M0,0 L1,1 Z""#.to_owned()),
            Just(r#""0 -> 1 @0-5f""#.to_owned()),
            word.clone().prop_map(|w| format!("\"{w}\"")),
        ];
        leaf.prop_recursive(4, 32, 4, move |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4)
                    .prop_map(|v| format!("[{}]", v.join(","))),
                prop::collection::vec((word.clone(), inner), 0..4).prop_map(|pairs| {
                    let body: Vec<String> =
                        pairs.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
                    format!("{{{}}}", body.join(","))
                }),
            ]
        })
    }

    proptest! {
        #[test]
        fn reading_shapes_and_effects_never_panics(items in json(), fx in json()) {
            let scene = format!(
                r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"a","shapes":{items}}},{{"id":"b","rect":"1x1","fx":{fx}}}]}}"#
            );
            let _ = parse_document(&scene);
        }
    }
}

/// The masks of the one rect layer of a scene whose `mask` holds `items`.
fn masks(items: &str) -> crate::Result<Vec<Item>> {
    let json = format!(
        r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"a","rect":"100x100","mask":[{items}]}}]}}"#
    );
    parse_document(&json).map(|doc| doc.scene().root.layers[0].mask.clone())
}

#[test]
fn a_mask_holds_its_path_and_rests_at_its_defaults() {
    let masks = masks(r#"{"mask":"M0,0 L10,0 L10,10 Z","feather":4,"mode":"subtract","id":"cut"}"#)
        .unwrap();
    let mask = &masks[0];
    assert_eq!(mask.kind.role, Role::Mask);
    assert_eq!(mask.values[0].0, props::MASK_PATH);
    assert_eq!(mask.rest(props::MASK_FEATHER), Value::Point([4.0, 4.0]));
    assert_eq!(mask.rest(props::MASK_OPACITY), Value::Number(1.0));
    assert_eq!(mask.rest(props::MASK_EXPAND), Value::Number(0.0));
    assert_eq!(mask.setting("mode"), "subtract");
    assert_eq!(mask.setting("keep"), "inside");
}

#[test]
fn a_mask_mode_is_one_of_its_list() {
    let error = masks(r#"{"mask":"M0,0 L10,0 Z","mode":"xor"}"#).unwrap_err();
    assert_eq!(error.path_text().as_deref(), Some("a.mask[0].mode"));
}

#[test]
fn masks_of_a_layer_have_their_own_ids() {
    let error =
        masks(r#"{"mask":"M0,0 L1,0 Z","id":"m"},{"mask":"M0,0 L1,0 Z","id":"m"}"#).unwrap_err();
    assert_eq!(error.path_text().as_deref(), Some("a.mask[1].id"));
}
