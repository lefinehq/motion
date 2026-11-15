//! Effects that draw sound: `audio-spectrum`, a band for each frequency as
//! tall as that frequency is loud, and `audio-waveform`, the wave itself.
//!
//! The engine hands each the window of sound it asked for ([`Heard`]),
//! centred `offset` after the layer's time and `duration` long, so a frame
//! draws alone. Spectra come from our own radix-2 FFT of the Hann-windowed
//! window, as amplitudes: a full-scale sine reaches 1, and `height` px.
//! Bands, dashes, dots and lines are drawn by their distance to each
//! pixel, faded over `softness` of their half thickness, coloured from
//! `inside-color` along their middle to `outside-color` at their edges, and
//! kept to the layer's box.

use std::f64::consts::TAU;

use vello_cpu::kurbo::{Affine, Point, Vec2};

use crate::colour::{choice, number, rgb};
use crate::effect::{ParamValue, Space};
use crate::pixels::Pixels;

/// The sound an effect reads: the window it asked for, per channel, at
/// `rate` samples a second.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    /// The left channel.
    pub left: Vec<f32>,
    /// The right channel.
    pub right: Vec<f32>,
    /// Samples a second.
    pub rate: f32,
}

impl Heard {
    /// Both channels averaged.
    fn mono(&self) -> Vec<f32> {
        self.left
            .iter()
            .zip(&self.right)
            .map(|(l, r)| (l + r) / 2.0)
            .collect()
    }
}

/// Indices of the parameters both effects share, in their rows' order.
mod at {
    pub(super) const START: usize = 0;
    pub(super) const END: usize = 1;
    pub(super) const HEIGHT: usize = 2;
    pub(super) const THICKNESS: usize = 5;
    pub(super) const SOFTNESS: usize = 6;
    pub(super) const INSIDE: usize = 7;
    pub(super) const OUTSIDE: usize = 8;
}

/// A point parameter.
fn point(values: &[ParamValue], i: usize) -> Point {
    match values.get(i) {
        Some(ParamValue::Point([x, y])) => Point::new(f64::from(*x), f64::from(*y)),
        _ => Point::ORIGIN,
    }
}

/// How the bands or the wave are laid: where each stands, in the layer's
/// coordinates, and which way is out.
struct Path {
    start: Point,
    end: Point,
    circle: bool,
}

impl Path {
    /// Where point `k` of `count` stands, and the unit vector out from the
    /// line there: to the left going from `start` to `end`, or out of the
    /// circle.
    fn at(&self, k: usize, count: usize) -> (Point, Vec2) {
        let d = self.end - self.start;
        match self.circle {
            false => {
                let t = match count > 1 {
                    true => k as f64 / (count - 1) as f64,
                    false => 0.5,
                };
                let length = d.hypot().max(f64::MIN_POSITIVE);
                (self.start + d * t, Vec2::new(d.y, -d.x) / length)
            }
            true => {
                let radius = d.hypot();
                let angle = d.y.atan2(d.x) + TAU * k as f64 / count.max(1) as f64;
                let out = Vec2::new(angle.cos(), angle.sin());
                (self.start + out * radius, out)
            }
        }
    }
}

/// Draws into the frame's grid: strokes and dots in the layer's
/// coordinates, mapped by `transform`, kept to the layer's box.
struct Pen<'a> {
    pixels: &'a mut Pixels,
    transform: Affine,
    back: Affine,
    bounds: Option<vello_cpu::kurbo::Rect>,
    /// Half the thickness, in pixels of the grid: where the edge is half
    /// covered.
    half: f64,
    /// How far the edge fades, centred on `half`, in pixels of the grid.
    fade: f64,
    inside: [f32; 4],
    outside: [f32; 4],
}

