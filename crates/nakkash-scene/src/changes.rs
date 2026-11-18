//! What an edit can change in what a walk meets ([D79]): the nodes, at any
//! depth, whose visits may differ between two compiled versions of a scene.
//!
//! A node's visit at a frame is read from the states of the nodes on its
//! path (its own and those of the `comp` nodes around it) and from what
//! each of those states is read from in its composition: the node itself,
//! the nodes it moves with, its matte and what that moves with, the camera
//! of a composition, and whether another node reads it as a matte or an
//! effect input, which keeps a hidden composition evaluated. A node none of
//! whose sources changed visits the same at every frame.
//!
//! [D79]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d79-an-edit-checks-only-what-it-can-change

use std::collections::HashMap;

use crate::{CompGraph, Node, NodeContent, SceneGraph};

/// Which of the nodes [`SceneGraph::paths`] lists may be met differently
/// by a [`walk`](crate::walk) of `after` than of `before`, at some frame:
/// one flag per path, in that order. `None` when the two differ in shape,
/// so that the paths themselves differ, or in the root's canvas or clock,
/// which every node is measured against: then every node may differ.
///
/// The two are compared instance by instance: a `comp` layer that draws a
/// copy of its composition in one version, as an override makes, is
/// compared with what it draws in the other.
pub fn changed_paths(before: &SceneGraph, after: &SceneGraph) -> Option<Vec<bool>> {
    if !same_canvas(&before.root, &after.root) {
        return None;
    }
    let mut pair = Pair {
        before,
        after,
        compared: HashMap::new(),
        paths: Vec::new(),
    };
    pair.collect(None, None, false)?;
    Some(pair.paths)
}

/// The two versions being compared, and what is found so far.
struct Pair<'a> {
    before: &'a SceneGraph,
    after: &'a SceneGraph,
    /// The flags of each pair of compositions compared, by their indices
    /// (`None` for the root), so one drawn many times is compared once.
    compared: HashMap<(Option<usize>, Option<usize>), Vec<bool>>,
    /// A flag for each path met so far.
    paths: Vec<bool>,
}

impl Pair<'_> {
    /// The composition at `index` of `graph`, or its root.
    fn comp(graph: &SceneGraph, index: Option<usize>) -> &CompGraph {
        index.map_or(&graph.root, |i| &graph.comps[i])
    }

    /// Appends a flag for each node of the compositions `b` of `before` and
    /// `a` of `after`, then for the nodes of the compositions each draws, in
    /// the order of [`SceneGraph::paths`]: set when the node, or a `comp`
    /// node around it (`around`), may be met differently. `None` when they
    /// differ in shape.
    fn collect(&mut self, b: Option<usize>, a: Option<usize>, around: bool) -> Option<()> {
        let (before, after) = (Self::comp(self.before, b), Self::comp(self.after, a));
        if before.nodes.len() != after.nodes.len() {
            return None;
        }
        let changed = self
            .compared
            .entry((b, a))
            .or_insert_with(|| changed_nodes(before, after))
            .clone();
        for ((old, new), own) in before.nodes.iter().zip(&after.nodes).zip(changed) {
            if old.id != new.id {
                return None;
            }
            let flag = around || own;
            self.paths.push(flag);
            match (&old.content, &new.content) {
                (NodeContent::Comp(x), NodeContent::Comp(y)) => {
                    self.collect(Some(*x), Some(*y), flag)?;
                }
                (NodeContent::Comp(_), _) | (_, NodeContent::Comp(_)) => return None,
                _ => {}
            }
        }
        Some(())
    }
}

/// Whether two compositions have the same canvas and clock: everything of
/// theirs but their nodes and their id, which a walk does not read (an
/// override's copy of a composition has an id of its own).
fn same_canvas(before: &CompGraph, after: &CompGraph) -> bool {
    before.width == after.width
        && before.height == after.height
        && before.fps == after.fps
        && before.duration_frames == after.duration_frames
        && before.background == after.background
        && before.shutter == after.shutter
}

/// Which nodes of a composition, listed alike in both versions, may be
/// evaluated or walked differently: those that changed, and those that
/// read a node that did.
fn changed_nodes(before: &CompGraph, after: &CompGraph) -> Vec<bool> {
    let count = after.nodes.len();
    if !same_canvas(before, after) {
        return vec![true; count];
    }
    let mut changed: Vec<bool> = before
        .nodes
        .iter()
        .zip(&after.nodes)
        .map(|(b, a)| !same_node(b, a))
        .collect();
    // Whether another node reads a node as its matte or an effect's input
    // keeps it evaluated while hidden: a change of what reads which changes
    // both what was read and what is.
    for (b, a) in before.nodes.iter().zip(&after.nodes) {
        if b.matte != a.matte || b.inputs != a.inputs {
            for source in reads(b).chain(reads(a)) {
                if let Some(flag) = changed.get_mut(source) {
                    *flag = true;
                }
            }
        }
    }
    // A node moves with its parents and is clipped by its matte, and so by
    // what those move with and are clipped by: spread until nothing more
    // changes. Each round marks a node or ends, so it ends.
    loop {
        let mut spread = false;
        for (i, node) in after.nodes.iter().enumerate() {
            if changed[i] {
                continue;
            }
            let sources = node.parent.into_iter().chain(node.matte.map(|(m, _)| m));
            let old = &before.nodes[i];
            let old_sources = old.parent.into_iter().chain(old.matte.map(|(m, _)| m));
            if sources
                .chain(old_sources)
                .any(|s| changed.get(s).copied().unwrap_or(true))
            {
                changed[i] = true;
                spread = true;
            }
        }
        if !spread {
            break;
        }
    }
    // The camera a composition is seen through places every node in space,
    // and a camera changes with what it moves with, so this comes last.
    let cameras = |c: &CompGraph| {
        c.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.content, NodeContent::Camera { .. }))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    };
    if cameras(before)
        .into_iter()
        .chain(cameras(after))
        .any(|i| changed[i])
    {
        return vec![true; count];
    }
    changed
}

/// Whether two nodes are the same, as a walk meets them: two `comp` nodes
/// may draw different copies of one composition, which are compared on
/// their own, so their indices are not compared.
fn same_node(before: &Node, after: &Node) -> bool {
    match (&before.content, &after.content) {
        (NodeContent::Comp(x), NodeContent::Comp(y)) if x != y => {
            let mut moved = after.clone();
            moved.content = NodeContent::Comp(*x);
            *before == moved
        }
        _ => before == after,
    }
}

/// The nodes `node` reads: its matte and its effects' inputs.
fn reads(node: &Node) -> impl Iterator<Item = usize> + '_ {
    node.matte
        .map(|(m, _)| m)
        .into_iter()
        .chain(node.inputs.iter().flatten().copied())
}

#[cfg(test)]
#[path = "../tests/unit/changes.rs"]
mod tests;
