//! One walk over every node of an evaluated frame, at any depth: the nodes
//! of the scene's root, and inside each shown `comp` node the nodes of the
//! composition it draws ([D43]).
//!
//! Checks, queries and reports read the tree through [`walk`] instead of
//! each following compositions on their own. The renderer keeps its own
//! descent, because it builds a tree back (a composition is a clipped,
//! isolated layer, and a matte draws inside the layer it masks), where a
//! walk hands out one node at a time.
//!
//! [D43]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d43-one-layer-address-and-edits-per-instance

use kurbo::{Affine, Rect};
use motion_format::Address;
use motion_format::address::Id;
use motion_format::property::Prop;

use crate::space;
use crate::state::box_through;
use crate::{CompGraph, ContentState, FrameState, NodeContent, NodeState, SceneGraph};
use glam::DMat3;
use motion_format::shorthand::MatteMode;

/// One node met on a [`walk`].
#[derive(Debug, Clone, PartialEq)]
pub struct Visit<'a> {
    /// Its place in [`SceneGraph::paths`], the same at every frame, so a
    /// caller can gather what it sees of one node over many frames.
    pub index: usize,
    /// The ids from the scene's root down to the node: the `comp` nodes it
    /// sits in, outermost first, then its own.
    pub ids: Vec<&'a str>,
    /// Its state at the frame, in its own composition's canvas.
    pub node: &'a NodeState,
    /// From the node's own coordinates to the scene's canvas: its transform,
    /// then that of each `comp` node around it. A 3D node, or one inside a
    /// 3D `comp` node, is seen straight on here; [`Visit::plane`] sees it
    /// through the cameras.
    pub transform: Affine,
    /// From the node's own coordinates to the scene's canvas as drawn: its
    /// plane through its composition's camera when it is 3D, then each
    /// `comp` node around it the same way.
    pub plane: DMat3,
    /// A camera is on the way: it, or a `comp` node around it, is 3D, so
    /// [`Visit::plane`] is not [`Visit::transform`].
    pub projected: bool,
    /// Whether it draws in the frame: it, and every `comp` node around it,
    /// is [visible](NodeState::visible).
    pub visible: bool,
    /// Its opacity times that of every `comp` node around it.
    pub opacity: f64,
    /// The part of the scene's canvas it may draw in: each `comp` node around
    /// it clips to its composition's canvas, and an alpha or luma matte, on
    /// it or on a `comp` node around it, to the box its matte layer draws.
    /// `None` when nothing limits it.
    pub clip: Option<Rect>,
}

impl Visit<'_> {
    /// How many `comp` nodes it sits in: 0 for a node of the scene's root.
    pub fn depth(&self) -> usize {
        self.ids.len() - 1
    }

    /// Its address, as edits and diagnostics write it: `card/title`.
    ///
    /// # Panics
    /// If an id does not follow the id rule, which validation rules out.
    pub fn address(&self) -> Address {
        address_of(&self.ids)
    }

    /// The axis-aligned box its content covers on the scene's canvas, before
    /// any clip.
    ///
    /// Through cameras, what is behind them is left out; a node wholly
    /// behind one covers an empty box where its origin is seen straight on.
    pub fn bounds(&self) -> Rect {
        if !self.projected {
            return box_through(self.transform, self.node.local_bounds);
        }
        space::bbox(&space::outline(self.plane, self.node.local_bounds)).unwrap_or_else(|| {
            let origin = self.transform * kurbo::Point::ZERO;
            Rect::from_points(origin, origin)
        })
    }

    /// The part of [`Visit::bounds`] the `comp` nodes around it let it draw,
    /// or `None` when they clip all of it away.
    pub fn drawn(&self) -> Option<Rect> {
        let bounds = self.bounds();
        let Some(clip) = self.clip else {
            return Some(bounds);
        };
        let inside = bounds.intersect(clip);
        (inside.width() > 0.0 && inside.height() > 0.0).then_some(inside)
    }
}

/// The address of the node `ids` lead to from the scene's root, as
/// [`SceneGraph::paths`] lists them: `["card", "title"]` is `card/title`.
///
/// # Panics
/// If an id does not follow the id rule, which validation rules out.
pub fn address_of(ids: &[&str]) -> Address {
    Address::layer(ids.iter().map(|id| {
        id.parse::<Id>()
            .expect("validated layer ids follow the id rule")
    }))
}

