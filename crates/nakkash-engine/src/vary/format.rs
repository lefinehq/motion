//! The format axis ([D78]): the master on a canvas of another size, with
//! what matters re-placed rather than cropped.
//!
//! The master is evaluated and walked as `check` walks it. Each layer that
//! draws is either background, covering most of the canvas, or content: a
//! text, a logo, a picture, a shape. A `comp` layer that covers the canvas
//! is opened, so its own layers count; a smaller one counts as one piece,
//! a card or a lockup. Content whose boxes over time touch is one group.
//! Each group is scaled into the new title-safe area, placed where it sat
//! relative to the old one, and stacked away from the groups it would now
//! overlap. Backgrounds are scaled to cover the new canvas, aimed at their
//! busiest window: the one whose drawn edges are strongest.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::{BTreeMap, BTreeSet};

use nakkash_format::address::Id;
use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_raster::kurbo::{self, Rect};
use nakkash_scene::{ContentState, walk};

use super::rng::Rng;
use super::wrap::{Layout, Shown, Wrap};
use crate::session::Session;

/// Sizes a spec may name by their aspect, on the master's short side.
const ASPECTS: [(&str, u32, u32); 5] = [
    ("9:16", 9, 16),
    ("16:9", 16, 9),
    ("1:1", 1, 1),
    ("4:5", 4, 5),
    ("5:4", 5, 4),
];

/// A layer covering this share of the canvas on average is background.
const BACKGROUND: f64 = 0.6;

/// A `comp` layer covering this share of the canvas is opened.
const CONTAINER: f64 = 0.6;

/// Boxes nearer than this share of the canvas's short side are one group.
const TOUCHING: f64 = 0.02;

/// The most groups a variant draws the master in.
const MOST_GROUPS: usize = 8;

/// The title-safe inset, as `check` measures it.
const SAFE: f64 = 0.05;

/// Draws one of `sizes` (every aspect but the master's when empty).
pub(crate) fn draw(master: &Session, sizes: &[String], rng: &mut Rng) -> Result<(u32, u32)> {
    let size = &master.scene().root.size;
    let (w, h) = (size.width, size.height);
    let names: Vec<String> = match sizes.is_empty() {
        true => ASPECTS
            .iter()
            .filter(|(_, a, b)| u64::from(*a) * u64::from(h) != u64::from(*b) * u64::from(w))
            .map(|(n, ..)| (*n).to_owned())
            .collect(),
        false => sizes.to_vec(),
    };
    let name = rng.pick(&names).cloned().unwrap_or_else(|| "9:16".into());
    parse_size(&name, w.min(h))
}

/// `name`, an aspect such as `9:16` on a short side of `short` px, or exact
/// px such as `1080x1350`.
pub(crate) fn parse_size(name: &str, short: u32) -> Result<(u32, u32)> {
    let bad = || {
        Diagnostic::new(Kind::Invalid, format!("`{name}` is not a size"))
            .expected("an aspect such as `9:16`, `1:1` or `4:5`, or px such as `1080x1350`")
    };
    if let Some((a, b)) = name.split_once(':') {
        let (a, b): (f64, f64) = (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?);
        if a <= 0.0 || b <= 0.0 {
            return Err(bad());
        }
        let short = f64::from(short);
        // Even sides, as encoders want them.
        let even = |v: f64| ((v / 2.0).round() * 2.0) as u32;
        return Ok(match a <= b {
            true => (even(short), even(short * b / a)),
            false => (even(short * a / b), even(short)),
        });
    }
    let (w, h) = name.split_once('x').ok_or_else(bad)?;
    let (w, h): (u32, u32) = (w.parse().map_err(|_| bad())?, h.parse().map_err(|_| bad())?);
    match w > 0 && h > 0 {
        true => Ok((w, h)),
        false => Err(bad()),
    }
}

/// What the walk saw of one layer over the sampled frames.
#[derive(Debug, Clone, Default)]
struct Seen {
    /// Its address from the master's root.
    ids: Vec<String>,
    /// What it is.
    kind: Piece,
    /// Its box on the canvas at each sampled frame it showed, clipped.
    boxes: Vec<(usize, Rect)>,
    /// The share of the canvas it covered, summed over those frames.
    coverage: f64,
    /// Whether nothing may hide it: an adjusting or filter layer, a camera.
    kept: bool,
}

