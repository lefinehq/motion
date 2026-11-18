//! Radial blurs: each pixel becomes the mean of the layer along a stretch
//! of the line through it that runs out from a centre (a zoom) or round it
//! (a spin), so what the layer drew streaks away from the centre or turns
//! about it.
//!
//! The mean is taken whole, not from a handful of samples: the layer is
//! read once along every line, one px apart, and the running sums of what
//! it read, and of what it read times where, give the mean over any
//! stretch of the line, flat or fading, in the same time whatever the
//! stretch's length. A stretch of a ray reaching from a pixel at the edge
//! of a 4K frame to the centre costs what one of a few px does.
//!
//! The lines are rays one px apart where they are farthest from the
//! centre, for a zoom, or rings one px apart, for a spin. Each pixel takes
//! the mean along the two lines either side of it, mixed by how near it
//! lies to each, so what is read between them is read as a bilinear sample
//! is.

use std::f64::consts::TAU;

use vello_cpu::kurbo::{Point, Rect, Vec2};

use crate::colour::{choice, number};
use crate::effect::{ParamValue, Space};
use crate::pixels::{Pixel, Pixels};

/// The point in `values[i]`.
fn point(values: &[ParamValue], i: usize) -> [f32; 2] {
    match values[i] {
        ParamValue::Point(p) => p,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// Which way the stretch a pixel takes its mean over lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Stretch {
    /// From the pixel back by this much: towards the centre (a zoom) or
    /// clockwise (a spin) when positive, so what is drawn streaks out
    /// from the centre or counterclockwise; the other way when negative.
    Behind(f64),
    /// This much in all, half of it either side of the pixel.
    Across(f64),
}

/// How the pixels of a stretch weigh in its mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fade {
    /// All alike.
    Flat,
    /// From all at the pixel to none at the stretch's far ends, straight.
    Linear,
}

/// Samples of the layer along one line, one px apart, with their running
/// sums.
///
/// Sample `k` stands for the cell from `first + k - 0.5` to
/// `first + k + 0.5` of the line's coordinate, in which the layer is taken
/// as that sample all through: beyond the samples it is transparent. A
/// ring repeats every `period` of its coordinate, its samples lying within
/// one period from where it starts.
#[derive(Debug, Default)]
struct Line {
    /// The coordinate of the first sample.
    first: f64,
    /// The samples.
    samples: Vec<Pixel>,
    /// The sum of the samples before each, and of them all at the end.
    sums: Vec<[f64; 4]>,
    /// The sum of each sample before each times its coordinate, and of
    /// them all at the end.
    moments: Vec<[f64; 4]>,
    /// For a ring, how far along it its samples repeat.
    period: Option<f64>,
}

impl Line {
    /// Fills the line with `count` samples from `first` on, sample `k`
    /// read at `at(first + k)`, and repeating every `period`, if set.
    fn read(&mut self, first: f64, count: usize, period: Option<f64>, at: impl Fn(f64) -> Pixel) {
        self.first = first;
        self.period = period;
        self.samples.clear();
        self.samples
            .extend((0..count).map(|k| at(first + k as f64)));
        self.sums.clear();
        self.moments.clear();
        let (mut sum, mut moment) = ([0.0f64; 4], [0.0f64; 4]);
        for (k, sample) in self.samples.iter().enumerate() {
            self.sums.push(sum);
            self.moments.push(moment);
            let u = first + k as f64;
            for c in 0..4 {
                sum[c] += f64::from(sample[c]);
                moment[c] += f64::from(sample[c]) * u;
            }
        }
        self.sums.push(sum);
        self.moments.push(moment);
    }

    /// The integrals of the layer, and of it times the coordinate, from
    /// where the samples start (or where the period holding `u` starts)
    /// to `u`.
    fn integrals(&self, u: f64) -> ([f64; 4], [f64; 4]) {
        let start = self.first - 0.5;
        let Some(period) = self.period else {
            return self.within(u - start);
        };
        // A whole period holds `sum` and `moment` about its own start;
        // period q holds the same, shifted by q periods.
        let q = ((u - start) / period).floor();
        let (local, local_moment) = self.within(u - start - q * period);
        let (sum, moment) = (
            self.sums[self.samples.len()],
            self.moments[self.samples.len()],
        );
        let shift = q * period;
        let mut out = ([0.0; 4], [0.0; 4]);
        for c in 0..4 {
            out.0[c] = q * sum[c] + local[c];
            out.1[c] = q * moment[c]
                + period * sum[c] * q * (q - 1.0) / 2.0
                + local_moment[c]
                + shift * local[c];
        }
        out
    }

