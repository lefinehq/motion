//! Unit tests for `src/overrides.rs`.

use super::*;
use serde_json::json;

fn ids(address: &str) -> Vec<Id> {
    address.parse::<Address>().unwrap().layers
}

/// `card` shows `label` with its own overrides; `label` holds a `badge`
/// layer showing `icon`, with an override of the composition's own.
fn scene() -> Value {
    json!({
        "comps": {
            "icon": {"layers": [{"id": "dot", "rect": "1x1", "fill": "#000000"}]},
            "label": {"layers": [
                {"id": "title", "text": "Shared", "fill": "#FFFFFF"},
                {"id": "badge", "comp": "icon", "overrides": {"dot": {"fill": "#00FF00", "rect": "2x2"}}}
            ]}
        },
        "layers": [
            {"id": "plain", "comp": "label"},
            {"id": "card", "comp": "label", "overrides": {"title": {"text": "Sale"}, "badge/dot": {"fill": "#FF0000"}}}
        ]
    })
}

#[test]
fn a_root_layer_is_its_own_json() {
    let raw = scene();
    assert_eq!(layer_json(&raw, &ids("plain")).unwrap()["comp"], "label");
}

#[test]
fn an_inner_layer_has_every_override_on_the_way_set_over_it() {
    let raw = scene();
    let title = layer_json(&raw, &ids("card/title")).unwrap();
    assert_eq!(
        (&title["text"], &title["fill"]),
        (&json!("Sale"), &json!("#FFFFFF"))
    );
    assert_eq!(
        layer_json(&raw, &ids("plain/title")).unwrap()["text"],
        "Shared"
    );
    // The instance's override wins over the composition's own, key by key.
    let dot = layer_json(&raw, &ids("card/badge/dot")).unwrap();
    assert_eq!(
        (&dot["fill"], &dot["rect"]),
        (&json!("#FF0000"), &json!("2x2"))
    );
    let dot = layer_json(&raw, &ids("plain/badge/dot")).unwrap();
    assert_eq!(dot["fill"], "#00FF00");
}

#[test]
fn a_missing_layer_has_no_json() {
    let raw = scene();
    for address in [
        "ghost",
        "card/ghost",
        "card/title/deeper",
        "plain/badge/dot/x",
    ] {
        assert_eq!(layer_json(&raw, &ids(address)), None, "{address}");
    }
    assert_eq!(layer_json(&raw, &[]), None);
}

#[test]
fn set_over_reports_a_missing_first_id_and_changes_nothing() {
    let mut layers = vec![json!({"id": "a"})];
    let keys = json!({"text": "x"}).as_object().unwrap().clone();
    assert!(!set_over(&mut layers, &ids("b"), &keys));
    assert_eq!(layers, [json!({"id": "a"})]);
    assert!(set_over(&mut layers, &ids("a/inner"), &keys));
    assert_eq!(
        layers,
        [json!({"id": "a", "overrides": {"inner": {"text": "x"}}})]
    );
}
