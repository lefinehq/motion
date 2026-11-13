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
        curve: b.curve,
        from_offset,
    }
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