/// What a layer is, for placing it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Piece {
    #[default]
    Shape,
    Text,
    Picture,
    Comp,
    Nothing,
}

/// The layout of `session`'s master on a `width` by `height` canvas, and a
/// line on what it found.
pub(crate) fn layout(session: &Session, width: u32, height: u32) -> Result<(Layout, String)> {
    let graph = session.graph()?;
    let scene = session.scene();
    let (mw, mh) = (graph.root.width, graph.root.height);
    let canvas = Rect::new(0.0, 0.0, mw, mh);
    let area = mw * mh;
    let frames = session.frames()?;
    let step = (frames.len() / 90).max(1);
    let mut seen: BTreeMap<usize, Seen> = BTreeMap::new();
    for (n, frame) in frames.clone().step_by(step).enumerate() {
        let state = session.frame(f64::from(frame))?;
        for visit in walk(graph, &state) {
            let entry = seen.entry(visit.index).or_insert_with(|| {
                let ids: Vec<String> = visit.ids.iter().map(|s| (*s).to_owned()).collect();
                let parsed: Vec<Id> = ids.iter().filter_map(|i| i.parse().ok()).collect();
                let adjust = scene.layer(&parsed).is_some_and(|l| l.adjust);
                let kind = match &visit.node.content {
                    ContentState::Text { .. } => Piece::Text,
                    ContentState::Image(_) | ContentState::Video { .. } => Piece::Picture,
                    ContentState::Comp(..) => Piece::Comp,
                    ContentState::Rect(_) | ContentState::Shapes(_) => Piece::Shape,
                    _ => Piece::Nothing,
                };
                Seen {
                    ids,
                    kind,
                    kept: adjust
                        || matches!(
                            visit.node.content,
                            ContentState::Camera | ContentState::Filter | ContentState::Audio
                        ),
                    ..Seen::default()
                }
            });
            if !visit.visible || visit.opacity <= 0.01 {
                continue;
            }
            let Some(drawn) = visit.drawn() else { continue };
            let inside = drawn.intersect(canvas);
            if inside.width() <= 0.0 || inside.height() <= 0.0 {
                continue;
            }
            entry.coverage += inside.area() / area;
            entry.boxes.push((n, inside));
        }
    }
    // Containers are opened; what lies inside a piece belongs to it.
    let opened: BTreeSet<Vec<String>> = seen
        .values()
        .filter(|s| s.kind == Piece::Comp && share(s) >= CONTAINER)
        .map(|s| s.ids.clone())
        .collect();
    let piece_of = |ids: &[String]| -> Option<Vec<String>> {
        // The first comp on the way down that is not opened is the piece.
        for depth in 1..=ids.len() {
            let head = &ids[..depth];
            if depth == ids.len() || !opened.contains(head) {
                return Some(head.to_vec());
            }
        }
        None
    };
    let mut pieces: BTreeMap<Vec<String>, Seen> = BTreeMap::new();
    for s in seen.values() {
        if s.kept || s.kind == Piece::Nothing || opened.contains(&s.ids) {
            continue;
        }
        let Some(head) = piece_of(&s.ids) else {
            continue;
        };
        let piece = pieces.entry(head.clone()).or_insert_with(|| Seen {
            ids: head.clone(),
            kind: match head.len() == s.ids.len() {
                true => s.kind,
                false => Piece::Comp,
            },
            ..Seen::default()
        });
        piece.boxes.extend(s.boxes.iter().copied());
        piece.coverage = piece.coverage.max(s.coverage);
        if head.len() == s.ids.len() {
            piece.coverage = s.coverage;
        }
    }
    // A piece's coverage is its own frames' union, frame by frame.
    for piece in pieces.values_mut() {
        let mut per: BTreeMap<usize, Rect> = BTreeMap::new();
        for (n, b) in &piece.boxes {
            per.entry(*n).and_modify(|u| *u = u.union(*b)).or_insert(*b);
        }
        piece.coverage = per.values().map(|b| b.area() / area).sum();
        piece.boxes = per.into_iter().collect();
    }
    let (backgrounds, content): (Vec<&Seen>, Vec<&Seen>) = pieces
        .values()
        .filter(|p| !p.boxes.is_empty())
        .partition(|p| p.kind != Piece::Text && share(p) >= BACKGROUND);
    let groups = group(&content, mw.min(mh) * TOUCHING);
    let address = |ids: &[String]| ids.join("/");
    let all_content: Vec<String> = content.iter().map(|p| address(&p.ids)).collect();
    let all_backgrounds: Vec<String> = backgrounds.iter().map(|p| address(&p.ids)).collect();
    let (nw, nh) = (f64::from(width), f64::from(height));
    let safe = Rect::new(nw * SAFE, nh * SAFE, nw * (1.0 - SAFE), nh * (1.0 - SAFE));
    let old_safe = Rect::new(mw * SAFE, mh * SAFE, mw * (1.0 - SAFE), mh * (1.0 - SAFE));
    let mut shown = Vec::new();
    // A hair past covering, so no edge of a background lands on the frame's.
    let cover = (nw / mw).max(nh / mh) * 1.004;
    let mut note_bg = String::from("no background");
    if !backgrounds.is_empty() || groups.is_empty() {
        let centre = busiest(session, &all_content, cover, (nw, nh))?;
        shown.push(Shown {
            hide: all_content.clone(),
            anchor: centre,
            position: [nw / 2.0, nh / 2.0],
            scale: cover,
        });
        note_bg = format!(
            "{} backgrounds at {:.2}x, centred on {:.0},{:.0}",
            backgrounds.len(),
            cover,
            centre[0],
            centre[1]
        );
    }
    // Text keeps its size on the short side, no larger.
    let nominal = nw.min(nh) / mw.min(mh);
    let mut placed: Vec<(Rect, BTreeSet<usize>, f64, [f64; 2])> = Vec::new();
    for g in &groups {
        let b = g.bounds;
        let scale = nominal
            .min(safe.width() / b.width())
            .min(safe.height() / b.height());
        let (cx, cy) = (b.center().x, b.center().y);
        let u = ((cx - old_safe.x0) / old_safe.width()).clamp(0.0, 1.0);
        let v = ((cy - old_safe.y0) / old_safe.height()).clamp(0.0, 1.0);
        let (w, h) = (b.width() * scale, b.height() * scale);
        let x = (safe.x0 + u * safe.width()).clamp(safe.x0 + w / 2.0, safe.x1 - w / 2.0);
        let y = (safe.y0 + v * safe.height()).clamp(safe.y0 + h / 2.0, safe.y1 - h / 2.0);
        placed.push((
            Rect::new(x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0),
            g.frames.clone(),
            scale,
            [cx, cy],
        ));
    }
    stack(&mut placed, &groups, safe);
    for (g, (r, _, scale, anchor)) in groups.iter().zip(&placed) {
        let mut hide: Vec<String> = all_backgrounds.clone();
        hide.extend(
            all_content
                .iter()
                .filter(|a| !g.members.contains(*a))
                .cloned(),
        );
        shown.push(Shown {
            hide,
            anchor: *anchor,
            position: [r.center().x, r.center().y],
            scale: *scale,
        });
    }
    let texts = content.iter().filter(|p| p.kind == Piece::Text).count();
    let scales: Vec<String> = placed.iter().map(|p| format!("{:.2}", p.2)).collect();
    let note = format!(
        "format {width}x{height}: {} groups of {} pieces ({texts} text) at {}; {note_bg}",
        groups.len(),
        content.len(),
        scales.join(" ")
    );
    Ok((
        Layout {
            width,
            height,
            shown,
        },
        note,
    ))
}

