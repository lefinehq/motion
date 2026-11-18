//! `contrast`: a text layer's glyphs against what is drawn behind them, as
//! WCAG 2's contrast ratio, at least 4.5:1, or 3:1 for text of 24 px and up
//! on a 1080-px-tall canvas.
//!
//! The scene is drawn twice for each readable text layer at a sample of
//! frames: once without the layer, for what is behind it (the `bg` of the
//! scene and of each composition it sits in, and the layers below it), and
//! once with only the layer, inside the `comp` layers it sits in but with no
//! `bg` drawn, for its glyphs.

use std::sync::{Mutex, PoisonError};

use nakkash_format::Result;
use nakkash_query::{Code, Measure, Report, Severity, Unit, Violation};
use nakkash_raster::Image;
use nakkash_scene::{ContentState, FrameState, NodeState, SceneGraph, Visit, address_of, walk};

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
///
/// Each readable text at each sample is measured on its own thread. When
/// `drawn_whole`, every frame of the scene has been drawn whole without
/// failing (by `flash`), so drawing a frame with one layer left out cannot
/// fail either; then a text whose glyphs cover less than a px is passed
/// over without drawing what is behind it, which it would not be measured
/// against.
pub(super) fn check(session: &Session, drawn_whole: bool) -> Result<Report> {
    let graph = session.graph()?;
    let frames = session.frames()?.end;
    let stride = ((graph.root.fps / SAMPLES_A_SECOND).round() as u32).max(1);
    let bare = bare(graph);
    let samples: Vec<(u32, FrameState)> = (0..frames.div_ceil(stride))
        .map(|sample| {
            let frame = sample * stride;
            session.frame(f64::from(frame)).map(|state| (frame, state))
        })
        .collect::<Result<_>>()?;
    // Every readable text of every sample, in frame order, then paint order.
    let texts: Vec<Text> = samples
        .iter()
        .enumerate()
        .flat_map(|(at, (frame, state))| {
            walk(graph, state).filter_map(move |visit| {
                let size = readable(&visit)?;
                Some(Text {
                    sample: at,
                    frame: *frame,
                    index: visit.index,
                    ids: visit.ids.iter().map(|id| (*id).to_owned()).collect(),
                    size,
                    scale: visit.transform.determinant().abs().sqrt(),
                })
            })
        })
        .collect();
    let measured = Mutex::new(Vec::with_capacity(texts.len()));
    let count = u32::try_from(texts.len()).unwrap_or(u32::MAX);
    session.on_threads(0..count, |item| {
        let text = &texts[item as usize];
        let state = &samples[text.sample].1;
        let found = measure(session, graph, &bare, state, text, drawn_whole);
        measured
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((item, found));
        Ok(())
    })?;
    let mut measured = measured
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    measured.sort_by_key(|(item, _)| *item);
    let mut seen = Vec::new();
    for (item, found) in measured {
        // The first failure in frame order, then paint order, as one frame
        // after another would meet it.
        match found {
            Ok(found) => seen.extend(found),
            Err(mut d) => {
                d.message = format!("frame {}: {}", texts[item as usize].frame, d.message);
                return Err(d);
            }
        }
    }
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
                with: None,
            });
        }
    }
    Ok(report)
}

/// A text layer readable at one sample, to be measured there.
struct Text {
    /// Which sample it is read at.
    sample: usize,
    /// The frame of that sample.
    frame: u32,
    /// Its place in the graph's paths.
    index: usize,
    /// The ids leading to it.
    ids: Vec<String>,
    /// The smallest size it is set in, in its own px.
    size: f64,
    /// How much its transform scales it, on the canvas.
    scale: f64,
}

/// `graph` with no composition drawing a background, for the glyphs alone:
/// a `comp` layer draws its composition's `bg` across its canvas, which
/// would cover the glyphs' surroundings and read as glyphs.
fn bare(graph: &SceneGraph) -> SceneGraph {
    let mut bare = graph.clone();
    for comp in &mut bare.comps {
        comp.background = [0.0; 4];
    }
    bare
}

/// `text` measured at its sample, `state`: `bare` is the graph as [`bare`]
/// gives it. `None` when its glyphs cover less than a px. With
/// `drawn_whole`, as [`check`] says, what is behind such glyphs is not
/// drawn; else it is drawn first, so a failure to draw it is the one met.
fn measure(
    session: &Session,
    graph: &SceneGraph,
    bare: &SceneGraph,
    state: &FrameState,
    text: &Text,
    drawn_whole: bool,
) -> Result<Option<Seen>> {
    let ids: Vec<&str> = text.ids.iter().map(String::as_str).collect();
    let glyphs = session
        .list_in(bare, &alone(state, &ids))
        .and_then(|mut list| {
            list.background = None;
            shrunk(session, list)
        });
    if drawn_whole && glyphs.as_ref().is_ok_and(|g| coverage(g) < 1.0) {
        return Ok(None);
    }
    let behind = shrunk(session, session.list_of(&without(state, &ids))?)?;
    let Some(ratio) = ratio(&glyphs?, &behind) else {
        return Ok(None);
    };
    let large = text.size * text.scale * 1080.0 / graph.root.height;
    Ok(Some(Seen {
        index: text.index,
        frame: text.frame,
        ratio,
        limit: if large >= LARGE_PX { LARGE } else { NORMAL },
    }))
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
    let total = coverage(glyphs);
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

/// How many px the glyphs in `glyphs` cover, adding up their coverage in
/// px order.
fn coverage(glyphs: &Image) -> f64 {
    glyphs
        .pixels()
        .chunks_exact(4)
        .filter(|glyph| glyph[3] > 0)
        .map(|glyph| f64::from(glyph[3]) / 255.0)
        .sum()
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
