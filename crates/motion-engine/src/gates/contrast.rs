//! `contrast`: a text layer's glyphs against what is drawn behind them, as
//! WCAG 2's contrast ratio, at least 4.5:1, or 3:1 for text of 24 px and up
//! on a 1080-px-tall canvas.
//!
//! The scene is drawn twice for each readable text layer at a sample of
//! frames: once without the layer, for what is behind it, and once with
//! only the layer (and the `comp` layers it sits in), for its glyphs.

use std::sync::{Mutex, PoisonError};

use motion_format::Result;
use motion_query::{Code, Measure, Report, Severity, Unit, Violation};
use motion_raster::Image;
use motion_scene::{ContentState, FrameState, NodeState, SceneGraph, Visit, address_of, walk};

use super::{luminance, over_black, shrunk};
use crate::session::Session;

/// The least ratio for text below [`LARGE_PX`].
const NORMAL: f64 = 4.5;
/// The least ratio for text of [`LARGE_PX`] and up.
const LARGE: f64 = 3.0;
/// The size from which text is large, in px of a 1080-px-tall canvas.
const LARGE_PX: f64 = 24.0;
/// The share of a layer's glyph coverage that must reach the ratio: thin
/// edges and the odd glyph over a busy patch may fall short.
const MOST_OF: f64 = 0.9;
/// A text layer is readable from this opacity up, as for `text_hold`.
const READABLE: f64 = 0.9;
/// Samples a second.
const SAMPLES_A_SECOND: f64 = 2.0;

/// One text layer measured at one frame.
struct Seen {
    /// Its place in the graph's paths.
    index: usize,
    frame: u32,
    /// The ratio most of its glyphs reach.
    ratio: f64,
    /// The least ratio it needs.
    limit: f64,
}

/// The `contrast` report of the scene: one error line per run of failing
/// samples of a text layer, and a pass for each text layer measured that
/// never fails.
pub(super) fn check(session: &Session) -> Result<Report> {
    let graph = session.graph()?;
    let frames = session.frames()?.end;
    let stride = ((graph.root.fps / SAMPLES_A_SECOND).round() as u32).max(1);
    let seen = Mutex::new(Vec::new());
    session.on_threads(0..frames.div_ceil(stride), |sample| {
        let frame = sample * stride;
        let found = measure(session, graph, frame).map_err(|d| (frame, d))?;
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(found);
        Ok(())
    })?;
    let mut seen = seen.into_inner().unwrap_or_else(PoisonError::into_inner);
    seen.sort_by_key(|s| (s.index, s.frame));

    let paths = graph.paths();
    let mut report = Report {
        violations: Vec::new(),
        passed: 0,
    };
    for layer in seen.chunk_by(|a, b| a.index == b.index) {
        let failing: Vec<&Seen> = layer.iter().filter(|s| s.ratio < s.limit).collect();
        if failing.is_empty() {
            report.passed += 1;
        }
        for run in failing.chunk_by(|a, b| b.frame == a.frame + stride) {
            let worst = run.iter().min_by(|a, b| a.ratio.total_cmp(&b.ratio));
            let worst = worst.expect("a run has a sample");
            report.violations.push(Violation {
                severity: Severity::Error,
                code: Code::Contrast,
                address: address_of(&paths[worst.index]),
                first_frame: run[0].frame,
                last_frame: run[run.len() - 1].frame,
                // Down to a tenth, so a ratio just short never prints as
                // its limit.
                measure: Measure::Below {
                    value: (worst.ratio * 10.0).floor() / 10.0,
                    limit: worst.limit,
                    unit: Unit::Ratio,
                },
                fix: None,
            });
        }
    }
    Ok(report)
}

/// Every readable text layer of `frame`, measured.
fn measure(session: &Session, graph: &SceneGraph, frame: u32) -> Result<Vec<Seen>> {
    let state = session.frame(f64::from(frame))?;
    let height = graph.root.height;
    let mut found = Vec::new();
    for visit in walk(graph, &state) {
        let Some(size) = readable(&visit) else {
            continue;
        };
        let behind = shrunk(session, session.list_of(&without(&state, &visit.ids))?)?;
        let mut list = session.list_of(&alone(&state, &visit.ids))?;
        list.background = None;
        let glyphs = shrunk(session, list)?;
        let Some(ratio) = ratio(&glyphs, &behind) else {
            continue;
        };
        let large = size * visit.transform.determinant().abs().sqrt() * 1080.0 / height;
        found.push(Seen {
            index: visit.index,
            frame,
            ratio,
            limit: if large >= LARGE_PX { LARGE } else { NORMAL },
        });
    }
    Ok(found)
}

/// The smallest size a text layer is set in, in its own px, when it is
/// readable at the frame: drawn, and opaque enough.
fn readable(visit: &Visit<'_>) -> Option<f64> {
    let ContentState::Text { text, .. } = &visit.node.content else {
        return None;
    };
    if !visit.visible || visit.opacity < READABLE {
        return None;
    }
    let content = &text.content;
    let sizes = content.runs.iter().map(|run| run.style.font.size);
    Some(sizes.fold(content.base.font.size, f64::min))
}

/// The ratio [`MOST_OF`] of the glyphs' coverage in `glyphs` reaches
/// against `behind`, px by px; `None` when they cover less than a px.
fn ratio(glyphs: &Image, behind: &Image) -> Option<f64> {
    let mut seen: Vec<(f64, f64)> = glyphs
        .pixels()
        .chunks_exact(4)
        .zip(behind.pixels().chunks_exact(4))
        .filter(|(glyph, _)| glyph[3] > 0)
        .map(|(glyph, behind)| {
            let text = luminance([glyph[0], glyph[1], glyph[2]]);
            let back = luminance(over_black(behind));
            let ratio = (text.max(back) + 0.05) / (text.min(back) + 0.05);
            (ratio, f64::from(glyph[3]) / 255.0)
        })
        .collect();
    let total: f64 = seen.iter().map(|&(_, cover)| cover).sum();
    if total < 1.0 {
        return None;
    }
    seen.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut under = 0.0;
    seen.into_iter().find_map(|(ratio, cover)| {
        under += cover;
        (under >= (1.0 - MOST_OF) * total).then_some(ratio)
    })
}

/// `state` with the node at `ids` hidden.
fn without(state: &FrameState, ids: &[&str]) -> FrameState {
    let mut state = state.clone();
    if let Some(node) = node_mut(&mut state, ids) {
        node.hidden = true;
    }
    state
}

/// `state` with only the node at `ids` drawn, inside the `comp` nodes
/// around it.
fn alone(state: &FrameState, ids: &[&str]) -> FrameState {
    let mut state = state.clone();
    keep(&mut state, ids);
    state
}

/// Hides every node of `state` but the one `ids` leads through.
fn keep(state: &mut FrameState, ids: &[&str]) {
    let Some((id, inner)) = ids.split_first() else {
        return;
    };
    for node in &mut state.nodes {
        if node.id != *id {
            node.hidden = true;
        } else if let ContentState::Comp(_, Some(comp)) = &mut node.content {
            keep(comp, inner);
        }
    }
}

/// The node `ids` leads to from `state`, through the `comp` nodes.
fn node_mut<'a>(state: &'a mut FrameState, ids: &[&str]) -> Option<&'a mut NodeState> {
    let (id, inner) = ids.split_first()?;
    let node = state.nodes.iter_mut().find(|n| n.id == *id)?;
    if inner.is_empty() {
        return Some(node);
    }
    match &mut node.content {
        ContentState::Comp(_, Some(comp)) => node_mut(comp, inner),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/gates/contrast.rs"]
mod tests;
