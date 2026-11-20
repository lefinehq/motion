//! `contrast`: a text layer's glyphs against what is drawn behind them, as
//! WCAG 2's contrast ratio, at least 4.5:1, or 3:1 for text of 24 px and up
//! on a canvas whose short side is 1080 px.
//!
//! The scene is drawn twice for each readable text layer at a sample of
//! frames: once without the layer, for what is behind it (the `bg` of the
//! scene and of each composition it sits in, and the layers below it), and
//! once with only the layer, inside the `comp` layers it sits in but with no
//! `bg` drawn, for its glyphs.
//!
//! Copies of one text, the same words drawn over each other where a
//! template stacks them for a split, an echo or a glow, are read as one
//! text: each copy's glyphs are the others' too, so against the others
//! every copy would read 1:1, and no change to the film around them could
//! read worse. The copies are drawn together, and measured against what
//! is behind all of them.

use std::sync::{Mutex, PoisonError};

use nakkash_format::Result;
use nakkash_query::{Code, Measure, Report, Severity, Unit, Violation};
use nakkash_raster::Image;
use nakkash_raster::kurbo::Rect;
use nakkash_scene::{ContentState, FrameState, NodeState, SceneGraph, Visit, address_of, walk};

use super::{luminance, over_black, shrunk};
use crate::session::Session;

/// The least ratio for text below [`LARGE_PX`].
const NORMAL: f64 = 4.5;
/// The least ratio for text of [`LARGE_PX`] and up.
const LARGE: f64 = 3.0;
/// The size from which text is large, in px of a canvas whose short side
/// is 1080 px: a screen shows a portrait canvas's width as it shows a
/// landscape one's height.
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
                let ContentState::Text { text, .. } = &visit.node.content else {
                    return None;
                };
                Some(Text {
                    sample: at,
                    frame: *frame,
                    index: visit.index,
                    ids: visit.ids.iter().map(|id| (*id).to_owned()).collect(),
                    size,
                    scale: visit.transform.determinant().abs().sqrt(),
                    words: text.content.text.clone(),
                    area: visit.transform.transform_rect_bbox(text.layout.bounds()),
                })
            })
        })
        .collect();
    let groups = copies(&texts);
    let measured = Mutex::new(Vec::with_capacity(groups.len()));
    let count = u32::try_from(groups.len()).unwrap_or(u32::MAX);
    session.on_threads(0..count, |item| {
        let members: Vec<&Text> = groups[item as usize].iter().map(|&i| &texts[i]).collect();
        let state = &samples[members[0].sample].1;
        let found = measure(session, graph, &bare, state, &members, drawn_whole);
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
                let first = groups[item as usize][0];
                d.message = format!("frame {}: {}", texts[first].frame, d.message);
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
    /// Its words.
    words: String,
    /// Its layout's box on the canvas.
    area: Rect,
}

/// The share of the smaller of two boxes the two must share for two texts
/// of the same words to be copies of one.
const SAME_PLACE: f64 = 0.6;

/// `texts` gathered into the copies of one text, in the order they come:
/// at one sample, the same words over most of the same box as the first of
/// the gathering, or the same text layer drawn by instances of the same
/// compositions (paths as long, to the same layer), wherever a split or a
/// glitch has moved each. Each gathering is measured once.
fn copies(texts: &[Text]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let found = groups.iter_mut().find(|group| {
            let first = &texts[group[0]];
            first.sample == text.sample
                && first.words == text.words
                && (overlap(first.area, text.area) >= SAME_PLACE
                    || instances(&first.ids, &text.ids))
        });
        match found {
            Some(group) => group.push(i),
            None => groups.push(vec![i]),
        }
    }
    groups
}

/// Whether `a` and `b` lead to one text layer through instances of the
/// same compositions: as long and the same last id, however many of the
/// instances on the way differ, as a split built of instances of instances
/// draws one title.
fn instances(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.len() > 1 && a.last() == b.last() && a != b
}

