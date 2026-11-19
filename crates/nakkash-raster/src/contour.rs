//! Contours: the outlines where a level of a layer's drawing crosses a
//! threshold, traced by marching squares (Lorensen and Cline's marching
//! cubes in two dimensions) over the pixel centres, chained into paths
//! and simplified by Douglas and Peucker's algorithm. `stroke-edges` draws
//! strokes along them with the pen.

use std::collections::HashMap;

use vello_cpu::kurbo::{Affine, BezPath, PathEl, Point, Vec2};

use crate::blur::{Edges, SIGMA_PER_BLURRINESS, gaussian};
use crate::colour::{LUMA, choice, number, rgb};
use crate::distort::layer_box;
use crate::effect::{Input, ParamValue, Space};
use crate::pen::Pen;
use crate::pixels::Pixels;

/// A traced contour: its points in order on the grid, and whether it
/// closes on itself (it does unless it leaves the grid).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Contour {
    /// Its points, in pixels of the grid.
    pub(crate) points: Vec<Point>,
    /// Whether its last point joins its first.
    pub(crate) closed: bool,
}

impl Contour {
    /// Its length in pixels of the grid, the closing piece included.
    pub(crate) fn length(&self) -> f64 {
        let open: f64 = self.points.windows(2).map(|w| (w[1] - w[0]).hypot()).sum();
        match (self.closed, self.points.first(), self.points.last()) {
            (true, Some(a), Some(b)) => open + (*a - *b).hypot(),
            _ => open,
        }
    }
}

/// The key of a crossing on the edge of the grid's cell lattice from the
/// sample `(x, y)` rightwards (`across`) or downwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Crossing {
    x: i64,
    y: i64,
    across: bool,
}

/// The contours where `level` (one value per pixel, `width` by `height`,
/// row by row) crosses `threshold`, at pixel centres, inside wound
/// clockwise on the grid (so the outside is on the left going along),
/// each simplified to within `tolerance` px. Samples past the grid count
/// as 0 and below the threshold, so every contour closes.
pub(crate) fn trace(
    level: &[f32],
    width: usize,
    height: usize,
    threshold: f32,
    tolerance: f64,
) -> Vec<Contour> {
    let on_grid = |x: i64, y: i64| x >= 0 && y >= 0 && x < width as i64 && y < height as i64;
    let at = |x: i64, y: i64| -> f32 {
        match on_grid(x, y) {
            true => level[y as usize * width + x as usize],
            false => 0.0,
        }
    };
    let above = |x: i64, y: i64| on_grid(x, y) && at(x, y) >= threshold;
    // Where along an edge the level crosses, on the grid.
    let point = |c: Crossing| -> Point {
        let (x1, y1) = if c.across {
            (c.x + 1, c.y)
        } else {
            (c.x, c.y + 1)
        };
        let (a, b) = (at(c.x, c.y), at(x1, y1));
        let t = match a != b {
            true => f64::from((threshold - a) / (b - a)).clamp(0.0, 1.0),
            false => 0.5,
        };
        Point::new(
            c.x as f64 + 0.5 + t * (x1 - c.x) as f64,
            c.y as f64 + 0.5 + t * (y1 - c.y) as f64,
        )
    };
    // Each cell's pieces, from one crossing to the next with the inside on
    // the right; a cell whose diagonal corners alone are inside joins
    // them through its middle when its mean is inside too.
    let mut next: HashMap<Crossing, Crossing> = HashMap::new();
    for y in -1..height as i64 {
        for x in -1..width as i64 {
            let top = Crossing { x, y, across: true };
            let bottom = Crossing {
                x,
                y: y + 1,
                across: true,
            };
            let left = Crossing {
                x,
                y,
                across: false,
            };
            let right = Crossing {
                x: x + 1,
                y,
                across: false,
            };
            let case = usize::from(above(x, y)) << 3
                | usize::from(above(x + 1, y)) << 2
                | usize::from(above(x + 1, y + 1)) << 1
                | usize::from(above(x, y + 1));
            let mean = (at(x, y) + at(x + 1, y) + at(x + 1, y + 1) + at(x, y + 1)) / 4.0;
            let joined = mean >= threshold;
            let pieces: &[(Crossing, Crossing)] = match case {
                0b0001 => &[(left, bottom)],
                0b0010 => &[(bottom, right)],
                0b0011 => &[(left, right)],
                0b0100 => &[(right, top)],
                0b0101 if joined => &[(left, top), (right, bottom)],
                0b0101 => &[(left, bottom), (right, top)],
                0b0110 => &[(bottom, top)],
                0b0111 => &[(left, top)],
                0b1000 => &[(top, left)],
                0b1001 => &[(top, bottom)],
                0b1010 if joined => &[(top, right), (bottom, left)],
                0b1010 => &[(top, left), (bottom, right)],
                0b1011 => &[(top, right)],
                0b1100 => &[(right, left)],
                0b1101 => &[(right, bottom)],
                0b1110 => &[(bottom, left)],
                _ => &[],
            };
            for &(from, to) in pieces {
                next.insert(from, to);
            }
        }
    }
    // Chained from the topmost, then leftmost crossing still unused, so the
    // order and the start of each contour do not hang on the map's order.
    let mut starts: Vec<Crossing> = next.keys().copied().collect();
    starts.sort_by_key(|c| (c.y, c.x, !c.across));
    let mut contours = Vec::new();
    for start in starts {
        if !next.contains_key(&start) {
            continue;
        }
        let mut points = vec![point(start)];
        let mut at = start;
        while let Some(to) = next.remove(&at) {
            if to == start {
                break;
            }
            points.push(point(to));
            at = to;
        }
        contours.push(Contour {
            points: simplified(&points, tolerance, true),
            closed: true,
        });
    }
    contours
}

