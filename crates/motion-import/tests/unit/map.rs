//! Unit tests for `src/map.rs`: a whole import of the sample project, and
//! how the composition to import is chosen.

use pretty_assertions::assert_eq;

use super::*;
use crate::test_support::sample_project;

/// The sample project imported with `options`.
fn imported(options: &Options) -> Import {
    import(&sample_project(), options).expect("the sample imports")
}

#[test]
fn imports_the_sample_with_its_report() {
    let import = imported(&Options {
        comp: None,
        fonts: vec!["fonts".into()],
        ..Options::default()
    });
    let scene = motion_format::write_document(&import.json);
    insta::assert_snapshot!(format!("{scene}\n{}", import.mapping));
}

#[test]
fn layers_stack_bottom_first_and_keep_their_parent() {
    let import = imported(&Options::default());
    let layers = import.document.scene().layers.clone();
    let ids: Vec<&str> = layers.iter().map(|layer| layer.id.as_str()).collect();
    assert_eq!(ids, ["ctrl", "title"]);
    assert_eq!(layers[1].parent.as_deref(), Some("ctrl"));
    let named: Vec<(&str, &str)> = import
        .mapping
        .ids
        .iter()
        .map(|named| (named.name.as_str(), named.id.as_str()))
        .collect();
    assert_eq!(
        named,
        [("Main", "main"), ("Title", "title"), ("Ctrl", "ctrl")]
    );
}

#[test]
fn reports_what_the_scene_cannot_hold() {
    let mapping = imported(&Options::default()).mapping;
    let what: Vec<&str> = mapping.by_what().into_keys().collect();
    assert_eq!(what, ["3D layers (drawn flat)", "`ADBE Marker`"]);
    assert_eq!(mapping.counts["ADBE Position"].mapped, 1);
    assert_eq!(mapping.counts["ADBE Slider Control"].ignored, 1);
}

#[test]
fn chooses_the_composition_by_name_or_refuses_to_guess() {
    let mut project = sample_project();
    let second = project.items[1].clone();
    project.items.push(Item {
        id: 4,
        name: "Alt".into(),
        ..second
    });
    let error = import(&project, &Options::default()).unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert!(error.message.contains("`Main`, `Alt`"), "{}", error.message);
    let chosen = Options {
        comp: Some("Alt".into()),
        ..Options::default()
    };
    assert_eq!(import(&project, &chosen).unwrap().mapping.scene, "Alt");
    let unknown = Options {
        comp: Some("Nope".into()),
        ..Options::default()
    };
    assert_eq!(
        import(&project, &unknown).unwrap_err().kind,
        Kind::UnknownName
    );
}

#[test]
fn a_project_without_compositions_says_so() {
    let mut project = sample_project();
    project
        .items
        .retain(|item| !matches!(item.kind, ItemKind::Composition(_)));
    let error = import(&project, &Options::default()).unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert_eq!(error.message, "the project holds no composition to import");
}