    /// The integrals from the start of the samples to `offset` past it.
    fn within(&self, offset: f64) -> ([f64; 4], [f64; 4]) {
        let n = self.samples.len();
        if offset <= 0.0 || n == 0 {
            return ([0.0; 4], [0.0; 4]);
        }
        let k = offset.floor() as usize;
        if k >= n {
            return (self.sums[n], self.moments[n]);
        }
        // Part of cell k, from its start to `offset`.
        let (part, low) = (offset - k as f64, self.first + k as f64 - 0.5);
        let high = low + part;
        let (mut sum, mut moment) = (self.sums[k], self.moments[k]);
        for c in 0..4 {
            let x = f64::from(self.samples[k][c]);
            sum[c] += x * part;
            moment[c] += x * (high * high - low * low) / 2.0;
        }
        (sum, moment)
    }

    /// The layer at `u`, between the two samples either side.
    fn at(&self, u: f64) -> [f64; 4] {
        let n = self.samples.len();
        let mut offset = u - self.first;
        if let Some(period) = self.period {
            offset = (offset + 0.5).rem_euclid(period) - 0.5;
        }
        let k = offset.floor();
        let t = offset - k;
        let get = |k: f64| -> [f64; 4] {
            match k >= 0.0 && (k as usize) < n {
                true => self.samples[k as usize].map(f64::from),
                false => [0.0; 4],
            }
        };
        let (a, b) = (get(k), get(k + 1.0));
        std::array::from_fn(|c| a[c] + (b[c] - a[c]) * t)
    }

    /// The weighted sum of the layer from `near` to `far` and the sum of
    /// the weights: all alike, or falling straight from 1 at `near` to 0
    /// at `far`.
    fn stretch(&self, near: f64, far: f64, fade: Fade) -> ([f64; 4], f64) {
        let length = (far - near).abs();
        let (low, high) = (near.min(far), near.max(far));
        let ((sum_low, moment_low), (sum_high, moment_high)) =
            (self.integrals(low), self.integrals(high));
        let sum: [f64; 4] = std::array::from_fn(|c| sum_high[c] - sum_low[c]);
        match fade {
            Fade::Flat => (sum, length),
            Fade::Linear => {
                // w(u) = (u - far) / (near - far), straight in u.
                let span = near - far;
                let out =
                    std::array::from_fn(|c| (moment_high[c] - moment_low[c] - far * sum[c]) / span);
                (out, length / 2.0)
            }
        }
    }

    /// The mean of the layer over the stretches from `near` to each of
    /// `fars`, by `fade`: the layer at `near` itself when they have no
    /// length.
    fn mean(&self, near: f64, fars: &[f64], fade: Fade) -> [f64; 4] {
        let (mut sum, mut weight) = ([0.0; 4], 0.0);
        for &far in fars {
            if (far - near).abs() < 1e-6 {
                continue;
            }
            let (s, w) = self.stretch(near, far, fade);
            for c in 0..4 {
                sum[c] += s[c];
            }
            weight += w;
        }
        // Below a hundredth of a px the stretch is the point itself.
        if weight < 1e-2 {
            return self.at(near);
        }
        sum.map(|s| s / weight)
    }
}

/// Where the layer's pixels lie seen from the centre: the angles they
/// span, counterclockwise from the grid's x axis, and how near and far
/// they are.
#[derive(Debug, Clone, Copy)]
struct View {
    /// The first angle, in radians.
    from: f64,
    /// How wide the angles are: a whole turn when the centre is among the
    /// pixels.
    width: f64,
    /// The nearest a pixel is, in px.
    near: f64,
    /// The farthest a pixel is, in px.
    far: f64,
}

impl View {
    /// The view of the `width` by `height` grid from `centre`.
    fn of(width: f64, height: f64, centre: Point) -> View {
        let grid = Rect::new(0.0, 0.0, width, height);
        let corners = [
            Point::new(0.0, 0.0),
            Point::new(width, 0.0),
            Point::new(0.0, height),
            Point::new(width, height),
        ];
        let far = corners
            .iter()
            .map(|c| c.distance(centre))
            .fold(0.0, f64::max);
        if grid.contains(centre) {
            return View {
                from: 0.0,
                width: TAU,
                near: 0.0,
                far,
            };
        }
        let nearest = Point::new(centre.x.clamp(0.0, width), centre.y.clamp(0.0, height));
        // Outside the grid, the corners span less than half a turn about
        // the direction to the grid's middle.
        let middle = angle(grid.center() - centre);
        let turn = |v: Vec2| {
            let d = angle(v) - middle;
            d - TAU * ((d + std::f64::consts::PI) / TAU).floor()
        };
        let (low, high) = corners
            .iter()
            .map(|c| turn(*c - centre))
            .fold((f64::MAX, f64::MIN), |(l, h), d| (l.min(d), h.max(d)));
        View {
            from: middle + low,
            width: high - low,
            near: nearest.distance(centre),
            far,
        }
    }

