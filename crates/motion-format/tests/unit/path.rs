//! Unit tests for `src/path.rs`.

use super::*;
use crate::Kind;
use proptest::prelude::*;

fn vertex(point: [f64; 2], arriving: [f64; 2], leaving: [f64; 2]) -> Vertex {
    Vertex {
        point,
        arriving,
        leaving,
    }
}

fn corner(x: f64, y: f64) -> Vertex {
    vertex([x, y], [0.0; 2], [0.0; 2])
}

#[test]
fn lines_make_corner_vertices() {
    let path: Path = "M0,0 L10,0 L10,10".parse().unwrap();
    assert!(!path.closed());
    assert_eq!(
        path.vertices().collect::<Vec<_>>(),
        [corner(0.0, 0.0), corner(10.0, 0.0), corner(10.0, 10.0)]
    );
}

#[test]
fn a_cubic_sets_the_handles_either_side_of_it() {
    let path: Path = "M0,0 C5,0 10,5 10,10".parse().unwrap();
    assert_eq!(
        path.vertices().collect::<Vec<_>>(),
        [
            vertex([0.0, 0.0], [0.0; 2], [5.0, 0.0]),
            vertex([10.0, 10.0], [0.0, -5.0], [0.0; 2]),
        ]
    );
}

#[test]
fn a_closed_path_back_at_its_start_joins_its_last_vertex_to_the_first() {
    let path: Path = "M0,0 C5,0 10,5 10,10 C10,5 5,0 0,0 Z".parse().unwrap();
    assert!(path.closed());
    assert_eq!(path.len(), 2);
    let first = path.vertices().next().unwrap();
    assert_eq!(first.arriving, [5.0, 0.0]);
}

#[test]
fn a_closed_path_that_ends_elsewhere_keeps_every_vertex() {
    let path: Path = "M0,0 L10,0 L10,10 Z".parse().unwrap();
    assert!(path.closed());
    assert_eq!(path.len(), 3);
}

#[test]
fn spaces_or_commas_separate_coordinates() {
    let a: Path = "M 0 0 L 10 0 C 1 2 3 4 5 6".parse().unwrap();
    let b: Path = "M0,0 L10,0 C1,2 3,4 5,6".parse().unwrap();
    assert_eq!(a, b);
}

#[test]
fn writing_gives_cubics_that_read_back_the_same() {
    for text in [
        "M0,0 L10,0 L10,10",
        "M0,0 C5,0 10,5 10,10 C10,5 5,0 0,0 Z",
        "M0,0 L10,0 L10,10 Z",
        "M3.5,-2",
    ] {
        let path: Path = text.parse().unwrap();
        let written = path.to_string();
        assert_eq!(
            written.parse::<Path>().unwrap(),
            path,
            "{text} -> {written}"
        );
    }
    let square: Path = "M0,0 L10,0 L10,10 Z".parse().unwrap();
    assert_eq!(
        square.to_string(),
        "M0,0 C0,0 10,0 10,0 C10,0 10,10 10,10 C10,10 0,0 0,0 Z"
    );
}

#[test]
fn an_empty_path_writes_nothing() {
    let path = Path::new(false, []);
    assert!(path.is_empty());
    assert_eq!(path.to_string(), "");
}

#[test]
fn components_hold_six_numbers_per_vertex() {
    let path = Path::new(true, [vertex([1.0, 2.0], [3.0, 4.0], [5.0, 6.0])]);
    assert_eq!(path.components(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let moved = path.with_components(&[0.0; 6]);
    assert!(moved.closed());
    assert_eq!(moved.vertices().next().unwrap(), corner(0.0, 0.0));
}

#[test]
#[should_panic(expected = "six numbers per vertex")]
fn with_components_rejects_a_different_count() {
    Path::new(false, [corner(0.0, 0.0)]).with_components(&[0.0; 12]);
}

#[test]
fn same_shape_needs_the_count_and_the_closure_to_match() {
    let open: Path = "M0,0 L1,1".parse().unwrap();
    let other: Path = "M5,5 C1,1 2,2 3,3".parse().unwrap();
    let closed: Path = "M0,0 L1,1 Z".parse().unwrap();
    let longer: Path = "M0,0 L1,1 L2,2".parse().unwrap();
    assert!(open.same_shape(&other));
    assert!(!open.same_shape(&closed));
    assert!(!open.same_shape(&longer));
}

#[test]
fn errors_say_what_went_wrong() {
    for (text, says) in [
        ("L0,0", "must start with `M x,y`"),
        ("M0", "must start with `M x,y`"),
        ("M0,0 Q1,1 2,2", "a command other than"),
        ("M0,0 L1", "without its points"),
        ("M0,0 C1,1 2,2", "without its points"),
        ("M0,0 Z L1,1", "a command other than"),
    ] {
        let error = text.parse::<Path>().unwrap_err();
        assert_eq!(error.kind, Kind::Syntax, "{text}");
        assert!(error.message.contains(says), "{text}: {}", error.message);
        assert_eq!(error.expected.as_deref(), Some(PATH_FORM));
    }
}

fn coordinate() -> impl Strategy<Value = f64> {
    (-10_000i32..10_000).prop_map(|n| f64::from(n) / 4.0)
}

fn any_vertex() -> impl Strategy<Value = Vertex> {
    prop::array::uniform6(coordinate())
        .prop_map(|n| vertex([n[0], n[1]], [n[2], n[3]], [n[4], n[5]]))
}

proptest! {
    #[test]
    fn reading_never_panics(text in "\\PC{0,40}") {
        let _ = text.parse::<Path>();
    }

    #[test]
    fn reading_command_soup_never_panics(text in "[MLCZ0-9., -]{0,40}") {
        let _ = text.parse::<Path>();
    }

    #[test]
    fn any_open_path_round_trips(vertices in prop::collection::vec(any_vertex(), 1..6)) {
        // An open path's first arriving and last leaving handle draw nothing,
        // so they are not written.
        let mut vertices = vertices;
        vertices[0].arriving = [0.0; 2];
        vertices.last_mut().unwrap().leaving = [0.0; 2];
        let path = Path::new(false, vertices);
        prop_assert_eq!(path.to_string().parse::<Path>().unwrap(), path);
    }

    #[test]
    fn any_closed_path_round_trips(vertices in prop::collection::vec(any_vertex(), 2..6)) {
        let path = Path::new(true, vertices);
        prop_assert_eq!(path.to_string().parse::<Path>().unwrap(), path);
    }
}
