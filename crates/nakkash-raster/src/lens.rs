//! The lens blur: each pixel becomes the mean of the layer over the shape
//! of a lens's iris around it, a polygon of three to ten sides rounded
//! towards a circle, so what the layer drew spreads into soft polygons, as
//! an out-of-focus lens draws it.
//!
//! The iris is taken a row of pixels at a time: across each row it is
//! one stretch, from its left edge to its right, and the running sum of
//! each row of the layer gives the sum over any stretch of it in two
//! reads. A pixel costs two reads a row the iris spans, whatever its
//! shape, and the edges of the iris fall between pixels where they are, so
//! it grows smoothly with its radius.
//!
//! Its radius may change from pixel to pixel by the levels of a blur map:
//! a pixel reads the iris its own level gives, as far from the level in
//! focus as it is.

use std::f64::consts::{PI, TAU};

use vello_cpu::kurbo::{Affine, Point, Rect, Shape};

use crate::colour::{choice, number};
use crate::effect::{ParamValue, Space};
use crate::pixels::{Pixel, Pixels};

/// How many heights across the iris its edges are kept at.
pub const IRIS_ROWS: usize = 1024;
const ROWS: usize = IRIS_ROWS;

/// How many points round its outline the iris is drawn through.
const OUTLINE: usize = 720;

/// The shape of a lens's iris, at a radius of 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Iris {
    /// The highest of it, `y` down.
    pub top: f64,
    /// The lowest of it.
    pub bottom: f64,
    /// Its left and right edges at [`IRIS_ROWS`] heights evenly from `top`
    /// to `bottom`.
    pub edges: Vec<[f64; 2]>,
}

impl Iris {
    /// A polygon of `sides` sides whose corners lie on the unit circle,
    /// one straight up before turning, rounded `roundness` (0 to 1) of
    /// the way to the circle, `aspect` times as wide as it is high, and
    /// turned `rotation` radians clockwise.
    pub(crate) fn new(sides: usize, roundness: f64, aspect: f64, rotation: f64) -> Iris {
        let sides = sides.max(3) as f64;
        let wedge = TAU / sides;
        let roundness = roundness.clamp(0.0, 1.0);
        let (sin, cos) = rotation.sin_cos();
        // The outline, from straight up clockwise: the polygon's distance
        // from its middle at each angle, moved towards the circle's.
        let outline: Vec<Point> = (0..OUTLINE)
            .map(|k| {
                let a = TAU * k as f64 / OUTLINE as f64;
                let from_corner = (a + wedge / 2.0).rem_euclid(wedge) - wedge / 2.0;
                let polygon = (PI / sides).cos() / from_corner.cos();
                let r = polygon + (1.0 - polygon) * roundness;
                let (x, y) = (r * a.sin() * aspect, -r * a.cos());
                Point::new(x * cos - y * sin, x * sin + y * cos)
            })
            .collect();
        let (top, bottom) = outline
            .iter()
            .fold((f64::MAX, f64::MIN), |(t, b), p| (t.min(p.y), b.max(p.y)));
        let edges = (0..ROWS)
            .map(|i| {
                let y = top + (bottom - top) * i as f64 / (ROWS - 1) as f64;
                across(&outline, y)
            })
            .collect();
        Iris { top, bottom, edges }
    }