    /// Whether the angles go all the way round.
    fn whole(&self) -> bool {
        self.width >= TAU
    }

    /// `a`, turned whole turns to lie at or past `from`.
    fn unwind(&self, a: f64) -> f64 {
        self.from + (a - self.from).rem_euclid(TAU)
    }
}

/// The angle of `v`, counterclockwise from the grid's x axis as drawn
/// (y down), in radians.
fn angle(v: Vec2) -> f64 {
    (-v.y).atan2(v.x)
}

/// The grid's pixels in the order of their buckets, `keys[i]` for pixel
/// `i`, each below `buckets`: every pixel index, those of bucket 0 first,
/// and where each bucket starts in it.
fn sorted(keys: &[u32], buckets: usize) -> (Vec<u32>, Vec<usize>) {
    let mut starts = vec![0usize; buckets + 1];
    for &k in keys {
        starts[k as usize + 1] += 1;
    }
    for b in 0..buckets {
        starts[b + 1] += starts[b];
    }
    let mut next = starts.clone();
    let mut order = vec![0u32; keys.len()];
    for (i, &k) in keys.iter().enumerate() {
        order[next[k as usize]] = i as u32;
        next[k as usize] += 1;
    }
    (order, starts)
}

/// Two neighbouring lines, `j` and `j + 1`, read as they are needed and
/// read once each while the lines are visited in order.
struct Pair {
    /// Line `j`, and which line it holds.
    this: (Line, Option<usize>),
    /// Line `j + 1`, and which line it holds.
    next: (Line, Option<usize>),
}

impl Pair {
    fn new() -> Pair {
        Pair {
            this: (Line::default(), None),
            next: (Line::default(), None),
        }
    }

    /// Lines `j` and `j + 1`, read by `read` unless already held.
    fn at(&mut self, j: usize, read: impl Fn(&mut Line, usize)) -> (&Line, &Line) {
        if self.next.1 == Some(j) {
            std::mem::swap(&mut self.this, &mut self.next);
        }
        if self.this.1 != Some(j) {
            read(&mut self.this.0, j);
            self.this.1 = Some(j);
        }
        if self.next.1 != Some(j + 1) {
            read(&mut self.next.0, j + 1);
            self.next.1 = Some(j + 1);
        }
        (&self.this.0, &self.next.0)
    }
}

/// The centre of pixel `i` of a grid `width` wide.
fn centre_of(i: usize, width: usize) -> Point {
    Point::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5)
}

/// The far ends of the stretch of a line from `near`, `extent` long in
/// the line's own coordinate.
fn ends(near: f64, stretch: Stretch, extent: f64, fars: &mut Vec<f64>) {
    fars.clear();
    match stretch {
        Stretch::Behind(_) => fars.push(near - extent),
        Stretch::Across(_) => {
            fars.push(near - extent / 2.0);
            fars.push(near + extent / 2.0);
        }
    }
}

/// A zoom: every pixel the mean of its ray from `centre` over `stretch`,
/// a share of its distance from the centre, ending at the centre. Rays
/// are one px apart where the grid is farthest from the centre and are
/// read one px at a time.
pub(crate) fn zoom(pixels: &mut Pixels, centre: Point, stretch: Stretch, fade: Fade) {
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let share = match stretch {
        Stretch::Behind(s) | Stretch::Across(s) => s,
    };
    if width == 0 || height == 0 || share.abs() < 1e-9 {
        return;
    }
    let view = View::of(width as f64, height as f64, centre);
    // One px apart at the far end: a ray every 1/far radians.
    let rays = ((view.width * view.far).ceil() as usize).max(1);
    let step = view.width / rays as f64;
    let first = view.near.floor();
    let count = (view.far.ceil() - first) as usize + 2;
    let along = |i: usize| (view.unwind(angle(centre_of(i, width) - centre)) - view.from) / step;
    let keys: Vec<u32> = (0..width * height)
        .map(|i| (along(i).floor().max(0.0) as usize).min(rays - 1) as u32)
        .collect();
    let (order, starts) = sorted(&keys, rays);
    let source = pixels.clone();
    let read = |line: &mut Line, j: usize| {
        let a = view.from + j as f64 * step;
        let (dx, dy) = (a.cos(), -a.sin());
        line.read(first, count, None, |r| {
            source.sample(centre.x + dx * r, centre.y + dy * r)
        });
    };
    let mut pair = Pair::new();
    let mut fars = Vec::with_capacity(2);
    let out = pixels.data_mut();
    for j in 0..rays {
        let range = starts[j]..starts[j + 1];
        if range.is_empty() {
            continue;
        }
        let (this, next) = pair.at(j, read);
        for &i in &order[range] {
            let i = i as usize;
            let r = centre_of(i, width).distance(centre);
            let t = (along(i) - j as f64).clamp(0.0, 1.0);
            ends(r, stretch, r * share, &mut fars);
            // A stretch ends at the centre: it does not cross it.
            for far in &mut fars {
                *far = far.max(0.0);
            }
            let (a, b) = (this.mean(r, &fars, fade), next.mean(r, &fars, fade));
            out[i] = std::array::from_fn(|c| (a[c] + (b[c] - a[c]) * t) as f32);
        }
    }
}