/// The share of the canvas `s` covers on average while it shows: a
/// background that fades in or out is still one.
fn share(s: &Seen) -> f64 {
    (s.coverage / s.boxes.len().max(1) as f64).min(1.0)
}

/// A group of content placed together.
#[derive(Debug, Clone)]
struct Group {
    /// The addresses of its pieces.
    members: BTreeSet<String>,
    /// The union of their boxes.
    bounds: Rect,
    /// The sampled frames it shows at.
    frames: BTreeSet<usize>,
    /// Its first piece, in paint order.
    first: usize,
}

/// The pieces of `content` grouped: those whose boxes over time come
/// within `gap` of each other are one, then the nearest are merged until
/// at most [`MOST_GROUPS`] are left.
fn group(content: &[&Seen], gap: f64) -> Vec<Group> {
    let mut groups: Vec<Group> = content
        .iter()
        .enumerate()
        .map(|(i, p)| Group {
            members: BTreeSet::from([p.ids.join("/")]),
            bounds: resting(&p.boxes),
            frames: p.boxes.iter().map(|(n, _)| *n).collect(),
            first: i,
        })
        .collect();
    // Pieces are one group when they show together and sit together; one
    // that replaces another in the same place later is placed on its own.
    let near = |a: &Group, b: &Group| {
        !a.frames.is_disjoint(&b.frames)
            && a.bounds.inflate(gap, gap).intersect(b.bounds).area() > 0.0
    };
    loop {
        let mut merged = false;
        'outer: for i in 0..groups.len() {
            for j in i + 1..groups.len() {
                if near(&groups[i], &groups[j]) {
                    let g = groups.remove(j);
                    join(&mut groups[i], g);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            break;
        }
    }
    while groups.len() > MOST_GROUPS {
        let mut best = (0, 1, f64::MAX);
        for i in 0..groups.len() {
            for j in i + 1..groups.len() {
                // Those that never show together merge first: their union
                // is only as large as the larger of them.
                let apart = groups[i].frames.is_disjoint(&groups[j].frames);
                let d =
                    distance(groups[i].bounds, groups[j].bounds) + if apart { 0.0 } else { 1e6 };
                if d < best.2 {
                    best = (i, j, d);
                }
            }
        }
        let g = groups.remove(best.1);
        join(&mut groups[best.0], g);
    }
    groups.sort_by_key(|g| g.first);
    groups
}

fn join(into: &mut Group, other: Group) {
    into.members.extend(other.members);
    into.bounds = into.bounds.union(other.bounds);
    into.frames.extend(other.frames);
    into.first = into.first.min(other.first);
}

/// Where a piece rests: the union of its boxes at the samples where it has
/// not moved since the one before, or of all of them when it never rests.
/// A piece that slides in from an edge is placed where it lands, not over
/// the whole path it travels.
fn resting(boxes: &[(usize, Rect)]) -> Rect {
    let still = boxes.windows(2).filter_map(|w| {
        let ((_, a), (_, b)) = (w[0], w[1]);
        let same = (a.x0 - b.x0).abs() < 0.5
            && (a.y0 - b.y0).abs() < 0.5
            && (a.x1 - b.x1).abs() < 0.5
            && (a.y1 - b.y1).abs() < 0.5;
        same.then_some(b)
    });
    still
        .reduce(|a, b| a.union(b))
        .or_else(|| boxes.iter().map(|(_, b)| *b).reduce(|a, b| a.union(b)))
        .unwrap_or_default()
}

/// The gap between two boxes, 0 when they touch.
fn distance(a: Rect, b: Rect) -> f64 {
    let dx = (a.x0 - b.x1).max(b.x0 - a.x1).max(0.0);
    let dy = (a.y0 - b.y1).max(b.y0 - a.y1).max(0.0);
    dx.hypot(dy)
}

/// Moves placed groups that now overlap, while both show, apart down the
/// canvas in their master order, then fits the stack into `safe`.
fn stack(placed: &mut [(Rect, BTreeSet<usize>, f64, [f64; 2])], groups: &[Group], safe: Rect) {
    let gap = safe.height() * 0.02;
    // Top to bottom as they sat, left before right on one line.
    let mut order: Vec<usize> = (0..placed.len()).collect();
    order.sort_by(|&a, &b| {
        let (ga, gb) = (groups[a].bounds, groups[b].bounds);
        (ga.y0 + ga.x0 * 1e-3).total_cmp(&(gb.y0 + gb.x0 * 1e-3))
    });
    for k in 0..order.len() {
        let i = order[k];
        for &j in &order[..k] {
            let (ri, rj) = (placed[i].0, placed[j].0);
            let together = !placed[i].1.is_disjoint(&placed[j].1);
            if together && ri.intersect(rj).area() > 0.0 {
                let dy = rj.y1 + gap - ri.y0;
                placed[i].0 = ri + kurbo::Vec2::new(0.0, dy);
            }
        }
    }
    let top = placed.iter().map(|p| p.0.y0).fold(f64::MAX, f64::min);
    let bottom = placed.iter().map(|p| p.0.y1).fold(f64::MIN, f64::max);
    if placed.is_empty() || (top >= safe.y0 && bottom <= safe.y1) {
        return;
    }
    // Fit the stack: shrink it about its middle into the safe area.
    let k = (safe.height() / (bottom - top)).min(1.0);
    let mid = (top + bottom) / 2.0;
    let to = safe.center().y;
    for p in placed.iter_mut() {
        let c = p.0.center();
        let (w, h) = (p.0.width() * k, p.0.height() * k);
        let y = to + (c.y - mid) * k;
        p.0 = Rect::new(c.x - w / 2.0, y - h / 2.0, c.x + w / 2.0, y + h / 2.0);
        p.2 *= k;
    }
}

/// The point of the master's canvas to centre its backgrounds on: the
/// middle of the window of the new aspect, scaled by `cover`, whose drawn
/// edges are strongest, over a few frames drawn small with `content`
/// hidden.
fn busiest(
    session: &Session,
    content: &[String],
    cover: f64,
    (nw, nh): (f64, f64),
) -> Result<[f64; 2]> {
    let graph = session.graph()?;
    let (mw, mh) = (graph.root.width, graph.root.height);
    let middle = [mw / 2.0, mh / 2.0];
    // The window, in the master's px.
    let (ww, wh) = (nw / cover, nh / cover);
    if ww >= mw - 1.0 && wh >= mh - 1.0 {
        return Ok(middle);
    }
    let backdrop = Wrap::new(
        (mw, mh),
        None,
        Some(Layout {
            width: mw as u32,
            height: mh as u32,
            shown: vec![Shown {
                hide: content.to_vec(),
                anchor: middle,
                position: middle,
                scale: 1.0,
            }],
        }),
        None,
        Vec::new(),
    );
    let Ok(alone) = session.with_raw(backdrop.apply(session.document().raw())) else {
        return Ok(middle);
    };
    let frames = session.frames()?;
    let n = frames.len().max(1) as u32;
    let shrink = (160.0 / mw.max(mh)).min(1.0);
    let mut columns: Vec<f64> = Vec::new();
    let mut rows: Vec<f64> = Vec::new();
    for k in 1..=3 {
        let frame = frames.start + n * k / 4;
        let Ok(image) = crate::gates::small(&alone, f64::from(frame), shrink) else {
            continue;
        };
        let (w, h) = (image.width() as usize, image.height() as usize);
        columns.resize(w, 0.0);
        rows.resize(h, 0.0);
        let px = image.pixels();
        let lum = |x: usize, y: usize| {
            let i = (y * w + x) * 4;
            0.299 * f64::from(px[i]) + 0.587 * f64::from(px[i + 1]) + 0.114 * f64::from(px[i + 2])
        };
        for (y, row) in rows.iter_mut().enumerate().skip(1) {
            for (x, column) in columns.iter_mut().enumerate().skip(1) {
                let e = (lum(x, y) - lum(x - 1, y)).abs() + (lum(x, y) - lum(x, y - 1)).abs();
                *column += e;
                *row += e;
            }
        }
    }
    let best = |energy: &[f64], window: f64, side: f64| -> f64 {
        let n = energy.len();
        if n == 0 || window >= side {
            return side / 2.0;
        }
        let span = ((window / side) * n as f64).round().max(1.0) as usize;
        let mut sum: f64 = energy[..span.min(n)].iter().sum();
        // Ties go to the middle: a flat background stays centred.
        let mut best = (sum, (n - span) / 2);
        let mid = (n - span) as f64 / 2.0;
        let mut at_mid = sum;
        for start in 1..=n.saturating_sub(span) {
            sum += energy[start + span - 1] - energy[start - 1];
            if start == (n - span) / 2 {
                at_mid = sum;
            }
            let closer = ((start as f64) - mid).abs() < ((best.1 as f64) - mid).abs();
            if sum > best.0 * 1.05 || ((sum - best.0).abs() <= best.0 * 0.05 && closer) {
                best = (sum, start);
            }
        }
        if best.0 <= at_mid * 1.05 {
            best.1 = (n - span) / 2;
        }
        (best.1 as f64 + span as f64 / 2.0) / n as f64 * side
    };
    let x = best(&columns, ww, mw).clamp(ww / 2.0, mw - ww / 2.0);
    let y = best(&rows, wh, mh).clamp(wh / 2.0, mh - wh / 2.0);
    Ok([
        if ww >= mw { mw / 2.0 } else { x },
        if wh >= mh { mh / 2.0 } else { y },
    ])
}

#[cfg(test)]
#[path = "../../tests/unit/vary/format.rs"]
mod tests;
