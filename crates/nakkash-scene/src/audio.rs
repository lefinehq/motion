//! Where a scene's sound comes from: each audio node, reached from the
//! scene's own nodes through the `comp` nodes that hold it ([`Route`]),
//! and what it plays at any frame of the scene ([`Route::at`]): how far
//! into its file, and at what gain. A mixer samples routes as finely as it
//! likes; nothing here keeps state between frames.

use std::sync::Arc;

use nakkash_format::AudioContent;
use nakkash_format::ValueType;
use nakkash_format::property::Prop;

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
    let mut all = Vec::new();
    for i in 0..graph.root.nodes.len() {
        collect(graph, &graph.root.nodes, i, &mut Vec::new(), &mut all);
    }
    all
}

/// The routes of the sound node `index` of `nodes`, one canvas, plays: its
/// own when it is an audio node, every one its composition plays when it
/// is a `comp` node, none otherwise. Each starts at `nodes`, so it is read
/// with [`Route::at_in`] on that canvas.
pub fn routes_from(graph: &SceneGraph, nodes: &[Node], index: usize) -> Vec<Route> {
    let mut all = Vec::new();
    collect(graph, nodes, index, &mut Vec::new(), &mut all);
    all
}

/// Adds the routes down from node `i` of `nodes`, below `above`, to `all`.
fn collect(
    graph: &SceneGraph,
    nodes: &[Node],
    i: usize,
    above: &mut Vec<usize>,
    all: &mut Vec<Route>,
) {
    above.push(i);
    match &nodes[i].content {
        NodeContent::Audio(audio) => all.push(Route {
            steps: above.clone(),
            audio: Arc::clone(audio),
        }),
        NodeContent::Comp(index) => {
            let inner = &graph.comps[*index].nodes;
            for j in 0..inner.len() {
                collect(graph, inner, j, above, all);
            }
        }
        _ => {}
    }
    above.pop();
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
        self.at_in(graph, &graph.root.nodes, graph.root.fps, frame)
    }

    /// What it plays at `frame` of the canvas of `nodes`, whose clock runs
    /// at `fps`, for a route starting there (see [`routes_from`]).
    pub fn at_in(
        &self,
        graph: &SceneGraph,
        nodes: &[Node],
        fps: f64,
        frame: f64,
    ) -> Option<Playing> {
        let mut nodes = nodes;
        let mut fps = fps;
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
                    let remap = |at: f64| {
                        node.tracks[Prop::REMAP.index()]
                            .sample(at, ValueType::Number)
                            .as_number()
                            .unwrap_or(0.0)
                    };
                    let content = node.timing.content(local, remap(local), fps);
                    if node.timing.remaps {
                        // Its sound runs as fast as the time it remaps to.
                        speed *= (remap(local + 0.5) - remap(local - 0.5)) * fps;
                    }
                    let inner = content * comp.fps / fps;
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