impl Pen<'_> {
    /// The colour `across` of the way from a stroke's middle to its edge,
    /// turned `hue` degrees.
    fn color(&self, across: f64, hue: f64) -> [f32; 4] {
        let t = across.clamp(0.0, 1.0) as f32;
        let mixed: [f32; 4] =
            std::array::from_fn(|i| self.inside[i] + (self.outside[i] - self.inside[i]) * t);
        match hue == 0.0 {
            true => mixed,
            false => turned(mixed, hue),
        }
    }

    /// Lays `color` with `cover` over the grid's pixel `(x, y)`.
    fn lay(&mut self, x: i64, y: i64, color: [f32; 4], cover: f32) {
        if let Some(bounds) = self.bounds {
            let at = self.back * Point::new(x as f64 + 0.5, y as f64 + 0.5);
            if !bounds.contains(at) {
                return;
            }
        }
        let width = i64::from(self.pixels.width());
        let a = color[3] * cover;
        if a <= 0.0 {
            return;
        }
        let pixel = &mut self.pixels.data_mut()[(y * width + x) as usize];
        for c in 0..3 {
            pixel[c] = color[c] * a + pixel[c] * (1.0 - a);
        }
        pixel[3] = a + pixel[3] * (1.0 - a);
    }

    /// A stroke from `from` to `to`, both in the layer's coordinates, with
    /// round ends, or ending flat at them when `square`.
    fn stroke(&mut self, from: Point, to: Point, square: bool, hue: f64) {
        let (a, b) = (self.transform * from, self.transform * to);
        let reach = self.half + self.fade / 2.0;
        let (x0, x1) = (
            (a.x.min(b.x) - reach).floor() as i64,
            (a.x.max(b.x) + reach).ceil() as i64,
        );
        let (y0, y1) = (
            (a.y.min(b.y) - reach).floor() as i64,
            (a.y.max(b.y) + reach).ceil() as i64,
        );
        let (width, height) = (
            i64::from(self.pixels.width()),
            i64::from(self.pixels.height()),
        );
        let ab = b - a;
        let length2 = ab.hypot2();
        for y in y0.max(0)..y1.min(height) {
            for x in x0.max(0)..x1.min(width) {
                let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
                let t = match length2 > 0.0 {
                    true => ((p - a).dot(ab) / length2).clamp(0.0, 1.0),
                    false => 0.0,
                };
                let near = a + ab * t;
                let distance = match square && length2 > 0.0 {
                    // Butt ends: a box from `from` to `to`, its edges
                    // across the stroke at its ends.
                    true => {
                        let length = length2.sqrt();
                        let along = (p - a).dot(ab) / length;
                        let past = (along - length).max(-along);
                        (p - near).hypot().max(past + self.half)
                    }
                    false => (p - near).hypot(),
                };
                // A box filter a pixel wide over a hard edge, widened by
                // the softness: half covered at `half`.
                let cover = ((self.half - distance) / self.fade + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let color = self.color(distance / reach.max(f64::MIN_POSITIVE), hue);
                self.lay(x, y, color, cover as f32);
            }
        }
    }
}

/// `color` with its hue turned `degrees`, keeping its luma: a rotation
/// about the grey axis.
fn turned(color: [f32; 4], degrees: f64) -> [f32; 4] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    let third: f32 = 1.0 / 3.0;
    let root = third.sqrt();
    let k = |i: usize, j: usize| {
        let base = match i == j {
            true => cos + (1.0 - cos) * third,
            false => (1.0 - cos) * third,
        };
        let turn = match (i + 3 - j) % 3 {
            1 => root * sin,
            2 => -root * sin,
            _ => 0.0,
        };
        base + turn
    };
    let [r, g, b, a] = color;
    let c = [r, g, b];
    let out: [f32; 3] =
        std::array::from_fn(|i| (k(i, 0) * c[0] + k(i, 1) * c[1] + k(i, 2) * c[2]).clamp(0.0, 1.0));
    [out[0], out[1], out[2], a]
}

/// The pen for `values`, over `pixels` in `space`, cleared first unless
/// the layer's drawing stays under it.
fn pen<'a>(
    pixels: &'a mut Pixels,
    values: &[ParamValue],
    space: &Space,
    composite: bool,
) -> Pen<'a> {
    if !composite {
        pixels.data_mut().fill([0.0; 4]);
    }
    let scale = space.transform.determinant().abs().sqrt();
    let thickness = f64::from(number(values, at::THICKNESS)) * scale;
    let softness = f64::from(number(values, at::SOFTNESS)).clamp(0.0, 1.0);
    let half = thickness / 2.0;
    // At least a pixel of fade, so edges are smooth.
    let fade = (half * softness * 2.0).max(1.0);
    let color = |i: usize| {
        let [r, g, b] = rgb(values, i);
        let a = match values.get(i) {
            Some(ParamValue::Color(c)) => c.a,
            _ => 1.0,
        };
        [r, g, b, a]
    };
    Pen {
        pixels,
        transform: space.transform,
        back: space.transform.inverse(),
        bounds: space.bounds,
        half,
        fade,
        inside: color(at::INSIDE),
        outside: color(at::OUTSIDE),
    }
}

