//! Unit tests for `src/tree.rs`.

use crate::model::{Path, Value, Vertex};
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
