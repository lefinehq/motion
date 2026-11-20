use super::*;
use crate::parse_document;
use serde_json::json;

/// The scene `layers`, with `extra` keys at the top.
fn scene(extra: Json, layers: Json) -> Json {
    let mut scene = json!({"nakkash":1,"size":"200x100","dur":"1s"});
    scene
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    scene["layers"] = layers;
    scene
}

#[test]
fn a_layer_takes_its_styles_keys_under_its_own_where_it_names_them() {
    let raw = scene(
        json!({"styles":{"soft":{"fx":[{"glow":30}],"opacity":0.5},"faded":{"opacity":0.2}}}),
        json!([{"id":"a","rect":"10x10","inherits":["soft","faded"],"fill":"#FF0000"},
               {"id":"b","rect":"10x10","opacity":0.9,"inherits":"soft"}]),
    );
    let expanded = expand(&raw).unwrap();
    assert!(expanded.get("styles").is_none());
    // The later style wins over the earlier, and keys sit where `inherits` was.
    assert_eq!(
        expanded["layers"][0],
        json!({"id":"a","rect":"10x10","fx":[{"glow":30}],"opacity":0.2,"fill":"#FF0000"})
    );
    // The layer's own key wins.
    assert_eq!(
        expanded["layers"][1],
        json!({"id":"b","rect":"10x10","opacity":0.9,"fx":[{"glow":30}]})
    );
}

#[test]
fn a_styled_scene_reads_as_the_scene_written_out() {
    let styled = scene(
        json!({"styles":{"soft":{"fx":[{"glow":30}],"anim":{"opacity":[{"t":"0s","v":0,"curve":"rise"},{"t":"1s","v":1}]}}},
               "eases":{"rise":"speed(0,0.95,0,0.97)"}}),
        json!([{"id":"a","rect":"10x10","inherits":"soft"},{"id":"b","rect":"20x10","inherits":"soft"}]),
    );
    let anim =
        json!({"opacity":[{"t":"0s","v":0,"curve":"speed(0,0.95,0,0.97)"},{"t":"1s","v":1}]});
    let plain = scene(
        json!({}),
        json!([{"id":"a","rect":"10x10","fx":[{"glow":30}],"anim":anim},
               {"id":"b","rect":"20x10","fx":[{"glow":30}],"anim":anim}]),
    );
    let styled = parse_document(&styled.to_string()).unwrap();
    let plain = parse_document(&plain.to_string()).unwrap();
    assert_eq!(styled.scene(), plain.scene());
    // What was written is kept.
    assert!(styled.raw().get("styles").is_some());
}

#[test]
fn a_style_the_scene_lacks_is_an_error_at_the_layer_naming_the_known_ones() {
    let raw = scene(
        json!({"styles":{"soft":{"opacity":0.5}}}),
        json!([{"id":"a","rect":"10x10","inherits":"sofft"}]),
    );
    let error = parse_document(&raw.to_string()).unwrap_err();
    assert_eq!(error.path_text().as_deref(), Some("a.inherits"));
    assert!(error.to_string().contains("soft"), "{error}");
}

#[test]
fn a_style_cannot_set_the_layers_id_and_an_ease_cannot_take_a_curves_name() {
    let id = scene(json!({"styles":{"s":{"id":"x"}}}), json!([]));
    let error = parse_document(&id.to_string()).unwrap_err();
    assert_eq!(error.path_text().as_deref(), Some("styles.s.id"));
    let shadow = scene(json!({"eases":{"ease-out":"linear"}}), json!([]));
    let error = parse_document(&shadow.to_string()).unwrap_err();
    assert_eq!(error.kind, Kind::Conflict);
    let bad = scene(json!({"eases":{"rise":"speed(1)"}}), json!([]));
    assert!(parse_document(&bad.to_string()).is_err());
}

#[test]
fn a_scene_without_styles_or_eases_is_read_as_it_is() {
    let raw = scene(json!({}), json!([{"id":"a","rect":"10x10"}]));
    assert!(matches!(expand(&raw).unwrap(), Cow::Borrowed(_)));
}

#[test]
fn a_layer_reads_its_keys_through_its_styles() {
    let raw = scene(
        json!({"styles":{"soft":{"opacity":0.5}}}),
        json!([{"id":"a","rect":"10x10","inherits":"soft"}]),
    );
    let layer = raw["layers"][0].as_object().unwrap();
    assert_eq!(inherited(&raw, layer).unwrap()["opacity"], 0.5);
    assert_eq!(style_names(layer), ["soft"]);
    let ids = vec!["a".parse().unwrap()];
    assert_eq!(
        crate::overrides::layer_json(&raw, &ids).unwrap()["opacity"],
        0.5
    );
}
