//! `overlap`: two readable text layers whose glyphs run into each other, as
//! when one line's descenders touch the next line's capitals.
//!
//! Each glyph is measured by the box its outline covers on the canvas, after
//! its animators, so lines set close but clear of each other pass.

use super::{Finding, px};
use kurbo::{Affine, Rect};
use nakkash_scene::{ContentState, SceneGraph, Visit, space};

/// Ink that overlaps by this many px or less (on a 1080-px-tall canvas), on
/// either axis, only touches.
const TOUCHING: f64 = 1.0;

/// A glyph at least this opaque is read, as a layer is.
const READ: f64 = 0.9;

/// The boxes, on the scene's canvas, that the glyphs of the text node in
/// `visit` cover where they are drawn; `inks` are their boxes as laid out,
/// one per glyph in run order, as [`layout_inks`] gives them. Glyphs that
/// draw nothing, or that animators leave faint, are left out.
pub(crate) fn canvas_inks(visit: &Visit<'_>, inks: &[Option<Rect>]) -> Vec<Rect> {
    let ContentState::Text { glyphs, .. } = &visit.node.content else {
        return Vec::new();
    };
    inks.iter()
        .enumerate()
        .filter_map(|(i, ink)| {
            let ink = (*ink)?;
            let glyph = glyphs.get(i);
            if glyph.is_some_and(|g| g.opacity < READ) {
                return None;
            }
            let moved = glyph.map_or(Affine::IDENTITY, |g| g.transform);
            match visit.projected {
                false => Some((visit.transform * moved).transform_rect_bbox(ink)),
                true => space::bbox(&space::outline(visit.plane * space::projective(moved), ink)),
            }
        })
        .collect()
}

/// The boxes the glyphs of the text node in `visit` cover as laid out, in
/// its own coordinates: one per glyph in run order, `None` for one that
/// draws nothing; empty for a node that is not text.
pub(crate) fn layout_inks(visit: &Visit<'_>) -> Vec<Option<Rect>> {
    let ContentState::Text { text, .. } = &visit.node.content else {
        return Vec::new();
    };
    text.layout.runs.iter().flat_map(|run| run.inks()).collect()
}

/// How deep the ink of `a` and `b` runs into each other, the deepest pair
/// of glyphs: the smaller side of their overlap, when it is more than
/// touching.
pub(crate) fn judge(a: &[Rect], b: &[Rect], graph: &SceneGraph) -> Option<Finding> {
    let (Some(ra), Some(rb)) = (reach(a), reach(b)) else {
        return None;
    };
    if !ra.overlaps(rb) {
        return None;
    }
    let depth = deepest(a, b);
    (depth > TOUCHING * px(graph)).then_some(Finding {
        value: depth,
        limit: 0.0,
    })
}

/// How deep any glyph of `a` runs into any of `b`: the largest smaller side
/// of their overlaps, 0 when none overlap.
///
/// When every box is finite and `b` is sorted by its left edges
/// ([`sort_inks`]), the glyphs of `b` that start at or past a glyph of `a`'s
/// right edge are passed over: their overlap with it is exactly 0 across,
/// which cannot raise the largest, so the depth is the same.
fn deepest(a: &[Rect], b: &[Rect]) -> f64 {
    let overlap = |x: &Rect, y: &Rect| {
        let both = x.intersect(*y);
        both.width().min(both.height())
    };
    let finite = |r: &[Rect]| {
        r.iter()
            .all(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()))
    };
    let sorted = b.is_sorted_by(|p, q| p.x0 <= q.x0);
    if !(sorted && finite(a) && finite(b)) {
        return a
            .iter()
            .flat_map(|x| b.iter().map(move |y| overlap(x, y)))
            .fold(0.0, f64::max);
    }
    a.iter()
        .flat_map(|x| {
            b.iter()
                .take_while(move |y| y.x0 < x.x1)
                .map(move |y| overlap(x, y))
        })
        .fold(0.0, f64::max)
}

/// `inks` sorted by their left edges, as [`judge`] reads them fastest; the
/// order does not change what it finds.
pub(crate) fn sort_inks(inks: &mut [Rect]) {
    inks.sort_by(|p, q| p.x0.total_cmp(&q.x0));
}

/// The box all of `inks` cover, or `None` for no ink.
pub(crate) fn reach(inks: &[Rect]) -> Option<Rect> {
    inks.iter().copied().reduce(|x, y| x.union(y))
}

#[cfg(test)]
#[path = "../../tests/unit/rules/overlap.rs"]
mod tests;