/// Every node of `state`, a frame of `graph`, in paint order: each node,
/// then the nodes inside it when it is a shown `comp` node.
pub fn walk<'a>(graph: &'a SceneGraph, state: &'a FrameState) -> Walk<'a> {
    Walk {
        graph,
        sizes: subtree_sizes(graph),
        next_index: 0,
        stack: vec![Level {
            comp: &graph.root,
            nodes: &state.nodes,
            next: 0,
            ids: Vec::new(),
            transform: Affine::IDENTITY,
            plane: DMat3::IDENTITY,
            projected: false,
            visible: true,
            opacity: 1.0,
            clip: None,
        }],
    }
}

/// The iterator [`walk`] returns.
#[derive(Debug, Clone)]
pub struct Walk<'a> {
    graph: &'a SceneGraph,
    /// How many nodes each composition holds at any depth, to skip those of
    /// one that is not evaluated.
    sizes: Vec<usize>,
    /// The [`Visit::index`] of the next node.
    next_index: usize,
    /// The canvases being walked, the innermost last.
    stack: Vec<Level<'a>>,
}

/// One canvas on the way down: its nodes, the next to visit, and what the
/// `comp` nodes around it add to each of them.
#[derive(Debug, Clone)]
struct Level<'a> {
    /// The composition whose canvas this is, which says each node's matte.
    comp: &'a CompGraph,
    nodes: &'a [NodeState],
    next: usize,
    ids: Vec<&'a str>,
    transform: Affine,
    plane: DMat3,
    projected: bool,
    visible: bool,
    opacity: f64,
    clip: Option<Rect>,
}

impl<'a> Iterator for Walk<'a> {
    type Item = Visit<'a>;

    fn next(&mut self) -> Option<Visit<'a>> {
        loop {
            let level = self.stack.last_mut()?;
            let Some(node) = level.nodes.get(level.next) else {
                self.stack.pop();
                continue;
            };
            let at = level.next;
            level.next += 1;
            let mut ids = level.ids.clone();
            ids.push(node.id.as_str());
            let index = self.next_index;
            self.next_index += 1;
            let visit = Visit {
                index,
                ids,
                node,
                transform: level.transform * node.transform,
                plane: level.plane
                    * node
                        .plane
                        .unwrap_or_else(|| space::projective(node.transform)),
                projected: level.projected || node.plane.is_some(),
                visible: level.visible && node.visible(),
                opacity: level.opacity * node.number(Prop::OPACITY),
                clip: level.matted(at, MATTE_DEPTH).map_or(level.clip, |matte| {
                    Some(level.clip.map_or(matte, |clip| clip.intersect(matte)))
                }),
            };
            if let ContentState::Comp(comp, None) = &node.content {
                // Not evaluated at this frame: nothing inside it is met.
                self.next_index += self.sizes[*comp];
            }
            if let ContentState::Comp(index, Some(inner)) = &node.content {
                let comp = &self.graph.comps[*index];
                let whole = Rect::new(0.0, 0.0, comp.width, comp.height);
                let canvas = match visit.projected {
                    false => box_through(visit.transform, whole),
                    true => space::bbox(&space::outline(visit.plane, whole)).unwrap_or(Rect::ZERO),
                };
                self.stack.push(Level {
                    comp,
                    nodes: &inner.nodes,
                    next: 0,
                    ids: visit.ids.clone(),
                    transform: visit.transform,
                    plane: visit.plane,
                    projected: visit.projected,
                    visible: visit.visible,
                    opacity: visit.opacity,
                    clip: Some(visit.clip.map_or(canvas, |outer| outer.intersect(canvas))),
                });
            }
            return Some(visit);
        }
    }
}

/// How many mattes in a row [`Level::matted`] follows: each may be matted in
/// turn, and validation keeps a chain from looping, so this only bounds the
/// work.
const MATTE_DEPTH: usize = 16;

