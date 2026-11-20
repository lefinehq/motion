use super::*;
use serde_json::json;

#[test]
fn a_split_scene_flattens_back_to_itself() {
    let raw = json!({"nakkash":1,"comps":{"card":{"size":"100x100","layers":[{"id":"a","rect":"10x10"}]},
                     "end":{"size":"100x100","layers":[]}},"layers":[{"id":"c","comp":"card"}]});
    let (top, files) = split(&raw);
    assert_eq!(top["comps"]["card"], json!({"include":"comps/card.json"}));
    assert_eq!(files.len(), 2);
    assert!(has_includes(&top));
    let flat = flatten(&top, |path| {
        let (_, comp) = files.iter().find(|(p, _)| p == path).expect("a file");
        Ok(comp.to_string())
    })
    .unwrap();
    // Key order too, which the scene reads and its hash covers.
    assert_eq!(flat.to_string(), raw.to_string());
    assert!(!has_includes(&flat));
}

#[test]
fn an_include_beside_other_keys_or_of_another_include_is_an_error() {
    let beside = json!({"comps":{"card":{"include":"comps/card.json","size":"1x1"}}});
    let error = flatten(&beside, |_| Ok("{}".into())).unwrap_err();
    assert_eq!(error.path_text().as_deref(), Some("comps.card"));
    let nested = json!({"comps":{"card":{"include":"comps/card.json"}}});
    let error = flatten(&nested, |_| Ok(r#"{"include":"x.json"}"#.into())).unwrap_err();
    assert_eq!(error.path_text().as_deref(), Some("comps.card.include"));
    let missing = flatten(&nested, |path| {
        Err(Diagnostic::new(Kind::Io, format!("cannot read `{path}`")))
    })
    .unwrap_err();
    assert_eq!(missing.path_text().as_deref(), Some("comps.card.include"));
}

#[test]
fn a_split_scene_read_without_its_files_says_how_to_open_it() {
    let top = json!({"nakkash":1,"comps":{"card":{"include":"comps/card.json"}},"layers":[]});
    let error = crate::parse_document(&top.to_string()).unwrap_err();
    assert!(error.to_string().contains("flatten"), "{error}");
}
