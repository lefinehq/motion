//! Unit tests for `src/validate/item.rs`: reading shape items and effects.

use crate::item::{Role, props};
use crate::{Content, Item, Value, parse_document};

/// The items of the one shape layer of a scene whose `shapes` holds `items`.
fn shapes(items: &str) -> Vec<Item> {
    let json = format!(r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","shapes":[{items}]}}]}}"#);
    let doc = parse_document(&json).unwrap();
    match &doc.scene().layers[0].content {
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
fn paths_animate_with_key_lists_of_one_shape() {
    let items = shapes(
        r#"{"path":"M0,0 L10,0 L10,10 Z","anim":{"path":[{"t":0,"v":"M0,0 L10,0 L10,10 Z"},{"t":10,"v":"M0,0 L20,0 L20,20 Z"}]}}"#,
    );
    let track = &items[0].anim[0];
    assert_eq!(track.target.prop, props::PATH_PATH);
    assert_eq!(track.keys.len(), 2);
    assert!(track.keys[1].value.as_path().unwrap().closed());
}

#[test]
fn hidden_items_and_effects_are_kept() {
    let json = r##"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10","fx":[{"fill":"#00FF00","hidden":true,"anim":{"opacity":"0 -> 1 @0-5f"}}]}]}"##;
    let doc = parse_document(json).unwrap();
    let fx = &doc.scene().layers[0].fx;
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
        let word = prop::sample::select(vec![
            "group", "rect", "ellipse", "path", "fill", "stroke", "size", "position", "round",
            "color", "opacity", "width", "miter", "cap", "join", "rule", "id", "hidden", "anim",
            "x", "width", "round", "evenodd", "butt", "t", "v", "curve",
        ]);
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
                r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","shapes":{items}}},{{"id":"b","rect":"1x1","fx":{fx}}}]}}"#
            );
            let _ = parse_document(&scene);
        }
    }
}