    /// Its left and right edges at height `y`, between the heights kept.
    fn at(&self, y: f64) -> [f64; 2] {
        let t = ((y - self.top) / (self.bottom - self.top)).clamp(0.0, 1.0) * (ROWS - 1) as f64;
        let i = (t.floor() as usize).min(ROWS - 2);
        let f = t - i as f64;
        let (a, b) = (self.edges[i], self.edges[i + 1]);
        [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
    }

    /// The rows of pixels a pixel reads to spread as the iris at `radius`
    /// px: each row's offset from the pixel's, the share of it covered,
    /// and its left and right edges, in px from the pixel's middle.
    ///
    /// A pixel lights those that read it, so it reads the iris turned half
    /// a turn: the point `(x, y)` of what it reads is `(-x, -y)` of the
    /// iris. For an iris of an odd number of sides that turns it upside
    /// down, so that what spreads is the iris as it stands.
    pub fn rows(&self, radius: f64, rows: &mut Vec<Row>) {
        rows.clear();
        let (top, bottom) = (-self.bottom * radius, -self.top * radius);
        let (first, last) = ((top - 0.5).round() as i64, (bottom + 0.5).round() as i64);
        for dy in first..=last {
            let (low, high) = ((dy as f64 - 0.5).max(top), (dy as f64 + 0.5).min(bottom));
            if high <= low {
                continue;
            }
            let [left, right] = self.at(-(low + high) / 2.0 / radius);
            rows.push(Row {
                dy,
                share: high - low,
                left: -right * radius,
                right: -left * radius,
            });
        }
    }
}

/// The left and right of `outline` where it crosses height `y`.
fn across(outline: &[Point], y: f64) -> [f64; 2] {
    let (mut left, mut right) = (f64::MAX, f64::MIN);
    for (k, a) in outline.iter().enumerate() {
        let b = outline[(k + 1) % outline.len()];
        let (low, high) = (a.y.min(b.y), a.y.max(b.y));
        if y < low || y > high {
            continue;
        }
        let x = match high - low > 1e-12 {
            true => a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y),
            false => a.x.min(b.x),
        };
        let x2 = match high - low > 1e-12 {
            true => x,
            false => a.x.max(b.x),
        };
        left = left.min(x).min(x2);
        right = right.max(x).max(x2);
    }
    match left <= right {
        true => [left, right],
        false => [0.0, 0.0],
    }
}

/// One row of an iris in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    /// Its offset from the middle row.
    pub dy: i64,
    /// The share of the row's height the iris covers, 0 to 1.
    pub share: f64,
    /// Its left edge, in px right of the middle.
    pub left: f64,
    /// Its right edge, in px right of the middle.
    pub right: f64,
}

/// The running sums of every row of a grid: what `sum` reads.
struct RowSums {
    width: usize,
    height: usize,
    /// Row by row, the sum of the pixels before each, and of the row.
    sums: Vec<[f32; 4]>,
    /// The first and last pixel of each row, repeated past the edges.
    ends: Vec<[Pixel; 2]>,
    repeat: bool,
}

impl RowSums {
    fn of(pixels: &Pixels, repeat: bool) -> RowSums {
        let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
        let mut sums = Vec::with_capacity((width + 1) * height);
        let mut ends = Vec::with_capacity(height);
        for row in pixels.data().chunks_exact(width.max(1)) {
            // Summed in f64 and kept in f32: a row of 4,000 px keeps
            // a sum to about a 4,000th of a level.
            let mut sum = [0.0f64; 4];
            sums.push([0.0; 4]);
            for p in row {
                for c in 0..4 {
                    sum[c] += f64::from(p[c]);
                }
                sums.push(sum.map(|s| s as f32));
            }
            ends.push([row[0], row[width - 1]]);
        }
        RowSums {
            width,
            height,
            sums,
            ends,
            repeat,
        }
    }

    /// The sum over row `y` from `x0` to `x1`, px of the grid, the pixel
    /// `i` covering `i` to `i + 1`; what lies past the edges is nothing,
    /// or the edge pixels repeated.
    fn sum(&self, y: i64, x0: f64, x1: f64, out: &mut [f64; 4], weight: f64) {
        let y = match (y >= 0 && (y as usize) < self.height, self.repeat) {
            (true, _) => y as usize,
            (false, true) => y.clamp(0, self.height as i64 - 1) as usize,
            (false, false) => return,
        };
        let (a, b) = (self.prefix(y, x0), self.prefix(y, x1));
        for c in 0..4 {
            out[c] += (b[c] - a[c]) * weight;
        }
    }

