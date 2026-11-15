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

use kurbo::Affine;
use motion_format::Address;
use motion_format::address::Id;

use crate::{ContentState, FrameState, NodeState};

/// One node met on a [`walk`].
#[derive(Debug, Clone, PartialEq)]
pub struct Visit<'a> {
    /// The ids from the scene's root down to the node: the `comp` nodes it
    /// sits in, outermost first, then its own.
    pub ids: Vec<&'a str>,
    /// Its state at the frame, in its own composition's canvas.
    pub node: &'a NodeState,
    /// From the node's own coordinates to the scene's canvas: its transform,
    /// then that of each `comp` node around it.
    pub transform: Affine,
    /// Whether it draws in the frame: it, and every `comp` node around it,
    /// is [visible](NodeState::visible).
    pub visible: bool,
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
        Address::layer(self.ids.iter().map(|id| {
            id.parse::<Id>()
                .expect("validated layer ids follow the id rule")
        }))
    }
}

/// Every node of `state`, a frame of a whole scene, in paint order: each
/// node, then the nodes inside it when it is a shown `comp` node.
pub fn walk(state: &FrameState) -> Walk<'_> {
    Walk {
        stack: vec![Level {
            nodes: &state.nodes,
            next: 0,
            ids: Vec::new(),
            transform: Affine::IDENTITY,
            visible: true,
        }],
    }
}

/// The iterator [`walk`] returns.
#[derive(Debug, Clone)]
pub struct Walk<'a> {
    /// The canvases being walked, the innermost last.
    stack: Vec<Level<'a>>,
}

/// One canvas on the way down: its nodes, the next to visit, and what the
/// `comp` nodes around it add to each of them.
#[derive(Debug, Clone)]
struct Level<'a> {
    nodes: &'a [NodeState],
    next: usize,
    ids: Vec<&'a str>,
    transform: Affine,
    visible: bool,
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
            level.next += 1;
            let mut ids = level.ids.clone();
            ids.push(node.id.as_str());
            let visit = Visit {
                ids,
                node,
                transform: level.transform * node.transform,
                visible: level.visible && node.visible(),
            };
            if let ContentState::Comp(_, Some(inner)) = &node.content {
                self.stack.push(Level {
                    nodes: &inner.nodes,
                    next: 0,
                    ids: visit.ids.clone(),
                    transform: visit.transform,
                    visible: visit.visible,
                });
            }
            return Some(visit);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/walk.rs"]
mod tests;
