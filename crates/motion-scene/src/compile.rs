//! The compiler: scene in, scene graph out.
//!
//! Part of step 2 (Compile). It resolves `center` coordinates, measures text,
//! and converts entrance behaviours from seconds-or-frames into frames with a
//! pixel offset to start from.

use motion_format::shorthand::{Behaviour, Coord, Preset};
use motion_format::{Content, Scene};

use crate::graph::{Enter, Node, NodeKind, SceneGraph};
use crate::text::TextMeasure;

/// Compiles `scene` into a [`SceneGraph`], using `measure` to size text.
pub fn compile(scene: &Scene, measure: &dyn TextMeasure) -> SceneGraph {
    let width = f64::from(scene.size.width);
    let height = f64::from(scene.size.height);
    let nodes = scene
        .layers
        .iter()
        .map(|layer| {
            let (kind, w, h) = match &layer.content {
                Content::Text { text, font } => {
                    let (w, h) = measure.measure(text, font);
                    (NodeKind::Text, w, h)
                }
                Content::Rect(r) => (NodeKind::Rect, r.width, r.height),
            };
            Node {
                id: layer.id.clone(),
                kind,
                x: resolve(layer.at.x, width),
                y: resolve(layer.at.y, height),
                width: w,
                height: h,
                enter: layer.enter.as_ref().map(|b| enter(b, scene.fps)),
            }
        })
        .collect();
    SceneGraph {
        width,
        height,
        fps: scene.fps,
        duration_frames: scene.duration.to_frames(scene.fps),
        nodes,
    }
}

/// Turns a coordinate into pixels; `center` is half the canvas extent.
fn resolve(c: Coord, extent: f64) -> f64 {
    match c {
        Coord::Center => extent / 2.0,
        Coord::Px(v) => v,
    }
}

/// Converts a behaviour into frames and a starting offset.
fn enter(b: &Behaviour, fps: f64) -> Enter {
    let d = b.distance;
    // The node starts on the side it came from, so the offset points away
    // from the direction of travel. y grows downwards: fade-up starts below.
    let from_offset = match b.preset {
        Preset::Fade => (0.0, 0.0),
        Preset::FadeUp => (0.0, d),
        Preset::FadeDown => (0.0, -d),
        Preset::FadeLeft => (d, 0.0),
        Preset::FadeRight => (-d, 0.0),
    };
    Enter {
        start: b.start.to_frames(fps),
        duration: b.duration.to_frames(fps),
        easing: b.easing,
        from_offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::ApproxTextMeasure;

    fn launch() -> motion_format::Scene {
        motion_format::parse_document(include_str!("../../../examples/launch.json"))
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
}