/// A spin: every pixel the mean of its ring about `centre` over
/// `stretch`, an angle in radians. Rings are one px apart and read one px
/// at a time along them.
pub(crate) fn spin(pixels: &mut Pixels, centre: Point, stretch: Stretch, fade: Fade) {
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let turn = match stretch {
        Stretch::Behind(s) | Stretch::Across(s) => s,
    };
    if width == 0 || height == 0 || turn.abs() < 1e-9 {
        return;
    }
    let view = View::of(width as f64, height as f64, centre);
    let inner = view.near.floor() as usize;
    let rings = view.far.ceil() as usize - inner + 1;
    let out_from = |i: usize| centre_of(i, width).distance(centre) - inner as f64;
    let keys: Vec<u32> = (0..width * height)
        .map(|i| (out_from(i).floor().max(0.0) as usize).min(rings - 1) as u32)
        .collect();
    let (order, starts) = sorted(&keys, rings);
    let source = pixels.clone();
    // Ring k has radius `inner + k` and a whole number of samples round
    // it, about one px apart; its coordinate is the angle in samples.
    let round = |k: usize| ((TAU * (inner + k) as f64).ceil() as usize).max(1);
    let read = |line: &mut Line, k: usize| {
        let radius = (inner + k) as f64;
        let n = round(k);
        let step = TAU / n as f64;
        let (first, count) = match view.whole() {
            true => (0.0, n),
            false => (
                (view.from / step).floor(),
                ((view.width / step).ceil() as usize + 2).min(n),
            ),
        };
        line.read(first, count, Some(n as f64), |u| {
            let a = u * step;
            source.sample(centre.x + radius * a.cos(), centre.y - radius * a.sin())
        });
    };
    let mut pair = Pair::new();
    let mut fars = Vec::with_capacity(2);
    let out = pixels.data_mut();
    for k in 0..rings {
        let range = starts[k]..starts[k + 1];
        if range.is_empty() {
            continue;
        }
        let (this, next) = pair.at(k, read);
        let per_radian = [round(k) as f64 / TAU, round(k + 1) as f64 / TAU];
        for &i in &order[range] {
            let i = i as usize;
            let a = angle(centre_of(i, width) - centre);
            let t = (out_from(i) - k as f64).clamp(0.0, 1.0);
            let mut mean = |line: &Line, per_radian: f64| {
                let u = a * per_radian;
                ends(u, stretch, turn * per_radian, &mut fars);
                line.mean(u, &fars, fade)
            };
            let (a, b) = (mean(this, per_radian[0]), mean(next, per_radian[1]));
            out[i] = std::array::from_fn(|c| (a[c] + (b[c] - a[c]) * t) as f32);
        }
    }
}

/// `radial-blur`: what the layer drew streaked along lines out from
/// `center` (`zoom`) or round it (`spin`), over `amount`: a percentage of
/// each pixel's distance from the centre for a zoom, degrees for a spin.
/// `stretch` `behind` takes each pixel's mean back towards the centre or
/// clockwise, so the layer streaks outwards or counterclockwise (the other
/// way for a negative amount); `across` takes it half each way. `fade`
/// `linear` weighs the pixels of a stretch from all at the pixel to none
/// at its end.
pub(crate) fn radial_blur(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let amount = f64::from(number(values, 0));
    let [x, y] = point(values, 1);
    let centre = space.transform * Point::new(f64::from(x), f64::from(y));
    let fade = match choice(values, 4) {
        0 => Fade::Flat,
        _ => Fade::Linear,
    };
    let reach = match choice(values, 2) {
        0 => amount / 100.0,
        _ => amount.to_radians(),
    };
    let stretch = match choice(values, 3) {
        0 => Stretch::Behind(reach),
        _ => Stretch::Across(reach.abs()),
    };
    match choice(values, 2) {
        0 => zoom(pixels, centre, stretch, fade),
        _ => spin(pixels, centre, stretch, fade),
    }
}

#[cfg(test)]
#[path = "../tests/unit/radial.rs"]
mod tests;