/// The share of the smaller of boxes `a` and `b` the two share.
fn overlap(a: Rect, b: Rect) -> f64 {
    let shared = a.intersect(b);
    let smaller = a.area().min(b.area());
    match smaller > 0.0 && shared.width() > 0.0 && shared.height() > 0.0 {
        true => shared.area() / smaller,
        false => 0.0,
    }
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

/// `members`, the copies of one text, measured together at their sample,
/// `state`: one measure for each, the same ratio against its own limit.
/// `bare` is the graph as [`bare`] gives it. None when their glyphs cover
/// less than a px. With
/// `drawn_whole`, as [`check`] says, what is behind such glyphs is not
/// drawn; else it is drawn first, so a failure to draw it is the one met.
fn measure(
    session: &Session,
    graph: &SceneGraph,
    bare: &SceneGraph,
    state: &FrameState,
    members: &[&Text],
    drawn_whole: bool,
) -> Result<Vec<Seen>> {
    let all: Vec<Vec<&str>> = members
        .iter()
        .map(|text| text.ids.iter().map(String::as_str).collect())
        .collect();
    let only = alone(state, &all);
    let draw = |state: &FrameState| {
        session.list_in(bare, state).and_then(|mut list| {
            list.background = None;
            shrunk(session, list)
        })
    };
    // The glyphs' shape, where their drawing reaches: an effect that wipes
    // or mattes the text away leaves no glyph there.
    let glyphs =
        draw(&shape(&only)).and_then(|shape| draw(&only).and_then(|drawn| reached(&shape, &drawn)));
    if drawn_whole && glyphs.as_ref().is_ok_and(|g| coverage(g) < 1.0) {
        return Ok(Vec::new());
    }
    let behind = shrunk(session, session.list_of(&without(state, &all))?)?;
    let whole = shrunk(session, session.list_of(state)?)?;
    let over = draw(&above(state, &all))?;
    let glyphs = glyphs?;
    if let Ok(dir) = std::env::var("NK_DUMP")
        && members.len() > 20
    {
        let f = members[0].frame;
        for (n, im) in [
            ("glyphs", &glyphs),
            ("behind", &behind),
            ("whole", &whole),
            ("over", &over),
        ] {
            let _ = std::fs::write(
                format!("{dir}/{f:03}-{n}.png"),
                im.to_png().unwrap_or_default(),
            );
        }
        let _ = std::fs::write(
            format!("{dir}/{f:03}-members.txt"),
            members
                .iter()
                .map(|m| m.ids.join("/"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let Some(ratio) = ratio(&glyphs, &behind, &whole, &over) else {
        return Ok(Vec::new());
    };
    Ok(members
        .iter()
        .map(|text| {
            let short = graph.root.width.min(graph.root.height);
            let large = text.size * text.scale * 1080.0 / short;
            Seen {
                index: text.index,
                frame: text.frame,
                ratio,
                limit: if large >= LARGE_PX { LARGE } else { NORMAL },
            }
        })
        .collect())
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

/// The ratio [`MOST_OF`] of the glyphs' coverage reaches, px by px, read
/// in the frame as drawn: `glyphs` is the shape of the text alone, with no
/// effect of its own or of its compositions, where its drawing reaches, `whole` the frame and
/// `behind` the frame without the text. `None` when the glyphs cover less
/// than a px.
///
/// The text's colour is the frame's where its glyphs are, with all that
/// is drawn over it, its effects and its compositions' and the way they
/// are blended, so a title a template draws through fills, tints and glows
/// reads as it shows. At a glyph's edge the frame is the glyph laid over
/// what is behind at the glyph's cover, and that cover is taken off again,
/// so the edge reads as the core does; where that gives the glyph's own
/// colour, within the frame's rounding, the text is drawn as set and its
/// colour is read exactly. Px where something drawn over the text covers
/// it are left out.
fn ratio(glyphs: &Image, behind: &Image, whole: &Image, over: &Image) -> Option<f64> {
    let mut seen: Vec<(f64, f64)> = Vec::new();
    let mut total = 0.0;
    let all = glyphs
        .pixels()
        .chunks_exact(4)
        .zip(behind.pixels().chunks_exact(4))
        .zip(whole.pixels().chunks_exact(4))
        .zip(over.pixels().chunks_exact(4));
    for (((glyph, behind), whole), over) in all {
        let a = f64::from(glyph[3]) / 255.0;
        if a <= 0.0 {
            continue;
        }
        let back = over_black(behind);
        let drawn = over_black(whole);
        // Where what is drawn over the text is opaque and the frame is
        // what is behind, within its rounding, though the glyph's own
        // colour would stand out there, the text is covered: those px are
        // not shown, and a text covered whole is not read at all.
        let unchanged = (0..3).all(|c| drawn[c].abs_diff(back[c]) <= 1);
        let own = [glyph[0], glyph[1], glyph[2]];
        if over[3] >= OPAQUE && unchanged && contrast(own, back) >= COVERED {
            continue;
        }
        let unmixed: [f64; 3] =
            std::array::from_fn(|c| (f64::from(drawn[c]) - (1.0 - a) * f64::from(back[c])) / a);
        // Within the frame's rounding of the glyph's own colour, the text
        // is drawn as set: read that colour, exactly.
        let slack = 0.5 / a + 1.0;
        let as_set = (0..3).all(|c| (unmixed[c] - f64::from(glyph[c])).abs() <= slack);
        let text: [u8; 3] = match as_set {
            true => [glyph[0], glyph[1], glyph[2]],
            false => unmixed.map(|v| v.round().clamp(0.0, 255.0) as u8),
        };
        let ratio = contrast(text, back);
        seen.push((ratio, a));
        total += a;
    }
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

/// The WCAG contrast of two colours.
fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// How far a glyph's own colour must stand out from what is behind it for
/// a frame that does not change there to mean the glyph is covered rather
/// than drawn in the colour behind it.
const COVERED: f64 = 1.5;

/// How opaque what is drawn over a glyph must be to cover it.
const OPAQUE: u8 = 242;

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

/// `state` with the nodes at each of `all` hidden.
fn without(state: &FrameState, all: &[Vec<&str>]) -> FrameState {
    let mut state = state.clone();
    for ids in all {
        if let Some(node) = node_mut(&mut state, ids) {
            node.hidden = true;
        }
    }
    state
}

/// `state` with only the nodes at each of `all` drawn, inside the `comp`
/// nodes around them.
fn alone(state: &FrameState, all: &[Vec<&str>]) -> FrameState {
    let mut state = state.clone();
    let all: Vec<&[&str]> = all.iter().map(Vec::as_slice).collect();
    keep(&mut state, &all);
    state
}

/// `state` with only what is drawn over the nodes at each of `all`: at
/// each level of the compositions they lie in, the nodes painted after
/// the last of them. Track mattes and adjustments drawn there count, as
/// the renderer draws them.
fn above(state: &FrameState, all: &[Vec<&str>]) -> FrameState {
    fn mark(state: &mut FrameState, all: &[&[&str]]) {
        let on = |node: &NodeState| all.iter().any(|ids| ids.first() == Some(&node.id.as_str()));
        let Some(last) = state.nodes.iter().rposition(on) else {
            return;
        };
        for (i, node) in state.nodes.iter_mut().enumerate() {
            if i > last {
                continue;
            }
            let inner: Vec<&[&str]> = all
                .iter()
                .filter_map(|ids| ids.split_first())
                .filter(|(id, _)| **id == node.id)
                .map(|(_, inner)| inner)
                .filter(|inner| !inner.is_empty())
                .collect();
            match (&mut node.content, inner.is_empty()) {
                (ContentState::Comp(_, Some(comp)), false) => mark(comp, &inner),
                _ => node.hidden = true,
            }
        }
    }
    let mut state = state.clone();
    let all: Vec<&[&str]> = all.iter().map(Vec::as_slice).collect();
    mark(&mut state, &all);
    state
}

/// `shape` where `drawn` reaches: each px's cover the less of the two.
fn reached(shape: &Image, drawn: &Image) -> Result<Image> {
    let px: Vec<u8> = shape
        .pixels()
        .chunks_exact(4)
        .zip(drawn.pixels().chunks_exact(4))
        .flat_map(|(s, d)| [s[0], s[1], s[2], s[3].min(d[3])])
        .collect();
    Image::new(shape.width(), shape.height(), px)
}

/// `state` with no node's effects but its corner pins: what is drawn is
/// the nodes' shapes, a text's glyphs where they fall in the frame, and a
/// pin is what lays a layer's plane in the frame.
fn shape(state: &FrameState) -> FrameState {
    fn strip(state: &mut FrameState) {
        for node in &mut state.nodes {
            let fx = std::mem::take(&mut node.fx);
            for pin in &mut node.pins {
                let item = fx[pin.fx].clone();
                pin.fx = node.fx.len();
                node.fx.push(item);
            }
            if let ContentState::Comp(_, Some(comp)) = &mut node.content {
                strip(comp);
            }
        }
    }
    let mut state = state.clone();
    strip(&mut state);
    state
}

/// Hides every node of `state` but those the paths in `all` lead through.
fn keep(state: &mut FrameState, all: &[&[&str]]) {
    for node in &mut state.nodes {
        let inner: Vec<&[&str]> = all
            .iter()
            .filter_map(|ids| ids.split_first())
            .filter(|(id, _)| **id == node.id)
            .map(|(_, inner)| inner)
            .collect();
        if inner.is_empty() {
            node.hidden = true;
        } else if let ContentState::Comp(_, Some(comp)) = &mut node.content {
            // A path that ends here keeps the whole composition.
            if inner.iter().all(|ids| !ids.is_empty()) {
                keep(comp, &inner);
            }
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