/// `points` simplified to within `tolerance` px by Douglas and Peucker's
/// algorithm, keeping the first point; a closed path is split at the point
/// farthest from its first so both halves have ends.
pub(crate) fn simplified(points: &[Point], tolerance: f64, closed: bool) -> Vec<Point> {
    if points.len() < 3 || tolerance <= 0.0 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    let last = points.len() - 1;
    keep[last] = true;
    let mut spans = vec![(0, last)];
    if closed {
        let far = (1..points.len())
            .max_by(|&a, &b| {
                (points[a] - points[0])
                    .hypot2()
                    .total_cmp(&(points[b] - points[0]).hypot2())
            })
            .unwrap_or(last);
        keep[far] = true;
        spans = vec![(0, far), (far, last)];
    }
    while let Some((a, b)) = spans.pop() {
        if b <= a + 1 {
            continue;
        }
        let (p, q) = (points[a], points[b]);
        let line = q - p;
        let length = line.hypot();
        let (far, distance) = (a + 1..b)
            .map(|i| {
                let d = match length > 0.0 {
                    true => line.cross(points[i] - p).abs() / length,
                    false => (points[i] - p).hypot(),
                };
                (i, d)
            })
            .fold((a, -1.0), |best, c| if c.1 > best.1 { c } else { best });
        if distance > tolerance {
            keep[far] = true;
            spans.push((a, far));
            spans.push((far, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// The points of `contour` from `from` to `to` px along it, where `to` may
/// pass its length on a closed contour and carry on round from its start.
fn stretch(contour: &Contour, from: f64, to: f64) -> Vec<Point> {
    let mut ring = contour.points.clone();
    if contour.closed
        && let Some(first) = ring.first().copied()
    {
        ring.push(first);
    }
    let length: f64 = ring.windows(2).map(|w| (w[1] - w[0]).hypot()).sum();
    if ring.len() < 2 || length <= 0.0 {
        return ring;
    }
    let laps = if contour.closed { 2 } else { 1 };
    let mut out = Vec::new();
    let mut walked = 0.0;
    for lap in 0..laps {
        for w in ring.windows(2) {
            let piece = (w[1] - w[0]).hypot();
            let (start, end) = (walked, walked + piece);
            walked = end;
            if end < from || start > to || piece <= 0.0 {
                continue;
            }
            let at = |d: f64| w[0] + (w[1] - w[0]) * ((d - start) / piece).clamp(0.0, 1.0);
            if out.is_empty() {
                out.push(at(from.max(start)));
            }
            out.push(at(to.min(end)));
        }
        if lap == 0 && to <= length {
            break;
        }
    }
    out
}

/// A share from 0 to 1 that `seed` and `index` give, the same every frame.
fn phase_of(seed: u32, index: usize) -> f64 {
    let mut h = u64::from(seed) << 32 ^ index as u64;
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// The level of `pixel` (premultiplied) that `channel` reads, 0 to 1: its
/// colour over black, or its alpha.
fn level_of(pixel: [f32; 4], channel: usize) -> f32 {
    let [r, g, b, a] = pixel;
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    match channel {
        1 => r,
        2 => g,
        3 => b,
        4 => a,
        5 => {
            let d = max - min;
            if d <= 0.0 {
                0.0
            } else if max == r {
                ((g - b) / d).rem_euclid(6.0) / 6.0
            } else if max == g {
                ((b - r) / d + 2.0) / 6.0
            } else {
                ((r - g) / d + 4.0) / 6.0
            }
        }
        6 => (max + min) / 2.0,
        7 => {
            let (l, d) = ((max + min) / 2.0, max - min);
            if d <= 0.0 {
                0.0
            } else {
                d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6)
            }
        }
        8 => max,
        _ => r * LUMA[0] + g * LUMA[1] + b * LUMA[2],
    }
}

/// `contour-strokes`: strokes along the contours where `channel` of the
/// layer's drawing (or of the `layer` it reads, centred on its box)
/// crosses `threshold`, after a blur of `pre-blur` and simplified to
/// within `tolerance` px, or along the layer's mask with `source`
/// `outline`. Each contour, or only the `contour`-th with `render`
/// `selected`, takes `segments` strokes spaced evenly round it (fewer on
/// shorter ones with `shorter` `fewer`), each `length` of its share long,
/// starting `rotation` of a turn round from the contour's start, and a
/// random share more with `random-phase` on. A stroke is `width` px wide,
/// its edge softened by 1 − `hardness`, and its opacity runs from
/// `start-opacity` through `mid-opacity` more than the straight line at
/// `mid-position` to `end-opacity`. `mode` draws the strokes alone
/// (`transparent`), over or under the layer, or keeps the layer only where
/// they are (`stencil`).
pub(crate) fn contour_strokes(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let threshold = number(values, 0);
    let scale = space.transform.determinant().abs().sqrt();
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let contours: Vec<Contour> = match (choice(values, 15), &space.outline) {
        (1, Some(outline)) => outlined(outline, space),
        (1, None) => Vec::new(),
        _ => {
            let channel = choice(values, 16);
            let source = match &space.input {
                Some(input) => centred(input, pixels, space),
                None => pixels.clone(),
            };
            let mut field = Pixels::transparent(source.width(), source.height());
            for (to, from) in field.data_mut().iter_mut().zip(source.data()) {
                *to = [level_of(*from, channel); 4];
            }
            let blur = number(values, 1) * SIGMA_PER_BLURRINESS * scale as f32;
            if blur > 0.0 {
                gaussian(&mut field, [blur; 2], Edges::Repeat);
            }
            let level: Vec<f32> = field.data().iter().map(|p| p[0]).collect();
            trace(
                &level,
                width,
                height,
                threshold,
                f64::from(number(values, 2)) * scale,
            )
        }
    };
    let chosen: Vec<(usize, &Contour)> = match choice(values, 17) {
        1 => {
            let n = number(values, 3).round().max(1.0) as usize;
            contours
                .get(n - 1)
                .map(|c| (n - 1, c))
                .into_iter()
                .collect()
        }
        _ => contours.iter().enumerate().collect(),
    };
    let longest = chosen.iter().map(|(_, c)| c.length()).fold(0.0, f64::max);
    let segments = number(values, 4).round().max(1.0);
    let share = f64::from(number(values, 5));
    let turn = f64::from(number(values, 6)) / 360.0;
    let seed = number(values, 7).max(0.0) as u32;
    let [r, g, b] = rgb(values, 8);
    let color = [r, g, b, 1.0];
    let [start, lift, middle, end] = [11, 12, 13, 14].map(|i| number(values, i));
    let middle = middle.clamp(1e-3, 1.0 - 1e-3);
    let at_middle = (start + (end - start) * middle + lift).clamp(0.0, 1.0);
    let opacity = |t: f64| {
        let t = t as f32;
        match t <= middle {
            true => start + (at_middle - start) * t / middle,
            false => at_middle + (end - at_middle) * (t - middle) / (1.0 - middle),
        }
    };
    let mut strokes = Pixels::transparent(pixels.width(), pixels.height());
    {
        let mut pen = Pen::new(
            &mut strokes,
            space,
            f64::from(number(values, 9)),
            f64::from(1.0 - number(values, 10)),
            [color, color],
        );
        let back = space.transform.inverse();
        for (index, contour) in chosen {
            let length = contour.length();
            if length <= 0.0 || share <= 0.0 {
                continue;
            }
            let count = match choice(values, 18) {
                1 if longest > 0.0 => (f64::from(segments) * length / longest).round().max(1.0),
                _ => f64::from(segments),
            };
            let random = match choice(values, 19) {
                1 => phase_of(seed, index),
                _ => 0.0,
            };
            for k in 0..count as usize {
                let from = (turn + random + k as f64 / count).rem_euclid(1.0) * length;
                let span = share * length / count;
                let (from, to) = match contour.closed {
                    true => (from, from + span),
                    false => (from, (from + span).min(length)),
                };
                let points: Vec<Point> = stretch(contour, from, to)
                    .into_iter()
                    .map(|p| back * p)
                    .collect();
                pen.trail(&points, opacity);
            }
        }
    }
    let mode = choice(values, 20);
    for (out, s) in pixels.data_mut().iter_mut().zip(strokes.data()) {
        let o = *out;
        *out = match mode {
            1 => std::array::from_fn(|c| s[c] + o[c] * (1.0 - s[3])),
            2 => std::array::from_fn(|c| o[c] + s[c] * (1.0 - o[3])),
            3 => o.map(|c| c * s[3]),
            _ => *s,
        };
    }
}

/// The subpaths of `outline`, in the layer's coordinates, flattened onto
/// the grid as contours.
fn outlined(outline: &BezPath, space: &Space) -> Vec<Contour> {
    let mut contours: Vec<Contour> = Vec::new();
    let path = space.transform * outline.clone();
    vello_cpu::kurbo::flatten(&path, 0.1, |element| match element {
        PathEl::MoveTo(p) => contours.push(Contour {
            points: vec![p],
            closed: false,
        }),
        PathEl::LineTo(p) => {
            if let Some(c) = contours.last_mut() {
                c.points.push(p);
            }
        }
        PathEl::ClosePath => {
            if let Some(c) = contours.last_mut() {
                c.closed = true;
            }
        }
        _ => {}
    });
    contours.retain(|c| c.points.len() > 1);
    contours
}

/// What `input` drew, moved so the middle of its box lies on the middle of
/// the reading layer's.
fn centred(input: &Input, pixels: &Pixels, space: &Space) -> Pixels {
    let target = layer_box(pixels, space);
    let shift = match input.bounds {
        Some(b) => target.center() - b.center(),
        None => Vec2::ZERO,
    };
    let moved = space.transform * Affine::translate(shift) * space.transform.inverse();
    input.pixels().transformed(moved)
}

#[cfg(test)]
#[path = "../tests/unit/contour.rs"]
mod tests;