    /// The sum over row `y` from its left edge to `x`.
    fn prefix(&self, y: usize, x: f64) -> [f64; 4] {
        let row = &self.sums[y * (self.width + 1)..(y + 1) * (self.width + 1)];
        let w = self.width as f64;
        let [first, last] = self.ends[y];
        if x <= 0.0 {
            return match self.repeat {
                true => first.map(|c| f64::from(c) * x),
                false => [0.0; 4],
            };
        }
        if x >= w {
            let all = row[self.width].map(f64::from);
            return match self.repeat {
                true => std::array::from_fn(|c| all[c] + f64::from(last[c]) * (x - w)),
                false => all,
            };
        }
        let i = x.floor() as usize;
        let f = x - i as f64;
        let (a, b) = (row[i], row[i + 1]);
        std::array::from_fn(|c| f64::from(a[c]) + f64::from(b[c] - a[c]) * f)
    }
}

/// Spreads `pixels` over `iris` at the radius `radius` gives for each
/// pixel, in px: the mean of the layer over the iris that size about the
/// pixel's middle. With `repeat`, the edge pixels lie beyond the edges;
/// without, nothing does, and the edges fade.
pub(crate) fn spread(
    pixels: &mut Pixels,
    iris: &Iris,
    repeat: bool,
    radius: impl Fn(usize) -> f64,
) {
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    if width == 0 || height == 0 {
        return;
    }
    let sums = RowSums::of(pixels, repeat);
    let source = pixels.clone();
    let mut rows = Vec::new();
    let mut held = f64::NAN;
    for (i, out) in pixels.data_mut().iter_mut().enumerate() {
        let r = radius(i);
        // Below a 20th of a px the iris is the pixel itself.
        if r < 0.05 {
            *out = source.data()[i];
            continue;
        }
        if r != held {
            iris.rows(r, &mut rows);
            held = r;
        }
        let (x, y) = ((i % width) as f64 + 0.5, (i / width) as i64);
        let (mut sum, mut weight) = ([0.0f64; 4], 0.0);
        for row in &rows {
            sums.sum(y + row.dy, x + row.left, x + row.right, &mut sum, row.share);
            weight += row.share * (row.right - row.left);
        }
        if weight <= 1e-9 {
            *out = source.data()[i];
            continue;
        }
        *out = sum.map(|s| (s / weight) as f32);
    }
}

/// A level a blur map gives, 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Rec. 601's luma of the straight colour.
    Luminance,
    /// The straight red.
    Red,
    /// The straight green.
    Green,
    /// The straight blue.
    Blue,
    /// The alpha.
    Alpha,
}

impl Level {
    const ALL: [Level; 5] = [
        Level::Luminance,
        Level::Red,
        Level::Green,
        Level::Blue,
        Level::Alpha,
    ];

    fn of(self, [r, g, b, a]: Pixel) -> f64 {
        let straight = |c: f32| if a > 0.0 { f64::from(c / a) } else { 0.0 };
        match self {
            Level::Luminance => straight(r) * 0.299 + straight(g) * 0.587 + straight(b) * 0.114,
            Level::Red => straight(r),
            Level::Green => straight(g),
            Level::Blue => straight(b),
            Level::Alpha => f64::from(a),
        }
    }
}

/// The number of sides of each `iris` choice: six first, as the scene's
/// default, then three to ten.
fn sides(choice: usize) -> usize {
    [6, 3, 4, 5, 7, 8, 9, 10][choice]
}

/// `lens-blur`'s plan: the layer spread over `iris` `radius` px of the
/// grid about each pixel, or, with a map, `radius` times how far each
/// pixel's level of the map is from the level in focus. With `repeat`, the
/// edge pixels lie beyond the frame's edges.
#[derive(Debug, Clone, PartialEq)]
pub struct LensBlur {
    /// The iris's radius, in px of the grid.
    pub radius: f64,
    /// The iris's shape.
    pub iris: Iris,
    /// Whether the edge pixels lie beyond the edges, else nothing does.
    pub repeat: bool,
    /// The blur map, when the effect reads one.
    pub map: Option<LensMap>,
}

/// How a lens blur reads its map: where the map lies on the layer, and
/// how its level gives each pixel's radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensMap {
    /// From the layer's own coordinates to px of the grid, which the map's
    /// coordinates are laid on too.
    pub transform: Affine,
    /// The layer's box, when it has one.
    pub bounds: Option<Rect>,
    /// The map's box, when it has one; the layer's otherwise.
    pub map_bounds: Option<Rect>,
    /// Whether the map's box is stretched over the layer's, else centred
    /// on it.
    pub stretch: bool,
    /// Which level of the map sets a pixel's radius.
    pub level: Level,
    /// Whether the level is taken from 1.
    pub inverted: bool,
    /// The level that stays sharp.
    pub focal: f64,
}

