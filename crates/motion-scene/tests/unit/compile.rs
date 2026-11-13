//! Unit tests for `src/compile.rs`.

use super::*;
use crate::text::ApproxTextMeasure;

fn launch() -> motion_format::Scene {
    motion_format::parse_document(include_str!("../../../../examples/launch.json"))
        .unwrap()
        .scene
}

#[test]
fn resolves_centre_positions_and_sizes() {
    let g = compile(&launch(), &ApproxTextMeasure);
    assert_eq!(
        (g.width, g.height, g.duration_frames),
        (1920.0, 1080.0, 150.0)
    );
    let t = &g.nodes[0];
    assert_eq!((t.x, t.y), (960.0, 260.0));
    assert!((t.width - 580.8).abs() < 1e-9 && (t.height - 115.2).abs() < 1e-9);
    let c = &g.nodes[1];
    assert_eq!((c.x, c.y, c.width, c.height), (960.0, 700.0, 300.0, 400.0));
}

#[test]
fn converts_behaviours_to_frame_based_entrances() {
    let g = compile(&launch(), &ApproxTextMeasure);
    let e = g.nodes[1].enter.as_ref().unwrap();
    assert_eq!(
        (e.start, e.duration, e.from_offset),
        (15.0, 12.0, (0.0, 60.0))
    );
}

#[test]
fn offsets_point_away_from_the_direction_of_travel() {
    // fade-up starts below (+y), fade-down above (-y), fade-left right of (+x), fade-right left of (-x)
    for (preset, off) in [
        ("fade-up", (0.0, 40.0)),
        ("fade-down", (0.0, -40.0)),
        ("fade-left", (40.0, 0.0)),
        ("fade-right", (-40.0, 0.0)),
        ("fade", (0.0, 0.0)),
    ] {
        let j = format!(
            r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","rect":"10x10","in":"{preset}"}}]}}"#
        );
        let s = motion_format::parse_document(&j).unwrap().scene;
        assert_eq!(
            compile(&s, &ApproxTextMeasure).nodes[0]
                .enter
                .as_ref()
                .unwrap()
                .from_offset,
            off,
            "{preset}"
        );
    }
}

#[test]
fn seconds_convert_to_frames_at_a_non_default_fps() {
    let j = r#"{"motion":1,"fps":60,"dur":"2s","layers":[{"id":"a","rect":"10x10","in":"fade-up 40px 0.25s @0.5s"}]}"#;
    let s = motion_format::parse_document(j).unwrap().scene;
    let g = compile(&s, &ApproxTextMeasure);
    let e = g.nodes[0].enter.as_ref().unwrap();
    // 0.5 s and 0.25 s at 60 fps.
    assert_eq!((e.start, e.duration), (30.0, 15.0));
    assert_eq!((g.fps, g.duration_frames), (60.0, 120.0));
}

#[test]
fn layers_without_entrance_have_none_and_kinds_match() {
    let j = r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"}]}"#;
    let s = motion_format::parse_document(j).unwrap().scene;
    let g = compile(&s, &ApproxTextMeasure);
    assert!(g.nodes[0].enter.is_none());
    assert_eq!(g.nodes[0].kind, NodeKind::Rect);
    assert_eq!(
        launch().layers.len(),
        compile(&launch(), &ApproxTextMeasure).nodes.len()
    );
}
