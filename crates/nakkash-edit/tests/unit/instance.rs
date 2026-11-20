use nakkash_format::{Document, canon};
use serde_json::{Value as Json, json};

use super::*;

/// A small template: a title, an accent, a screen that takes clips, a
/// choice of layout, a bounded number and a composition's own slot.
fn template() -> Json {
    json!({
        "nakkash": 1, "size": "1920x1080", "dur": "4s",
        "slots": {
            "title": {"text": "Launch", "role": "title", "uid": "u-title"},
            "accent": {"color": "#E63946", "role": "accent"},
            "screen": {"image": null, "kind": "screen", "accepts": ["image", "video"]},
            "layout": {"choice": "a", "values": ["a", "b"]},
            "speed": {"number": 1, "min": 0.5, "max": 2},
            "locked": {"color": "#000000"}
        },
        "locks": ["$locked"],
        "looks": {"dark": {"accent": "#2A9D8F"}},
        "comps": {
            "a": {"size": "1920x1080", "dur": "4s", "slots": {"tint": "#FF00FF"},
                  "layers": [{"id": "plate", "rect": "100x100", "fill": "$tint"}]},
            "b": {"size": "1920x1080", "dur": "4s", "layers": []}
        },
        "layers": [
            {"id": "section", "comp": "$layout"},
            {"id": "headline", "text": "$title", "font": "Inter Variable 96 700", "fill": "$accent"},
            {"id": "phone", "image": "$screen", "box": [100, 200], "fit": "cover"},
            {"id": "dot", "rect": "10x10", "fill": "$locked", "opacity": 0.5}
        ]
    })
}

fn doc() -> Document {
    Document::from_value(template()).expect("the template is valid")
}

fn instance(body: Json) -> Instance {
    let mut json =
        json!({"schema": SCHEMA, "template": {"path": "t.json", "hash": canon::hash(&template())}});
    json.as_object_mut()
        .expect("object")
        .extend(body.as_object().expect("object").clone());
    Instance::read(&json).expect("the instance reads")
}

fn merged(body: Json) -> Merged {
    merge(&doc(), &canon::hash(&template()), &instance(body), &|p| {
        p.to_owned()
    })
}

fn codes(m: &Merged) -> Vec<&'static str> {
    m.problems.iter().map(|p| p.code).collect()
}

#[test]
fn an_empty_instance_is_the_template() {
    let m = merged(json!({}));
    assert!(m.problems.is_empty(), "{:?}", m.problems);
    let raw = m.raw.expect("merged");
    assert_eq!(raw, template());
    assert_eq!(
        Document::from_value(raw).expect("valid").scene(),
        doc().scene()
    );
}

#[test]
fn values_equal_to_the_template_draw_the_template() {
    let m = merged(
        json!({"slots": {"title": "Launch", "accent": "#E63946", "layout": "a", "speed": 1, "a/tint": "#FF00FF"}}),
    );
    assert!(m.problems.is_empty(), "{:?}", m.problems);
    assert_eq!(
        Document::from_value(m.raw.expect("merged"))
            .expect("valid")
            .scene(),
        doc().scene()
    );
}

#[test]
fn values_reach_their_declarations_in_the_form_written() {
    let m = merged(
        json!({"slots": {"title": "Jeder Markt", "accent": "#0B6E4F", "a/tint": "#123456", "layout": "b"}}),
    );
    assert!(m.problems.is_empty(), "{:?}", m.problems);
    let raw = m.raw.expect("merged");
    assert_eq!(
        raw["slots"]["title"],
        json!({"text": "Jeder Markt", "role": "title", "uid": "u-title"})
    );
    assert_eq!(raw["slots"]["accent"]["color"], json!("#0B6E4F"));
    assert_eq!(raw["comps"]["a"]["slots"]["tint"], json!("#123456"));
    assert_eq!(raw["slots"]["layout"]["choice"], json!("b"));
}

#[test]
fn every_unknown_id_is_reported_at_once_with_the_nearest() {
    let m = merged(json!({"slots": {"tittle": "x", "accentt": "#000"}}));
    assert_eq!(codes(&m), ["slot.unknown", "slot.unknown"]);
    assert!(
        m.problems[0]
            .hint
            .as_deref()
            .is_some_and(|h| h.contains("`title`")),
        "{:?}",
        m.problems[0]
    );
    assert!(m.raw.is_none());
}

#[test]
fn a_locked_slot_and_a_value_out_of_bounds_are_refused_together() {
    let m = merged(json!({"slots": {"locked": "#FFFFFF", "speed": 3, "layout": "c", "title": 5}}));
    assert_eq!(
        codes(&m),
        [
            "slot.locked",
            "value.out_of_range",
            "choice.unknown",
            "type.mismatch"
        ]
    );
    let range = &m.problems[1];
    assert_eq!(range.rule.as_deref(), Some("max"));
    assert_eq!(range.measured, Some(json!(3)));
}

#[test]
fn hide_hides_what_follows_and_unset_keeps_the_template() {
    let m = merged(json!({"slots": {"title": {"$hide": true}, "accent": {"$unset": true}}}));
    assert!(m.problems.is_empty(), "{:?}", m.problems);
    let raw = m.raw.expect("merged");
    assert_eq!(raw["layers"][1]["hidden"], json!(true));
    assert_eq!(raw["slots"]["accent"], template()["slots"]["accent"]);
}

