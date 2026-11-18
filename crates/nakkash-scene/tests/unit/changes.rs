//! Unit tests for `src/changes.rs`.

use super::*;
use crate::compile;

fn graph(json: &str) -> SceneGraph {
    compile(
        nakkash_format::parse_document(json).unwrap().scene(),
        &mut nakkash_text::Typesetter::new(nakkash_text::FontSet::builtin()),
    )
    .unwrap()
}

/// The paths, as addresses, that `changed_paths` flags between `before`
/// and `after`, scenes whose layers are given.
fn flagged(before: &str, after: &str) -> Option<Vec<String>> {
    let scene = |layers: &str| {
        format!(
            r#"{{"nakkash":1,"size":"1000x1000","dur":"1s",
  "comps":{{"card":{{"size":"400x200","dur":"1s","layers":[
    {{"id":"title","rect":"10x10"}},{{"id":"under","rect":"10x10","parent":"title"}}]}}}},
  "layers":{layers}}}"#
        )
    };
    let (before, after) = (graph(&scene(before)), graph(&scene(after)));
    let paths = after.paths();
    changed_paths(&before, &after).map(|flags| {
        paths
            .iter()
            .zip(flags)
            .filter(|(_, flag)| *flag)
            .map(|(ids, _)| ids.join("/"))
            .collect()
    })
}

const TWO: &str =
    r#"[{"id":"back","rect":"10x10"},{"id":"card","comp":"card"},{"id":"other","comp":"card"}]"#;

#[test]
fn the_same_scene_changes_nothing() {
    assert_eq!(flagged(TWO, TWO), Some(vec![]));
}

#[test]
fn a_layer_changes_alone() {
    let after = TWO.replace(
        r#"{"id":"back","rect":"10x10"}"#,
        r#"{"id":"back","rect":"20x10"}"#,
    );
    assert_eq!(flagged(TWO, &after), Some(vec!["back".to_owned()]));
}

#[test]
fn a_composition_layer_changes_everything_inside_that_instance() {
    let after = TWO.replace(
        r#"{"id":"card","comp":"card"}"#,
        r#"{"id":"card","comp":"card","opacity":0.5}"#,
    );
    assert_eq!(
        flagged(TWO, &after),
        Some(vec![
            "card".into(),
            "card/title".into(),
            "card/under".into()
        ])
    );
}

#[test]
fn a_layer_parented_to_one_that_changed_changes_too() {
    let before = r#"[{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10","parent":"a"},{"id":"c","rect":"10x10"}]"#;
    let after = before.replace(
        r#"{"id":"a","rect":"10x10"}"#,
        r#"{"id":"a","rect":"10x10","position":[5,5]}"#,
    );
    assert_eq!(flagged(before, &after), Some(vec!["a".into(), "b".into()]));
}

#[test]
fn a_layer_matted_by_one_that_changed_changes_too() {
    let before = r#"[{"id":"m","rect":"10x10"},{"id":"b","rect":"10x10","matte":"m"},{"id":"c","rect":"10x10"}]"#;
    let after = before.replace(
        r#"{"id":"m","rect":"10x10"}"#,
        r#"{"id":"m","rect":"30x10"}"#,
    );
    assert_eq!(flagged(before, &after), Some(vec!["m".into(), "b".into()]));
}

#[test]
fn a_new_layer_changes_the_shape() {
    let after = TWO.replace("]", r#",{"id":"new","rect":"10x10"}]"#);
    assert_eq!(flagged(TWO, &after), None);
}

#[test]
fn a_layer_showing_another_composition_changes_the_shape() {
    let before = r#"[{"id":"x","rect":"10x10"}]"#;
    let after = r#"[{"id":"x","comp":"card"}]"#;
    assert_eq!(flagged(before, after), None);
}

#[test]
fn an_override_changes_its_layer_in_that_instance_alone() {
    let after = TWO.replace(
        r#"{"id":"card","comp":"card"}"#,
        r#"{"id":"card","comp":"card","overrides":{"title":{"opacity":0.5}}}"#,
    );
    assert_eq!(
        flagged(TWO, &after),
        Some(vec!["card/title".into(), "card/under".into()])
    );
}

#[test]
fn a_camera_moved_by_its_parent_changes_every_layer_it_sees() {
    let before = r#"[{"id":"rig","pivot":true,"3d":true},
        {"id":"cam","camera":true,"zoom":300,"z":-400,"parent":"rig"},
        {"id":"a","rect":"10x10","3d":true},{"id":"b","rect":"10x10"}]"#;
    let after = before.replace(
        r#"{"id":"rig","pivot":true,"3d":true}"#,
        r#"{"id":"rig","pivot":true,"3d":true,"position":[40,0]}"#,
    );
    assert_eq!(
        flagged(before, &after),
        Some(vec!["rig".into(), "cam".into(), "a".into(), "b".into()])
    );
}
