//! Unit tests for `src/tree.rs`.

use crate::model::{Path, Switches, Value, Vertex};
use crate::test_support::sample_project;

#[test]
fn prints_items_layers_and_their_properties() {
    insta::assert_snapshot!(sample_project().to_string());
}

#[test]
fn prints_paths_by_their_shape() {
    let vertex = Vertex {
        point: [0.0, 0.0],
        in_tangent: [0.0, 0.0],
        out_tangent: [0.0, 0.0],
    };
    let path = Value::Path(Path {
        closed: false,
        vertices: vec![vertex; 3],
    });
    assert_eq!(path.to_string(), "path open, 3 vertices");
}

#[test]
fn prints_every_switch() {
    let fields = serde_json::to_value(Switches::default()).unwrap();
    let printed = super::printed(&Switches::default());
    // How a layer turns by itself prints by its name, not as a switch.
    assert_eq!(printed.len(), fields.as_object().unwrap().len() - 1);
}