impl LensMap {
    /// The layer's box in its own coordinates on a `width` by `height`
    /// grid: its own, or the frame's.
    pub fn target(&self, width: u16, height: u16) -> Rect {
        self.bounds.unwrap_or_else(|| {
            let frame = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
            (self.transform.inverse() * frame.to_path(0.1)).bounding_box()
        })
    }

    /// Where on the map, in the layer's coordinates, the layer's point
    /// `at` reads, the layer's box being `target`.
    pub fn on_map(&self, at: Point, target: Rect) -> Point {
        let map_box = self.map_bounds.unwrap_or(target);
        match self.stretch {
            true => Point::new(
                map_box.x0 + (at.x - target.x0) / target.width().max(1e-6) * map_box.width(),
                map_box.y0 + (at.y - target.y0) / target.height().max(1e-6) * map_box.height(),
            ),
            false => (at - target.center() + map_box.center().to_vec2()).to_point(),
        }
    }

    /// The map's level of `pixel`, taken from 1 when inverted: how far
    /// from `focal` it is gives the radius.
    pub fn level_of(&self, pixel: Pixel) -> f64 {
        let l = self.level.of(pixel);
        match self.inverted {
            true => 1.0 - l,
            false => l,
        }
    }
}

/// [`LensBlur`] from `lens-blur`'s values in `space`: `iris` the polygon,
/// `roundness` of the way to a circle, `aspect` times as wide as high and
/// turned `rotation` degrees clockwise. With a `map` layer, each pixel's
/// radius is `radius` times how far its level of the map (`map-level`,
/// `inverted` or not) is from `focal-distance`; the map's box lies centred
/// on the layer's, or stretched over it. With `edges` `repeat`, the edge
/// pixels lie beyond the frame's edges.
pub(crate) fn lens_blur_plan(values: &[ParamValue], space: &Space) -> LensBlur {
    let radius = f64::from(number(values, 0));
    let roundness = f64::from(number(values, 1));
    let aspect = f64::from(number(values, 2)).max(1e-3);
    let rotation = f64::from(number(values, 3)).to_radians();
    let focal = f64::from(number(values, 4));
    let iris = Iris::new(sides(choice(values, 5)), roundness, aspect, rotation);
    let level = Level::ALL[choice(values, 6)];
    let stretch = choice(values, 7) == 1;
    let inverted = choice(values, 8) == 1;
    let repeat = choice(values, 9) == 1;
    // A uniform scale of the layer scales the iris with it.
    let scale = {
        let [a, b, c, d, ..] = space.transform.as_coeffs();
        (a * d - b * c).abs().sqrt()
    };
    LensBlur {
        radius: radius * scale,
        iris,
        repeat,
        map: space.input.as_ref().map(|input| LensMap {
            transform: space.transform,
            bounds: space.bounds,
            map_bounds: input.bounds,
            stretch,
            level,
            inverted,
            focal,
        }),
    }
}

/// `lens-blur`, see [`lens_blur_plan`].
pub(crate) fn lens_blur(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let LensBlur {
        radius,
        iris,
        repeat,
        map,
    } = lens_blur_plan(values, space);
    let (Some(map), Some(input)) = (map, &space.input) else {
        return spread(pixels, &iris, repeat, |_| radius);
    };
    let width = usize::from(pixels.width());
    let target = map.target(pixels.width(), pixels.height());
    let to = map.transform;
    let back = to.inverse();
    let source = input.pixels();
    let radii: Vec<f64> = (0..pixels.data().len())
        .map(|i| {
            let at = back * Point::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
            let there = to * map.on_map(at, target);
            radius * (map.level_of(source.sample(there.x, there.y)) - map.focal).abs()
        })
        .collect();
    spread(pixels, &iris, repeat, |i| radii[i]);
}

#[cfg(test)]
#[path = "../tests/unit/lens.rs"]
mod tests;