impl Level<'_> {
    /// The box, on the scene's canvas, that node `i`'s alpha or luma matte
    /// lets it draw in, or `None` when it has none. An inverted matte shows
    /// the node where the matte layer does not draw, so it does not limit
    /// the box. A matte layer outside its span draws nothing, and so lets
    /// nothing show.
    fn matted(&self, i: usize, depth: usize) -> Option<Rect> {
        let (source, mode) = self.comp.nodes.get(i)?.matte?;
        if matches!(mode, MatteMode::AlphaInverted | MatteMode::LumaInverted) || depth == 0 {
            return None;
        }
        let matte = self.nodes.get(source)?;
        if !matte.shown {
            return Some(Rect::ZERO);
        }
        let seen = Visit {
            index: 0,
            ids: Vec::new(),
            node: matte,
            transform: self.transform * matte.transform,
            plane: self.plane
                * matte
                    .plane
                    .unwrap_or_else(|| space::projective(matte.transform)),
            projected: self.projected || matte.plane.is_some(),
            visible: true,
            opacity: 1.0,
            clip: None,
        };
        let drawn = seen.bounds();
        Some(
            self.matted(source, depth - 1)
                .map_or(drawn, |outer| drawn.intersect(outer)),
        )
    }
}

/// How many nodes each composition of `graph` holds at any depth.
fn subtree_sizes(graph: &SceneGraph) -> Vec<usize> {
    fn size(graph: &SceneGraph, comp: usize, sizes: &mut [Option<usize>]) -> usize {
        if let Some(known) = sizes[comp] {
            return known;
        }
        let total = graph.comps[comp]
            .nodes
            .iter()
            .map(|node| match node.content {
                NodeContent::Comp(inner) => 1 + size(graph, inner, sizes),
                _ => 1,
            })
            .sum();
        sizes[comp] = Some(total);
        total
    }
    let mut sizes = vec![None; graph.comps.len()];
    (0..graph.comps.len())
        .map(|comp| size(graph, comp, &mut sizes))
        .collect()
}

impl SceneGraph {
    /// The ids of every node at any depth, from the scene's root down, in
    /// the order [`walk`] visits them: each node, then the nodes of the
    /// composition it draws. A composition shown twice is listed twice.
    pub fn paths(&self) -> Vec<Vec<&str>> {
        let mut paths = Vec::new();
        self.collect(&self.root.nodes, &[], &mut paths);
        paths
    }

    /// The node `ids` lead to from the root, and the composition graph it
    /// sits in: the root's for one id.
    pub fn node_at(&self, ids: &[&str]) -> Option<(&crate::Node, &CompGraph)> {
        let (last, outer) = ids.split_last()?;
        let mut comp = &self.root;
        for id in outer {
            let node = comp.nodes.iter().find(|n| n.id == *id)?;
            let NodeContent::Comp(index) = node.content else {
                return None;
            };
            comp = &self.comps[index];
        }
        let node = comp.nodes.iter().find(|n| n.id == *last)?;
        Some((node, comp))
    }

    /// The node of the layer `address` names: at any depth as drawn
    /// ([`SceneGraph::node_at`]), or as written in a composition,
    /// `scene.comps.card.layers[title]`, in the graph every layer without
    /// overrides shares.
    pub fn find(&self, address: &Address) -> Option<&crate::Node> {
        if let Some((comp, id)) = address.written_in_comp() {
            let comp = self.comps.iter().find(|c| c.id == comp)?;
            return comp.nodes.iter().find(|n| n.id == id);
        }
        if address.layers.is_empty() || !address.keys.is_empty() {
            return None;
        }
        let ids: Vec<&str> = address.layers.iter().map(AsRef::as_ref).collect();
        self.node_at(&ids).map(|(node, _)| node)
    }

    fn collect<'a>(
        &'a self,
        nodes: &'a [crate::Node],
        outer: &[&'a str],
        paths: &mut Vec<Vec<&'a str>>,
    ) {
        for node in nodes {
            let mut ids = outer.to_vec();
            ids.push(node.id.as_str());
            paths.push(ids.clone());
            if let NodeContent::Comp(index) = node.content {
                self.collect(&self.comps[index].nodes, &ids, paths);
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/walk.rs"]
mod tests;
