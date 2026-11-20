//! Unit tests for `src/relocate.rs`.

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

#[test]
fn relative_files_are_written_from_the_new_folder() {
    let mut raw = json!({"fonts":["fonts","/abs/f.ttf"],
        "comps":{"c":{"layers":[{"id":"p","image":"img/a.png"}]}},
        "layers":[{"id":"v","video":"../clips/b.mov"},{"id":"t","text":"image"}]});
    files(
        &mut raw,
        Path::new("/work/master"),
        Path::new("/work/master/out"),
    );
    assert_eq!(
        raw,
        json!({"fonts":["../fonts","/abs/f.ttf"],
            "comps":{"c":{"layers":[{"id":"p","image":"../img/a.png"}]}},
            "layers":[{"id":"v","video":"../../clips/b.mov"},{"id":"t","text":"image"}]})
    );
}

#[test]
fn a_path_is_relative_to_a_folder() {
    assert_eq!(
        relative(Path::new("/a/b/c.png"), Path::new("/a")).as_deref(),
        Some("b/c.png")
    );
    assert_eq!(
        relative(Path::new("/a/c.png"), Path::new("/a/b/d")).as_deref(),
        Some("../../c.png")
    );
    assert_eq!(
        relative(Path::new("/a"), Path::new("/a")).as_deref(),
        Some(".")
    );
}

#[test]
fn a_reference_to_a_slot_names_no_file_and_slots_and_styles_name_theirs() {
    let mut raw = json!({
        "slots":{"logo":{"image":"brand/logo.png"},"screen":{"image":null}},
        "styles":{"badge":{"image":"img/badge.png"}},
        "layers":[{"id":"s","image":"$screen"},{"id":"l","image":"$logo"}]});
    files(
        &mut raw,
        Path::new("/work/master"),
        Path::new("/work/master/out"),
    );
    assert_eq!(
        raw,
        json!({
            "slots":{"logo":{"image":"../brand/logo.png"},"screen":{"image":null}},
            "styles":{"badge":{"image":"../img/badge.png"}},
            "layers":[{"id":"s","image":"$screen"},{"id":"l","image":"$logo"}]})
    );
}
