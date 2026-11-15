//! Where a scene's sound comes from: each audio node, reached from the
//! scene's own nodes through the `comp` nodes that hold it ([`Route`]),
//! and what it plays at any frame of the scene ([`Route::at`]): how far
//! into its file, and at what gain. A mixer samples routes as finely as it
//! likes; nothing here keeps state between frames.

use std::sync::Arc;

use motion_format::AudioContent;
use motion_format::ValueType;
use motion_format::property::Prop;

use crate::graph::{Node, NodeContent, SceneGraph};

/// The way down to one audio node: its index among the scene's own nodes,
/// then among the nodes of each composition on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    steps: Vec<usize>,
    audio: Arc<AudioContent>,
}

/// What a route plays at one frame of the scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Playing {
    /// How far into the file, in seconds: the audio node's own time.
    pub seconds: f64,
    /// The product of the `volume` of the audio node and of every `comp`
    /// node above it.
    pub gain: f64,
    /// How many seconds of the file play in one second of the scene.
    pub speed: f64,
}

/// Every audio node of `graph`, in paint order, each as a route from the
/// scene's own nodes. A composition shown by several `comp` nodes gives a
/// route through each.
pub fn routes(graph: &SceneGraph) -> Vec<Route> {
    fn collect(graph: &SceneGraph, nodes: &[Node], above: &mut Vec<usize>, all: &mut Vec<Route>) {
        for (i, node) in nodes.iter().enumerate() {
            above.push(i);
            match &node.content {
                NodeContent::Audio(audio) => all.push(Route {
                    steps: above.clone(),
                    audio: Arc::clone(audio),
                }),
                NodeContent::Comp(index) => collect(graph, &graph.comps[*index].nodes, above, all),
                _ => {}
            }
            above.pop();
        }
    }
    let mut all = Vec::new();
    collect(graph, &graph.root.nodes, &mut Vec::new(), &mut all);
    all
}

impl Route {
    /// The id of the audio node it ends at.
    pub fn id<'g>(&self, graph: &'g SceneGraph) -> &'g str {
        let mut nodes = &graph.root.nodes;
        let mut id = "";
        for &i in &self.steps {
            let node = &nodes[i];
            id = &node.id;
            if let NodeContent::Comp(index) = &node.content {
                nodes = &graph.comps[*index].nodes;
            }
        }
        id
    }

    /// The file it plays, as the scene writes it.
    pub fn path(&self) -> &str {
        &self.audio.path
    }

    /// What it plays at the scene's `frame`, or `None` when it is silent
    /// there: a node on the way is hidden or outside its span, or a
    /// composition on the way is outside its own length.
    pub fn at(&self, graph: &SceneGraph, frame: f64) -> Option<Playing> {
        let mut nodes = &graph.root.nodes;
        let mut fps = graph.root.fps;
        let mut frame = frame;
        let mut gain = 1.0;
        let mut speed = 1.0;
        for &i in &self.steps {
            let node = &nodes[i];
            if node.hidden || !node.timing.shows_at(frame) {
                return None;
            }
            let local = node.timing.local(frame);
            let (lowest, highest) = Prop::VOLUME.spec().range.unwrap_or((0.0, f64::MAX));
            gain *= node.tracks[Prop::VOLUME.index()]
                .sample(local, ValueType::Number)
                .as_number()
                .unwrap_or(1.0)
                .clamp(lowest, highest);
            speed /= node.timing.stretch;
            match &node.content {
                NodeContent::Comp(index) => {
                    let comp = &graph.comps[*index];
                    let inner = local * comp.fps / fps;
                    if !(0.0..comp.duration_frames).contains(&inner) {
                        return None;
                    }
                    nodes = &comp.nodes;
                    fps = comp.fps;
                    frame = inner;
                }
                _ => {
                    return Some(Playing {
                        seconds: local / fps,
                        gain,
                        speed,
                    });
                }
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "../tests/unit/audio.rs"]
mod tests;