#[test]
fn a_picture_slot_that_accepts_clips_plays_one() {
    let m = merged(json!({"slots": {"screen": {"video": "app.mp4"}}}));
    assert!(m.problems.is_empty(), "{:?}", m.problems);
    let raw = m.raw.expect("merged");
    assert_eq!(raw["slots"]["screen"]["video"], json!("app.mp4"));
    assert!(raw["slots"]["screen"].get("image").is_none());
    assert_eq!(raw["layers"][2]["video"], json!("$screen"));
    assert!(raw["layers"][2].get("image").is_none());
    let refused = merged(json!({"slots": {"title": {"video": "x.mp4"}}}));
    assert_eq!(codes(&refused), ["type.mismatch"]);
}

#[test]
fn a_look_is_under_the_instance() {
    let m = merged(json!({"look": "dark"}));
    assert_eq!(
        m.raw.expect("merged")["slots"]["accent"]["color"],
        json!("#2A9D8F")
    );
    let m = merged(json!({"look": "dark", "slots": {"accent": "#FFFFFF"}}));
    assert_eq!(
        m.raw.expect("merged")["slots"]["accent"]["color"],
        json!("#FFFFFF")
    );
    assert_eq!(codes(&merged(json!({"look": "drak"}))), ["look.unknown"]);
}

#[test]
fn raw_sets_static_values_and_never_motion_timing_or_effects() {
    let m = merged(json!({"raw": [{"at": "dot.opacity", "expect": "number", "value": 0.8}]}));
    assert_eq!(codes(&m), ["raw.applied"]);
    assert_eq!(m.raw.expect("merged")["layers"][3]["opacity"], json!(0.8));
    for (at, expect, value) in [
        ("dot.anim", "json", json!({})),
        ("dot.span", "string", json!("0s-1s")),
        ("dot.fx", "json", json!([])),
        ("scene.dur", "string", json!("2s")),
    ] {
        let m = merged(json!({"raw": [{"at": at, "expect": expect, "value": value}]}));
        assert_eq!(codes(&m), ["raw.refused"], "{at}");
    }
    let wrong = merged(json!({"raw": [{"at": "dot.opacity", "expect": "number", "value": "x"}]}));
    assert_eq!(codes(&wrong), ["raw.refused"]);
}

#[test]
fn a_changed_template_warns_and_a_moved_uid_relinks() {
    let hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let mut stale = instance(json!({"slots": {"accent": "#000000"}}));
    stale.template.hash = hash.into();
    let m = merge(&doc(), &canon::hash(&template()), &stale, &|p| p.to_owned());
    assert_eq!(codes(&m), ["template.changed"]);
    assert_eq!(m.relinked.template.hash, canon::hash(&template()));

    let mut moved = instance(json!({"slots": {"headline": "Hallo"}}));
    moved
        .template
        .uids
        .insert("headline".into(), "u-title".into());
    let m = merge(&doc(), &canon::hash(&template()), &moved, &|p| p.to_owned());
    assert_eq!(codes(&m), ["target.relinked"]);
    assert_eq!(
        m.raw.expect("merged")["slots"]["title"]["text"],
        json!("Hallo")
    );
    assert!(m.relinked.slots.contains_key("title"));

    let mut gone = instance(json!({"slots": {"subtitle": "x"}}));
    gone.template
        .uids
        .insert("subtitle".into(), "u-gone".into());
    let m = merge(&doc(), &canon::hash(&template()), &gone, &|p| p.to_owned());
    assert_eq!(codes(&m), ["target.stale"]);
}

#[test]
fn the_schema_is_closed() {
    let read = Instance::read(
        &json!({"schema": "nakkash.Instance.9", "template": {"path": "t.json"}, "slot": {}}),
    );
    let problems = read.expect_err("refused");
    let codes: Vec<_> = problems.iter().map(|p| p.code).collect();
    assert_eq!(
        codes,
        [
            "schema.unknown_field",
            "schema.version_unsupported",
            "schema.invalid"
        ]
    );
}

#[test]
fn an_instance_writes_back_as_it_reads() {
    let i = instance(
        json!({"look": "dark", "slots": {"title": "x"}, "sections": {"section": {"role": "hook"}}, "raw": [{"at": "dot.opacity", "expect": "number", "value": 1}]}),
    );
    assert_eq!(Instance::read(&i.to_json()).expect("reads"), i);
}

#[test]
fn values_equal_to_the_template_are_left_out_when_written() {
    let mut i = instance(
        json!({"slots": {"title": "Launch", "accent": "#e63946", "speed": 1.5, "layout": {"$unset": true}}}),
    );
    elide(&mut i, &doc());
    assert_eq!(i.slots.keys().collect::<Vec<_>>(), ["speed"]);
}

#[test]
fn a_problem_is_one_object() {
    let m = merged(json!({"slots": {"speed": 9}}));
    let p = m.problems[0].to_json();
    assert_eq!(p["code"], json!("value.out_of_range"));
    assert_eq!(p["slot"], json!("speed"));
    assert_eq!(p["path"], json!("slots.speed"));
    assert_eq!(p["stage"], json!(3));
    assert_eq!(p["level"], json!("error"));
}