/// How far a band must reach, in px, to draw: below it is the rounding
/// of silence.
const SILENT: f64 = 1e-3;

/// `audio-spectrum`.
pub(crate) fn spectrum(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let composite = choice(values, 18) == 1;
    let Some(heard) = space.sound.as_deref() else {
        if !composite {
            pixels.data_mut().fill([0.0; 4]);
        }
        return;
    };
    let (from, to) = (f64::from(number(values, 9)), f64::from(number(values, 10)));
    let bands = (number(values, 11).round() as usize).max(1);
    let hue = f64::from(number(values, 12));
    let (display, side, circle) = (
        choice(values, 13),
        choice(values, 14),
        choice(values, 15) == 1,
    );
    let (average, symmetry) = (choice(values, 16) == 1, choice(values, 17) == 0);
    let height = f64::from(number(values, at::HEIGHT));
    let amplitudes = bands_of(
        &heard.mono(),
        f64::from(heard.rate),
        from,
        to,
        bands,
        average,
    );
    let path = Path {
        start: point(values, at::START),
        end: point(values, at::END),
        circle,
    };
    let thickness = f64::from(number(values, at::THICKNESS));
    let mut pen = pen(pixels, values, space, composite);
    for (k, amplitude) in amplitudes.iter().enumerate() {
        let (base, out) = path.at(k, bands);
        let turn = match bands > 1 {
            true => {
                let t = k as f64 / (bands - 1) as f64;
                match symmetry {
                    true => hue * (1.0 - (2.0 * t - 1.0).abs()),
                    false => hue * t,
                }
            }
            false => 0.0,
        };
        let reach = amplitude * height;
        // A silent band draws nothing, as in the original: no dot or
        // square stands on the line.
        if reach < SILENT {
            continue;
        }
        let sides: &[f64] = match side {
            1 => &[1.0],
            2 => &[-1.0],
            _ => &[1.0, -1.0],
        };
        for &sign in sides {
            let out = out * sign;
            let tip = base + out * reach;
            match display {
                // A column of squares a thickness apart.
                0 => {
                    let step = thickness * 2.0;
                    let mut along = 0.0;
                    while along <= reach && step > 0.0 {
                        let a = base + out * along;
                        let b = base + out * (along + thickness).min(reach.max(along));
                        pen.stroke(a, b, true, turn);
                        along += step;
                    }
                }
                1 => pen.stroke(base, tip, false, turn),
                _ => pen.stroke(tip, tip, false, turn),
            }
        }
    }
}

/// `audio-waveform`.
pub(crate) fn waveform(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let composite = choice(values, 13) == 1;
    let Some(heard) = space.sound.as_deref() else {
        if !composite {
            pixels.data_mut().fill([0.0; 4]);
        }
        return;
    };
    let count = (number(values, 9).round() as usize).max(2);
    let (display, channel, circle) = (
        choice(values, 10),
        choice(values, 11),
        choice(values, 12) == 1,
    );
    let samples = match channel {
        1 => heard.left.clone(),
        2 => heard.right.clone(),
        _ => heard.mono(),
    };
    let height = f64::from(number(values, at::HEIGHT));
    let path = Path {
        start: point(values, at::START),
        end: point(values, at::END),
        circle,
    };
    let mut pen = pen(pixels, values, space, composite);
    let points: Vec<(Point, Point)> = (0..count)
        .map(|k| {
            let (base, out) = path.at(k, count);
            let value = sample_at(&samples, k as f64 / (count - 1) as f64);
            (base, base + out * (value * height))
        })
        .collect();
    for (k, &(base, tip)) in points.iter().enumerate() {
        match display {
            0 => pen.stroke(base, tip, false, 0.0),
            // A line through the points, closed round a circle.
            1 => {
                let next = match (points.get(k + 1), circle) {
                    (Some(&(_, next)), _) => Some(next),
                    (None, true) => points.first().map(|&(_, first)| first),
                    (None, false) => None,
                };
                if let Some(next) = next {
                    pen.stroke(tip, next, false, 0.0);
                }
            }
            _ => pen.stroke(tip, tip, false, 0.0),
        }
    }
}

