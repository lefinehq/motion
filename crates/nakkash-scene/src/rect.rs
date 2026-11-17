//! The rectangle a layer's content covers in its own coordinates, at one
//! time: what an imported expression's `sourceRectAtTime` reads
//! ([`NodeState::content_rect`], [`content_rects`]).

use std::collections::HashMap;

use kurbo::{Rect, Shape};
use nakkash_format::Address;
use nakkash_format::item::props;
use nakkash_text::GlyphRun;

use crate::{ContentState, FrameState, NodeState, SceneGraph, ShapeDraw, evaluate, evaluate_comp};

impl NodeState {
    /// The rectangle the node's content covers in its own coordinates.
    ///
    /// - **Text** covers the outlines of its glyphs as they are set: not
    ///   the advance of a trailing space, nor the room the font keeps above
    ///   and below its letters. Its animators change nothing: a glyph an
    ///   animator moves or hides covers where it is set. Box text covers its
    ///   words, not its box; with `extents`, it covers its box.
    /// - **Shapes** cover their outlines, where their groups put them; with
    ///   `extents`, a stroke widens its outline by half its width.
    /// - **A rectangle, picture, clip or composition** covers its box, from
    ///   the origin.
    ///
    /// Content that covers nothing, such as empty text, covers an empty
    /// rectangle at the origin.
    pub fn content_rect(&self, extents: bool) -> Rect {
        let covered = match &self.content {
            // Animators move and fade glyphs where they are drawn, not
            // where they are set.
            ContentState::Text { text, .. } => match (extents, text.content.frame) {
                (true, Some(frame)) => Some(frame),
                _ => text
                    .layout
                    .runs
                    .iter()
                    .flat_map(GlyphRun::inks)
                    .flatten()
                    .reduce(|a, b| a.union(b)),
            },
            ContentState::Shapes(draws) => draws
                .iter()
                .map(|draw| shape_rect(draw, extents))
                .reduce(|a, b| a.union(b)),
            ContentState::Rect(_)
            | ContentState::Comp(..)
            | ContentState::Image(_)
            | ContentState::Video { .. } => Some(self.own_box),
            ContentState::Audio
            | ContentState::Pivot
            | ContentState::Camera
            | ContentState::Filter => None,
        };
        covered.unwrap_or(Rect::ZERO)
    }
}

/// The rectangle one shape draw covers in the layer's coordinates: its
/// outline where its groups put it, widened by half a stroke's width with
/// `extents`.
fn shape_rect(draw: &ShapeDraw, extents: bool) -> Rect {
    let outline = (draw.transform * draw.outline.clone()).bounding_box();
    let stroke = std::ptr::eq(draw.paint.kind.registry, props::STROKE_WIDTH.registry());
    match extents && stroke {
        true => {
            // The width is in the paint's group space; its groups scale it.
            let scale = draw.transform.as_coeffs();
            let reach = draw.paint.number(props::STROKE_WIDTH) / 2.0
                * (scale[0].hypot(scale[1])).max(scale[2].hypot(scale[3]));
            outline.inflate(reach, reach)
        }
        false => outline,
    }
}

/// The rectangles the contents of layers cover, each at `seconds` of its
/// composition's clock, in the layer's own coordinates
/// ([`NodeState::content_rect`]): one answer per ask, `(address, seconds, extents)`. An
/// address is one a scene writes a layer at: `title` for one of the scene's
/// own, `scene.comps.card.layers[title]` for a composition's; `None` when no
/// layer is there. Each composition is evaluated once per time asked.
pub fn content_rects(graph: &SceneGraph, asks: &[(Address, f64, bool)]) -> Vec<Option<Rect>> {
    let mut frames: HashMap<(Option<usize>, u64), FrameState> = HashMap::new();
    asks.iter()
        .map(|(address, seconds, extents)| {
            let (comp, index) = locate(graph, address)?;
            let clock = comp.map_or(&graph.root, |i| &graph.comps[i]);
            let state = frames.entry((comp, seconds.to_bits())).or_insert_with(|| {
                let frame = seconds * clock.fps;
                match comp {
                    Some(i) => evaluate_comp(graph, i, frame),
                    None => evaluate(graph, frame),
                }
            });
            Some(state.nodes[index].content_rect(*extents))
        })
        .collect()
}

/// The composition (`None` for the scene's own) and node index of the
/// layer a scene writes at `address`.
fn locate(graph: &SceneGraph, address: &Address) -> Option<(Option<usize>, usize)> {
    let (comp, id) = match address.written_in_comp() {
        Some((comp, id)) => (Some(graph.comps.iter().position(|c| c.id == comp)?), id),
        None => match address.layers.as_slice() {
            [id] if address.keys.is_empty() => (None, id.as_ref()),
            _ => return None,
        },
    };
    let nodes = &comp.map_or(&graph.root, |i| &graph.comps[i]).nodes;
    Some((comp, nodes.iter().position(|n| n.id == id)?))
}

#[cfg(test)]
#[path = "../tests/unit/rect.rs"]
mod tests;
