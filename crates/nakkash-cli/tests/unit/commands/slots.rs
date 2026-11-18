//! Unit tests for `src/commands/slots.rs`.

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

#[test]
fn coverage_counts_what_slots_hold() {
    let raw = json!({"nakkash":1,"size":"100x100","dur":"1s","bg":"$background",
    "slots":{"title":{"text":"Hi"},"background":"#000000","accent":"#E63946",
        "logo":{"image":"logo.png"}},
    "layers":[
        {"id":"t","text":"$title","fill":"$accent l+0.1"},
        {"id":"u","text":"Loose","fill":"#FFFFFF"},
        {"id":"p","image":"$logo","box":[10,10]},
        {"id":"q","image":"pic.png","box":[10,10]},
        {"id":"h","text":"Left out","hidden":true}
    ]});
    let coverage = coverage(&raw);
    assert_eq!(coverage.text, Count { bound: 1, total: 2 });
    assert_eq!(coverage.media, Count { bound: 1, total: 2 });
    // Black and the changed red are held; white is not.
    assert_eq!(coverage.colors, Count { bound: 2, total: 3 });
    assert!(
        coverage
            .unbound
            .iter()
            .any(|u| u.starts_with("layers[1].fill")),
        "{:?}",
        coverage.unbound
    );
}

#[test]
fn a_layer_shown_in_a_composition_is_named_after_it() {
    assert_eq!(shown_key(Some("card"), "title"), "card/title");
    assert_eq!(shown_key(None, "title"), "title");
}

#[test]
fn followers_name_their_layers() {
    assert_eq!(layer_of("title.text"), Some((None, "title")));
    assert_eq!(
        layer_of("comps.comp-01-edit.layers[sample].text"),
        Some((Some("comp-01-edit"), "sample"))
    );
    assert_eq!(layer_of("dur"), None);
    assert!(in_edit_comp("comps.comp-01-edit-text.layers[sample].text"));
    assert!(!in_edit_comp("comps.bg-fx.layers[x].fill"));
}

#[test]
fn a_neutral_is_changed_into_a_colour() {
    let listed = Listed {
        comp: None,
        slot: nakkash_format::Slot {
            id: "background".parse().unwrap(),
            ty: SlotType::Color,
            value: json!("#000000"),
            label: None,
            role: None,
        },
        followers: Vec::new(),
    };
    let to = changed(&listed, Path::new("x.png"), None).unwrap();
    assert_ne!(to, "#000000FF");
    assert!(to.starts_with('#'));
}

#[test]
fn frames_spread_over_the_length() {
    assert_eq!(spread(80, 8), vec![5, 15, 25, 35, 45, 55, 65, 75]);
    assert_eq!(spread(3, 8), vec![0, 1, 2]);
    assert_eq!(spread(0, 8), vec![0]);
}

#[test]
fn a_slot_is_drawn_where_its_layers_show() {
    let samples: Vec<u32> = (0..10).map(|i| i * 10).collect();
    let set = |ids: &[&str]| {
        ids.iter()
            .map(|s| (*s).to_owned())
            .collect::<BTreeSet<String>>()
    };
    let mut shown = vec![set(&[]); 10];
    shown[3] = set(&["word"]);
    shown[4] = set(&["word", "other"]);
    shown[7] = set(&["word", "other"]);
    let both = set(&["word", "other"]);
    // Where both show first, then where one does.
    assert_eq!(where_shown(&samples, &shown, &both, 100), vec![40, 70, 30]);
    // Where none shows, frames spread over the length.
    let none = set(&["gone"]);
    assert_eq!(where_shown(&samples, &shown, &none, 100).len(), CANDIDATES);
}
