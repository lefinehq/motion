//! Unit tests for `src/map/ids.rs`.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn names_become_ids_that_follow_the_scene_rule() {
    let mut ids = Ids::default();
    let given: Vec<String> = [
        "Keep your life simple",
        "#hashtag",
        "Shape Layer 1",
        "Shape Layer 1",
        "Shape Layer 1",
        "3D Title",
        "  ",
        "Größe",
    ]
    .iter()
    .map(|name| ids.give(name, "layer"))
    .collect();
    assert_eq!(
        given,
        [
            "keep-your-life-simple",
            "hashtag",
            "shape-layer-1",
            "shape-layer-1-2",
            "shape-layer-1-3",
            "layer-3d-title",
            "layer",
            "gr-e",
        ]
    );
}

#[test]
fn scopes_give_ids_on_their_own() {
    let (mut a, mut b) = (Ids::default(), Ids::default());
    assert_eq!(a.give("Title", "layer"), b.give("Title", "layer"));
}
