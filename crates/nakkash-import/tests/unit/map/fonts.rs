//! Unit tests for `src/map/fonts.rs`: naming a font by its face's own name,
//! substituting a font the import was not given, and reporting both.

use pretty_assertions::assert_eq;

use super::*;
use crate::map::{Finding, Options, import};
use crate::test_support::sample_project;

/// The sample project imported, its title set in `Inter-Bold`.
fn imported() -> Import {
    import(&sample_project(), &Options::default()).expect("the sample imports")
}

/// One substitution.
fn swap(from: &str, to: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(from.to_owned(), to.to_owned())])
}

#[test]
fn the_import_lists_every_font_with_where_it_is_used() {
    let import = imported();
    let fonts: Vec<(&str, usize)> = import
        .fonts
        .iter()
        .map(|(font, places)| (font.as_str(), places.len()))
        .collect();
    assert_eq!(fonts, [("Inter-Bold", 1)]);
}

#[test]
fn a_substituted_font_is_written_and_reported_where_it_is_used() {
    let mut import = imported();
    let at = import.fonts["Inter-Bold"].clone();
    import
        .substitute_fonts(&swap("Inter-Bold", "InterVariable-Bold"))
        .expect("the scene stays valid");
    assert_eq!(import.json["layers"][1]["font"], "InterVariable-Bold 44");
    let title = &import.document.scene().root.layers[1];
    let nakkash_format::Content::Text(text) = &title.content else {
        panic!("the title is text");
    };
    assert_eq!(text.base.font.family, "InterVariable-Bold");
    let substituted: Vec<Finding> = import
        .mapping
        .unsupported
        .iter()
        .filter(|f| f.what.starts_with("missing fonts"))
        .cloned()
        .collect();
    assert_eq!(
        substituted,
        at.iter()
            .map(|at| Finding {
                at: at.clone(),
                what: "missing fonts, substituted (`Inter-Bold → InterVariable-Bold`)".into(),
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        import.fonts.keys().collect::<Vec<_>>(),
        ["InterVariable-Bold"]
    );
    assert!(import.mapping.to_string().contains(&format!(
        "missing fonts, substituted (`Inter-Bold → InterVariable-Bold`) (1): {}",
        at[0]
    )));
}

#[test]
fn a_font_the_scene_does_not_use_or_keeps_changes_nothing() {
    let before = imported();
    for substitutes in [
        swap("Other-Bold", "InterVariable-Bold"),
        swap("Inter-Bold", "Inter-Bold"),
    ] {
        let mut import = imported();
        import.substitute_fonts(&substitutes).expect("valid");
        assert_eq!(import.json, before.json);
        assert_eq!(import.mapping, before.mapping);
        import.rename_fonts(&substitutes).expect("valid");
        assert_eq!(import.json, before.json);
        assert_eq!(import.mapping, before.mapping);
    }
}

#[test]
fn a_font_named_by_another_name_of_its_face_is_renamed_and_listed_not_counted() {
    let mut import = imported();
    let unsupported = import.mapping.unsupported.clone();
    import
        .rename_fonts(&swap("Inter-Bold", "InterVariable-Bold"))
        .expect("the scene stays valid");
    assert_eq!(import.json["layers"][1]["font"], "InterVariable-Bold 44");
    let title = &import.document.scene().root.layers[1];
    let nakkash_format::Content::Text(text) = &title.content else {
        panic!("the title is text");
    };
    assert_eq!(text.base.font.family, "InterVariable-Bold");
    assert_eq!(import.mapping.unsupported, unsupported);
    assert_eq!(
        import.mapping.renamed_fonts,
        swap("Inter-Bold", "InterVariable-Bold")
    );
    assert_eq!(
        import.fonts.keys().collect::<Vec<_>>(),
        ["InterVariable-Bold"]
    );
    assert!(import.mapping.to_string().contains(
        "  fonts under another name of their face: `Inter-Bold` is `InterVariable-Bold`\n"
    ));
    let json = serde_json::to_value(&import.mapping).expect("a report");
    assert_eq!(json["renamed_fonts"]["Inter-Bold"], "InterVariable-Bold");
}

#[test]
fn a_report_without_renamed_fonts_leaves_them_out() {
    let json = serde_json::to_value(&imported().mapping).expect("a report");
    assert!(json.get("renamed_fonts").is_none());
    assert!(!imported().mapping.to_string().contains("another name"));
}
