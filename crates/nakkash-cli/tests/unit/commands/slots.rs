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
            seen: true,
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

#[test]
fn coverage_counts_the_slots_marked_unseen() {
    let raw = json!({"nakkash":1,"size":"100x100","dur":"1s",
        "slots":{"a":{"color":"#000000","seen":false},"b":"#FFFFFF"},
        "comps":{"c":{"size":"10x10","dur":"1s","slots":{"d":{"number":1,"seen":false}},"layers":[]}},
        "layers":[]});
    assert_eq!(coverage(&raw).unseen, Count { bound: 2, total: 3 });
}

#[test]
fn marking_writes_what_verify_saw() {
    let dir = std::env::temp_dir().join(format!("nakkash-mark-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scene.json");
    let scene = json!({"nakkash":1,"size":"100x100","dur":"1s",
        "slots":{"a":"#000000","b":{"color":"#FFFFFF","seen":false},"c":{"number":0.5,"label":"Size"}},
        "comps":{"k":{"size":"10x10","dur":"1s","slots":{"d":0.2},"layers":[{"id":"r","rect":"5x5","opacity":"$d"}]}},
        "layers":[{"id":"x","rect":"10x10","fill":"$a","opacity":"$c"},
                  {"id":"y","rect":"10x10","fill":"$b"},
                  {"id":"z","comp":"k"}]});
    fs::write(&path, scene.to_string()).unwrap();
    let session = Session::open(&path).unwrap();
    let one = |comp: Option<&str>, id: &str, ty: &'static str, outcome| Verified {
        comp: comp.map(str::to_owned),
        id: id.into(),
        ty,
        edit: String::new(),
        outcome,
    };
    let verified = Verification {
        slots: vec![
            one(None, "a", "color", Outcome::Unchanged),
            one(
                None,
                "b",
                "color",
                Outcome::Changed {
                    frame: 0,
                    pixels: 9,
                },
            ),
            one(None, "c", "number", Outcome::Skipped("not tried".into())),
            one(Some("k"), "d", "number", Outcome::Unchanged),
        ],
        together: None,
    };
    assert_eq!(mark(&path, &session, &verified).unwrap(), 2);
    let written: Json = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    // A shorthand is written out typed to take its mark; a slot seen to
    // change loses its mark; one not tried keeps what it had.
    assert_eq!(
        written["slots"]["a"],
        json!({"color":"#000000","seen":false})
    );
    assert_eq!(written["slots"]["b"], json!({"color":"#FFFFFF"}));
    assert_eq!(written["slots"]["c"], json!({"number":0.5,"label":"Size"}));
    assert_eq!(
        written["comps"]["k"]["slots"]["d"],
        json!({"number":0.2,"seen":false})
    );
    fs::remove_dir_all(&dir).ok();
}