/// The sample `t` of the way through `samples`, between samples linearly.
fn sample_at(samples: &[f32], t: f64) -> f64 {
    let Some(last) = samples.len().checked_sub(1) else {
        return 0.0;
    };
    let place = t.clamp(0.0, 1.0) * last as f64;
    let i = place.floor() as usize;
    let f = place - i as f64;
    let a = f64::from(samples[i]);
    let b = f64::from(samples[(i + 1).min(last)]);
    a + (b - a) * f
}

/// The amplitude of `bands` frequencies from `from` to `to` Hz in
/// `samples` at `rate`: each band the loudest bin within half a band's
/// spacing of its frequency, or the spectrum read between bins when none
/// falls there. With `average`, the window's quarters, overlapping by
/// half, are averaged (Welch's method).
pub(crate) fn bands_of(
    samples: &[f32],
    rate: f64,
    from: f64,
    to: f64,
    bands: usize,
    average: bool,
) -> Vec<f64> {
    if samples.is_empty() || bands == 0 {
        return vec![0.0; bands];
    }
    let (spectrum, size) = match average && samples.len() >= 8 {
        false => amplitudes(samples),
        true => {
            let length = samples.len() / 4;
            let hop = (length / 2).max(1);
            let mut sum: Vec<f64> = Vec::new();
            let mut count = 0.0;
            let mut size = 0;
            let mut at = 0;
            while at + length <= samples.len() {
                let (part, n) = amplitudes(&samples[at..at + length]);
                if sum.is_empty() {
                    sum = vec![0.0; part.len()];
                    size = n;
                }
                sum.iter_mut().zip(&part).for_each(|(s, p)| *s += p);
                count += 1.0;
                at += hop;
            }
            (sum.into_iter().map(|s| s / count).collect(), size)
        }
    };
    let hertz_per_bin = rate / size as f64;
    let spacing = match bands > 1 {
        true => (to - from) / (bands - 1) as f64,
        false => 0.0,
    };
    (0..bands)
        .map(|k| {
            let centre = from + spacing * k as f64;
            let (low, high) = (centre - spacing.abs() / 2.0, centre + spacing.abs() / 2.0);
            let first = (low / hertz_per_bin).ceil().max(0.0) as usize;
            let last = ((high / hertz_per_bin).floor() as usize).min(spectrum.len() - 1);
            match first <= last && spacing != 0.0 {
                true => spectrum[first..=last].iter().copied().fold(0.0, f64::max),
                false => {
                    let place = (centre / hertz_per_bin).clamp(0.0, (spectrum.len() - 1) as f64);
                    let i = place.floor() as usize;
                    let f = place - i as f64;
                    let next = spectrum[(i + 1).min(spectrum.len() - 1)];
                    spectrum[i] + (next - spectrum[i]) * f
                }
            }
        })
        .collect()
}

/// The amplitude spectrum of `samples` under a Hann window, zero-padded
/// to a power of two: one amplitude a bin from 0 Hz to half the rate, a
/// full-scale sine reaching 1. Also the padded size.
fn amplitudes(samples: &[f32]) -> (Vec<f64>, usize) {
    let n = samples.len();
    let size = n.next_power_of_two().max(2);
    let mut re = vec![0.0; size];
    let mut im = vec![0.0; size];
    let mut gain = 0.0;
    for (i, s) in samples.iter().enumerate() {
        let w = match n > 1 {
            true => 0.5 - 0.5 * (TAU * i as f64 / (n - 1) as f64).cos(),
            false => 1.0,
        };
        re[i] = f64::from(*s) * w;
        gain += w;
    }
    fft(&mut re, &mut im);
    let scale = match gain > 0.0 {
        true => 2.0 / gain,
        false => 0.0,
    };
    let spectrum = (0..=size / 2)
        .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt() * scale)
        .collect();
    (spectrum, size)
}

/// The discrete Fourier transform of `re + i·im`, in place: iterative
/// radix-2, for a power-of-two length.
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());
    // Bit-reversed order.
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= n {
        let angle = -TAU / length as f64;
        for start in (0..n).step_by(length) {
            for k in 0..length / 2 {
                let (sin, cos) = (angle * k as f64).sin_cos();
                let (a, b) = (start + k, start + k + length / 2);
                let (tr, ti) = (re[b] * cos - im[b] * sin, re[b] * sin + im[b] * cos);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        length <<= 1;
    }
}

#[cfg(test)]
#[path = "../tests/unit/sound.rs"]
mod tests;
