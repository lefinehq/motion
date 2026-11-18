//! Generators: effects that draw something of their own on the layer, a
//! gradient, a grid, noise, a beam, or cut it into blinds. Each is worked out at
//! every pixel in the layer's own coordinates, so it moves, turns and
//! scales with the layer.
//!
//! A generator draws where the layer is: where what the layer drew has
//! alpha, which for a solid, the usual canvas of a generator, is its box.

use vello_cpu::kurbo::{Affine, Point};

use crate::colour::{choice, number, rgb};
use crate::composite::Operation;
use crate::effect::{ParamValue, Space};
use crate::pen::Pen;
use crate::pixels::Pixels;

/// The point in `values[i]`.
fn point(values: &[ParamValue], i: usize) -> [f32; 2] {
    match values[i] {
        ParamValue::Point(p) => p,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// The alpha of colour `values[i]`.
fn alpha(values: &[ParamValue], i: usize) -> f32 {
    match values[i] {
        ParamValue::Color(c) => c.a,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// Runs `paint` on every pixel the layer covers, with the pixel's centre
/// in the layer's coordinates, its straight colour and its place in the
/// grid (column, row); `paint` gives the straight colour and alpha the
/// pixel takes, the alpha as a share of the layer's there.
fn each_in_layer(
    pixels: &mut Pixels,
    space: &Space,
    paint: impl Fn([f32; 2], [f32; 4], [u32; 2]) -> [f32; 4],
) {
    let back: Affine = space.transform.inverse();
    let width = usize::from(pixels.width());
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        let coverage = pixel[3];
        if coverage <= 0.0 {
            continue;
        }
        let at = back * Point::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
        let straight = [
            pixel[0] / coverage,
            pixel[1] / coverage,
            pixel[2] / coverage,
            1.0,
        ];
        let place = [(i % width) as u32, (i / width) as u32];
        let [r, g, b, a] =
            paint([at.x as f32, at.y as f32], straight, place).map(|c| c.clamp(0.0, 1.0));
        let a = a * coverage;
        *pixel = [r * a, g * a, b * a, a];
    }
}

/// How `four-color` and `grid` draw over the layer, by name in
/// [`crate::composite::OPERATIONS`]; the first is the default.
pub(crate) const GENERATOR_MODES: &[&str] = &[
    "replace",
    "over",
    "add",
    "multiply",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "color-dodge",
    "color-burn",
    "darken",
    "lighten",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "stencil-alpha",
];

/// How `fractal-noise` draws over the layer, by name in
/// [`crate::composite::OPERATIONS`]; the first is the default.
pub(crate) const NOISE_MODES: &[&str] = &[
    "over",
    "replace",
    "add",
    "multiply",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "color-dodge",
    "color-burn",
    "darken",
    "lighten",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "stencil-alpha",
];

/// The operation `values[i]` picks from `modes`.
fn mode(modes: &[&str], values: &[ParamValue], i: usize) -> Operation {
    Operation::named(modes[choice(values, i)]).expect("every generator mode is an operation")
}

/// What a generator does with what the layer drew, at the pixel `at`: its
/// colour `over`, at `amount`, combined with the layer's straight colour
/// `under` by `operation`, as straight colour and a share of the layer's
/// alpha. `replace` draws it alone at `amount`; `over` lays it over the
/// layer by `amount`; the blend modes blend it with the layer, then lay
/// that over by `amount`.
fn composite(
    operation: Operation,
    under: [f32; 4],
    over: [f32; 3],
    amount: f32,
    at: [u32; 2],
) -> [f32; 4] {
    let [r, g, b, a] = operation.pixel(
        [under[0], under[1], under[2], 1.0],
        [over[0] * amount, over[1] * amount, over[2] * amount, amount],
        at,
    );
    match a > 0.0 {
        true => [r / a, g / a, b / a, a],
        false => [0.0; 4],
    }
}

/// `ramp`: a gradient from `start-color` at `start` to `end-color` at
/// `end`, along the line between them or around `start`; `blend` keeps
/// that much of the layer's own colour.
pub(crate) fn ramp(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let (start, from, end, to) = (
        point(values, 0),
        rgb(values, 1),
        point(values, 2),
        rgb(values, 3),
    );
    let (from_a, to_a) = (alpha(values, 1), alpha(values, 3));
    let blend = number(values, 4);
    let radial = choice(values, 5) == 1;
    let d = [end[0] - start[0], end[1] - start[1]];
    let length2 = d[0] * d[0] + d[1] * d[1];
    each_in_layer(pixels, space, |[x, y], under, _| {
        let v = [x - start[0], y - start[1]];
        let t = match (radial, length2 > 0.0) {
            (_, false) => 1.0,
            (false, true) => (v[0] * d[0] + v[1] * d[1]) / length2,
            (true, true) => ((v[0] * v[0] + v[1] * v[1]) / length2).sqrt(),
        }
        .clamp(0.0, 1.0);
        let c: [f32; 3] = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t);
        let a = from_a + (to_a - from_a) * t;
        let mix = |i: usize| c[i] + (under[i] - c[i]) * blend;
        [mix(0), mix(1), mix(2), a + (1.0 - a) * blend]
    });
}

/// `four-color`: four colours at four points, each pixel the mean of them
/// weighted by the inverse of its distance to each to the power
/// 2 × 100 / `blend`, so a larger `blend` mixes them further.
pub(crate) fn four_color(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let points: [[f32; 2]; 4] = std::array::from_fn(|i| point(values, 2 * i));
    let colors: [[f32; 3]; 4] = std::array::from_fn(|i| rgb(values, 2 * i + 1));
    let power = 2.0 * 100.0 / number(values, 8).max(1.0);
    let (opacity, operation) = (number(values, 9), mode(GENERATOR_MODES, values, 10));
    each_in_layer(pixels, space, |[x, y], under, at| {
        let mut weights = [0.0f32; 4];
        for (w, p) in weights.iter_mut().zip(&points) {
            let d2 = (x - p[0]).powi(2) + (y - p[1]).powi(2);
            *w = match d2 > 1e-6 {
                true => d2.powf(-power / 2.0),
                false => f32::INFINITY,
            };
        }
        let c: [f32; 3] = match weights.iter().position(|w| w.is_infinite()) {
            Some(at) => colors[at],
            None => {
                let total: f32 = weights.iter().sum();
                std::array::from_fn(|i| {
                    weights
                        .iter()
                        .zip(&colors)
                        .map(|(w, c)| w * c[i])
                        .sum::<f32>()
                        / total
                })
            }
        };
        composite(operation, under, c, opacity, at)
    });
}

/// `grid`: lines `border` px wide around cells from `anchor`, each cell
/// reaching to `corner`, or `width` square, or `width` by `height`; with
/// `invert`, the cells instead of the lines.
pub(crate) fn grid(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let (anchor, corner) = (point(values, 0), point(values, 1));
    let (width, height, border) = (number(values, 2), number(values, 3), number(values, 4));
    let (color, opacity) = (rgb(values, 5), number(values, 6));
    let (size, cells) = (choice(values, 7), choice(values, 8) == 1);
    let operation = mode(GENERATOR_MODES, values, 9);
    let cell = match size {
        0 => [(corner[0] - anchor[0]).abs(), (corner[1] - anchor[1]).abs()],
        1 => [width, width],
        _ => [width, height],
    }
    .map(|c| c.max(1e-3));
    // A pixel's share of a line, in px of the layer's own units.
    let line = |at: f32, step: f32| {
        let off = (at / step).rem_euclid(1.0) * step;
        let distance = off.min(step - off);
        (border / 2.0 - distance + 0.5).clamp(0.0, 1.0)
    };
    each_in_layer(pixels, space, |[x, y], under, at| {
        let on = line(x - anchor[0], cell[0]).max(line(y - anchor[1], cell[1]));
        let on = if cells { 1.0 - on } else { on };
        composite(operation, under, color, on * opacity, at)
    });
}

/// `venetian-blinds`: the layer cut into stripes `width` px wide, running
/// across `direction` (0° horizontal stripes), each hidden from one side
/// by `completion`, with a soft edge `feather` px wide.
pub(crate) fn venetian_blinds(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let (completion, direction) = (number(values, 0), number(values, 1).to_radians());
    let (width, feather) = (number(values, 2).max(1e-3), number(values, 3));
    let (sin, cos) = direction.sin_cos();
    each_in_layer(pixels, space, |[x, y], under, _| {
        let along = (x * sin + y * cos).rem_euclid(width);
        let edge = completion * width;
        let shown = match feather > 0.0 {
            true => ((along - edge) / feather + 0.5).clamp(0.0, 1.0),
            false => f32::from(u8::from(along >= edge)),
        };
        [under[0], under[1], under[2], shown]
    });
}

/// The layer's px across one noise cell at a `scale` of 1.
pub const NOISE_CELL: f32 = 100.0;

/// `fractal-noise`: grey value noise in `complexity` octaves, each
/// `sub-scaling` the size of the one before, turned by `sub-rotation`,
/// moved by `sub-offset` and weighed by `sub-influence`; `evolution` moves
/// through a third dimension, one turn a cell. `invert` turns the level
/// over first; contrast and brightness then apply around mid grey, and
/// `overflow` brings the level into 0 to 1.
pub(crate) fn fractal_noise(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let noise = Noise::from_values(values);
    let (opacity, operation) = (number(values, 14), mode(NOISE_MODES, values, 19));
    each_in_layer(pixels, space, |point, under, at| {
        let v = noise.at(point);
        composite(operation, under, [v; 3], opacity, at)
    });
}

/// The settings of fractal noise, read once.
struct Noise {
    contrast: f32,
    brightness: f32,
    rotation: f32,
    scale: [f32; 2],
    offset: [f32; 2],
    complexity: f32,
    influence: f32,
    scaling: f32,
    sub_rotation: f32,
    sub_offset: [f32; 2],
    evolution: f32,
    seed: u32,
    fractal: usize,
    kind: usize,
    overflow: usize,
    invert: bool,
}

impl Noise {
    fn from_values(values: &[ParamValue]) -> Noise {
        let scale = number(values, 3);
        Noise {
            contrast: number(values, 0),
            brightness: number(values, 1),
            rotation: number(values, 2).to_radians(),
            scale: [
                scale * number(values, 4) * NOISE_CELL,
                scale * number(values, 5) * NOISE_CELL,
            ]
            .map(|s| s.max(1e-3)),
            offset: point(values, 6),
            complexity: number(values, 7).max(1.0),
            influence: number(values, 8),
            scaling: number(values, 9).max(0.01),
            sub_rotation: number(values, 10).to_radians(),
            sub_offset: point(values, 11),
            evolution: number(values, 12) / 360.0,
            seed: number(values, 13).max(0.0) as u32,
            fractal: choice(values, 15),
            kind: choice(values, 16),
            overflow: choice(values, 17),
            invert: choice(values, 18) == 1,
        }
    }

    /// The level at `at`, in the layer's coordinates.
    fn at(&self, [x, y]: [f32; 2]) -> f32 {
        let (sin, cos) = self.rotation.sin_cos();
        let (dx, dy) = (x - self.offset[0], y - self.offset[1]);
        let mut p = [
            (dx * cos + dy * sin) / self.scale[0],
            (-dx * sin + dy * cos) / self.scale[1],
        ];
        let (sub_sin, sub_cos) = self.sub_rotation.sin_cos();
        let turbulent = matches!(self.fractal, 1..=3);
        let warp = Warp::of(self.fractal);
        let (mut sum, mut total, mut weight) = (0.0f32, 0.0f32, 1.0f32);
        let octaves = self.complexity.ceil() as u32;
        for octave in 0..octaves {
            // The last octave weighs by the fraction of complexity left.
            let share = (self.complexity - octave as f32).min(1.0);
            let n = value_noise(
                [p[0], p[1], self.evolution],
                self.seed.wrapping_add(octave),
                self.kind,
            );
            let n = if turbulent { (2.0 * n - 1.0).abs() } else { n };
            sum += n * weight * share;
            total += weight * share;
            weight *= self.influence;
            let mut scaled = [p[0] / self.scaling, p[1] / self.scaling];
            // A warping type moves the next octave by the level so far.
            if let Some(warp) = warp {
                let [dx, dy] = warp.by(sum / total.max(f32::EPSILON));
                scaled = [scaled[0] + dx, scaled[1] + dy];
            }
            p = [
                scaled[0] * sub_cos + scaled[1] * sub_sin + self.sub_offset[0] / NOISE_CELL,
                -scaled[0] * sub_sin + scaled[1] * sub_cos + self.sub_offset[1] / NOISE_CELL,
            ];
        }
        let mut v = if total > 0.0 { sum / total } else { 0.5 };
        if self.fractal == 1 {
            // Turbulent smooth: the creases rounded off.
            v = v * v * (3.0 - 2.0 * v);
        } else if self.fractal == 3 {
            v = v.sqrt();
        }
        // Inverted before the contrast and brightness, which then shape the
        // inverted level: a dark, contrasted field keeps its bright lines.
        let v = if self.invert { 1.0 - v } else { v };
        let v = (v - 0.5) * self.contrast + 0.5 + self.brightness;
        match self.overflow {
            1 => 1.0 / (1.0 + (-4.0 * (v - 0.5)).exp()),
            2 => {
                let t = v.rem_euclid(2.0);
                if t > 1.0 { 2.0 - t } else { t }
            }
            _ => v.clamp(0.0, 1.0),
        }
    }
}

/// How a warping fractal type moves each octave after the first: by the
/// level of the octaves before it, in cells of the next octave, so its
/// detail follows the shapes of the larger ones.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Warp {
    /// `smeary`: along the noise's own x axis, by up to one cell either
    /// way, which drags the detail into streaks across.
    Smear,
    /// `swirly`: half a cell in a direction that turns once around as the
    /// level runs from 0 to 1, which curls the detail around the larger
    /// shapes.
    Swirl,
}

impl Warp {
    /// The warp of the fractal type at index `fractal`, if it warps.
    fn of(fractal: usize) -> Option<Warp> {
        match fractal {
            4 => Some(Warp::Smear),
            5 => Some(Warp::Swirl),
            _ => None,
        }
    }

    /// How far it moves the next octave, at `level` so far, 0 to 1.
    fn by(self, level: f32) -> [f32; 2] {
        match self {
            Warp::Smear => [2.0 * level - 1.0, 0.0],
            Warp::Swirl => {
                let (sin, cos) = (std::f32::consts::TAU * level).sin_cos();
                [0.5 * cos, 0.5 * sin]
            }
        }
    }
}

/// Value noise at `p`, 0 to 1, between lattice values blended as `kind`
/// says: 0 `spline`, 1 `soft-linear`, 2 `linear` or 3 `block`.
pub(crate) fn value_noise(p: [f32; 3], seed: u32, kind: usize) -> f32 {
    let cell = p.map(f32::floor);
    let f: [f32; 3] = std::array::from_fn(|i| p[i] - cell[i]);
    let at = cell.map(|c| c as i32);
    if kind == 3 {
        return lattice(at, seed);
    }
    let ease = |t: f32| match kind {
        0 => t * t * t * (t * (t * 6.0 - 15.0) + 10.0),
        1 => t * t * (3.0 - 2.0 * t),
        _ => t,
    };
    let w = f.map(ease);
    let corner = |dx: i32, dy: i32, dz: i32| lattice([at[0] + dx, at[1] + dy, at[2] + dz], seed);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let plane = |dz: i32| {
        lerp(
            lerp(corner(0, 0, dz), corner(1, 0, dz), w[0]),
            lerp(corner(0, 1, dz), corner(1, 1, dz), w[0]),
            w[1],
        )
    };
    lerp(plane(0), plane(1), w[2])
}

/// A lattice point's value, 0 to 1, hashed from its coordinates and seed.
fn lattice([x, y, z]: [i32; 3], seed: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9)
        ^ (x as u32).wrapping_mul(0x85EB_CA6B)
        ^ (y as u32).wrapping_mul(0xC2B2_AE35)
        ^ (z as u32).wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// `beam`: a straight beam along the line from `start` to `end`, `length`
/// of the line long, its front `time × (1 + length)` of the way along: at
/// `time` 0 it has not left `start`, at 1 its back has reached `end`, and
/// with `length` 1 it spans the whole line at `time` 0.5. Its thickness runs
/// from `start-thickness` at `start` to `end-thickness` at `end`, its ends
/// are flat, and its edges fade over `softness` of its half thickness, from
/// `inside-color` along its middle to `outside-color` at its edges. It
/// draws in place of the layer, or over it with `composite` on, kept to
/// the layer's box.
pub(crate) fn beam(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    if choice(values, 9) == 0 {
        pixels.data_mut().fill([0.0; 4]);
    }
    let (start, end) = (point(values, 0), point(values, 1));
    let (length, time) = (number(values, 2), number(values, 3));
    let front = (time * (1.0 + length)).clamp(0.0, 1.0);
    let back = (time * (1.0 + length) - length).clamp(0.0, 1.0);
    if front <= back {
        return;
    }
    let along = |k: f32| {
        let k = f64::from(k);
        let at = |i: usize| f64::from(start[i]) + (f64::from(end[i]) - f64::from(start[i])) * k;
        Point::new(at(0), at(1))
    };
    let (from, to) = (number(values, 4), number(values, 5));
    let thickness = |k: f32| f64::from(from + (to - from) * k);
    let color = |i: usize| {
        let [r, g, b] = rgb(values, i);
        [r, g, b, alpha(values, i)]
    };
    let mut pen = Pen::new(
        pixels,
        space,
        thickness(back).max(thickness(front)),
        f64::from(number(values, 6)),
        [color(7), color(8)],
    );
    pen.beam(
        along(back),
        along(front),
        [thickness(back), thickness(front)],
    );
}

#[cfg(test)]
#[path = "../tests/unit/generate.rs"]
mod tests;
