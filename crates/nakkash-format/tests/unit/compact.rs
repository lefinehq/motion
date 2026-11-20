use super::*;
use crate::parse_document;
use serde_json::json;

fn same_scene(a: &Json, b: &Json) {
    let a = parse_document(&a.to_string()).unwrap();
    let b = parse_document(&b.to_string()).unwrap();
    assert_eq!(a.scene(), b.scene());
}

fn scene(layers: Json) -> Json {
    json!({"nakkash":1,"size":"200x100","dur":"2s","layers":layers})
}

#[test]
fn values_at_their_rest_are_left_out_and_the_scene_stays_the_same() {
    let raw = scene(json!([
        {"id":"a","rect":"10x10","opacity":1,"rotate":0,"scale":[1,1],"fx":[{"glow":30,"threshold":0.6}]},
        {"id":"b","rect":"10x10","opacity":0.5}
    ]));
    let (compacted, done) = compact(
        &raw,
        Parts {
            defaults: true,
            styles: false,
            eases: false,
        },
    )
    .unwrap();
    assert!(done.defaults >= 3, "{done:?}");
    let a = compacted["layers"][0].as_object().unwrap();
    assert!(!a.contains_key("opacity") && !a.contains_key("rotate") && !a.contains_key("scale"));
    assert_eq!(compacted["layers"][1]["opacity"], 0.5);
    // The scene keeps a resting value as written, so the command proves each
    // row before leaving it out; the rows met are listed for it.
    let rows = default_rows(&raw).unwrap();
    assert!(rows.iter().any(|r| r == "layer rotate"), "{rows:?}");
    let (kept, _) = compact_keeping(
        &raw,
        Parts {
            defaults: true,
            styles: false,
            eases: false,
        },
        &|r| r == "layer rotate",
    )
    .unwrap();
    assert_eq!(kept["layers"][0]["rotate"], 0);
}

#[test]
fn repeated_effect_stacks_and_animations_become_styles_with_readable_names() {
    let fx = json!([{"glow":30},{"box-blur":14},{"levels":1,"channel":"alpha","input-white":0.1}]);
    let anim = json!({"position":[{"t":"0s","v":[0,0],"curve":"speed(0,0.95,0,0.97)"},{"t":"1s","v":[50,0],"curve":"speed(0,0.95,0,0.97)"}]});
    let layers: Vec<Json> = (0..4)
        .map(|i| json!({"id":format!("l{i}"),"rect":"10x10","fx":fx,"anim":anim,"opacity":0.5}))
        .collect();
    let raw = scene(Json::Array(layers));
    let parts = Parts {
        defaults: false,
        styles: true,
        eases: true,
    };
    let (compacted, done) = compact(&raw, parts).unwrap();
    assert_eq!(done.styles, 2, "{compacted}");
    assert_eq!(done.inheriting, 4);
    let styles = compacted["styles"].as_object().unwrap();
    assert!(styles.contains_key("fx-glow-box-blur-levels"), "{styles:?}");
    assert!(styles.contains_key("anim-position"), "{styles:?}");
    // Once in a style, the curve is written twice: too few to be worth a name.
    assert_eq!(done.eases, 0);
    assert_eq!(
        compacted["layers"][0]["inherits"],
        json!(["fx-glow-box-blur-levels", "anim-position"])
    );
    assert!(compacted.to_string().len() < raw.to_string().len());
    same_scene(&raw, &compacted);
    // Compacting the compact form again changes nothing.
    let (again, _) = compact(&compacted, parts).unwrap();
    assert_eq!(again, compacted);
}

#[test]
fn a_value_written_once_stays_on_its_layer() {
    let raw = scene(json!([{"id":"a","rect":"10x10","fx":[{"glow":30}]}]));
    let (compacted, done) = compact(&raw, Parts::ALL).unwrap();
    assert_eq!(done.styles, 0);
    assert_eq!(compacted, raw);
}

#[test]
fn curves_are_named_by_their_shape() {
    assert_eq!(
        shape_words("speed(0,0.95,0,0.97)").as_deref(),
        Some("still-to-stop-95-97")
    );
    assert_eq!(
        shape_words("speed(2,0.33,0,0.5)").as_deref(),
        Some("moving-to-stop-33-50")
    );
    assert_eq!(
        shape_words("cubic(0.4,0,0.2,1)").as_deref(),
        Some("cubic-40-0-20-100")
    );
    assert_eq!(shape_words("ease-out"), None);
}

#[test]
fn a_curve_many_keys_write_becomes_an_ease_named_by_its_shape() {
    let layers: Vec<Json> = (0..6)
        .map(|i| {
            let curve = "speed(0,0.95,0,0.97)";
            json!({"id":format!("l{i}"),"rect":"10x10",
                   "anim":{"x":[{"t":"0s","v":i,"curve":curve},{"t":"1s","v":i + 50,"curve":curve},{"t":"2s","v":i}]}})
        })
        .collect();
    let raw = scene(Json::Array(layers));
    let (compacted, done) = compact(
        &raw,
        Parts {
            defaults: false,
            styles: true,
            eases: true,
        },
    )
    .unwrap();
    assert_eq!(
        (done.styles, done.eases, done.eased),
        (0, 1, 12),
        "{compacted}"
    );
    assert_eq!(
        compacted["eases"]["still-to-stop-95-97"],
        "speed(0,0.95,0,0.97)"
    );
    assert_eq!(
        compacted["layers"][0]["anim"]["x"][0]["curve"],
        "still-to-stop-95-97"
    );
    same_scene(&raw, &compacted);
}
