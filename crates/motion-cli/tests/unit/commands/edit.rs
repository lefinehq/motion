//! Unit tests for `src/commands/edit.rs`.

use super::*;

#[test]
fn temp_path_is_a_sibling_with_a_suffix() {
    assert_eq!(
        temp_path(Path::new("dir/scene.json")),
        Path::new("dir/scene.json.tmp")
    );
    assert_eq!(
        temp_path(Path::new("scene.json")),
        Path::new("scene.json.tmp")
    );
}
