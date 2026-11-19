//! The vector blur: each pixel becomes the mean of the layer along a short
//! straight streak whose direction and length come from a field of
//! vectors, the slope of one level of a map layer, so what the layer drew
//! smears along the map's hills or round them.
//!
//! The field is worked out once for the whole grid: the map's level,
//! softened by a Gaussian, its slope at every pixel, and that slope
//! softened again so that where it turns sharply (along a ridge or a
//! valley of the map, where it points two ways at once) it shortens
//! rather than flips. Each pixel then reads the layer along its own
//! streak, a sample a px.

use std::borrow::Cow;

use vello_cpu::kurbo::Vec2;

use crate::blur::{Edges, SIGMA_PER_BLURRINESS, gaussian};
use crate::colour::{choice, number};
use crate::effect::{ParamValue, Space};
use crate::pixels::{Pixel, Pixels};
use crate::plan::Blur;

/// The most samples a streak takes: past it they spread out evenly.
const MOST_SAMPLES: usize = 96;

/// Which streak each pixel takes from its vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Streak {
    /// Along the slope, both ways, as long as the slope is steep.
    Natural,
    /// Along the slope, both ways, always the full length.
    Constant,
    /// Across the slope (along the map's contours), both ways, as long as
    /// the slope is steep.
    Across,
    /// Along the slope, behind the pixel only, as long as the slope is
    /// steep.
    Behind,
    /// Like `Behind`, the pixels of the streak weighing less the farther
    /// they are behind.
    Fading,
}

impl Streak {
    const ALL: [Streak; 5] = [
        Streak::Natural,
        Streak::Constant,
        Streak::Across,
        Streak::Behind,
        Streak::Fading,
    ];
}

/// A level of a map, 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The straight red.
    Red,
    /// The straight green.
    Green,
    /// The straight blue.
    Blue,
    /// The alpha.
    Alpha,
    /// Rec. 601's luma of the straight colour.
    Luminance,
    /// The mean of the straight colour's largest and smallest channel.
    Lightness,
    /// The straight colour's hue, a share of a turn from red.
    Hue,
    /// The straight colour's saturation, by its lightness.
    Saturation,
}

impl Level {
    /// The levels in the order of the `map-level` choices: lightness
    /// first, as the scene's default.
    const ALL: [Level; 8] = [
        Level::Lightness,
        Level::Red,
        Level::Green,
        Level::Blue,
        Level::Alpha,
        Level::Luminance,
        Level::Hue,
        Level::Saturation,
    ];

    fn of(self, [r, g, b, a]: Pixel) -> f32 {
        let straight = |c: f32| if a > 0.0 { c / a } else { 0.0 };
        let (r, g, b) = (straight(r), straight(g), straight(b));
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        match self {
            Level::Red => r,
            Level::Green => g,
            Level::Blue => b,
            Level::Alpha => a,
            Level::Luminance => r * 0.299 + g * 0.587 + b * 0.114,
            Level::Lightness => (max + min) / 2.0,
            Level::Hue => {
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
            Level::Saturation => {
                let (l, d) = ((max + min) / 2.0, max - min);
                if d <= 0.0 {
                    0.0
                } else {
                    d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6)
                }
            }
        }
    }
}

/// The field of vectors over a `width` by `height` grid: at every pixel,
/// the slope of `levels`, softened by a Gaussian of `smoothing` px, in
/// levels a px.
fn slopes(levels: &[f32], width: usize, height: usize, smoothing: f32) -> Vec<[f32; 2]> {
    let at = |x: usize, y: usize| levels[y * width + x];
    // The slope as the field's x and y in one grid, so one blur softens
    // both: x in red, y in green.
    let mut field = Pixels::transparent(width as u16, height as u16);
    for (i, p) in field.data_mut().iter_mut().enumerate() {
        let (x, y) = (i % width, i / width);
        let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
        let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
        let dx = (at(right, y) - at(left, y)) / (right - left).max(1) as f32;
        let dy = (at(x, down) - at(x, up)) / (down - up).max(1) as f32;
        *p = [dx, dy, 0.0, 0.0];
    }
    if smoothing > 0.0 {
        gaussian(&mut field, [smoothing; 2], Edges::Repeat);
    }
    field.data().iter().map(|p| [p[0], p[1]]).collect()
}

/// Smears `pixels` along `field`, a vector a pixel whose length is a
/// share of `amount` px, as `streak` says; `turn` turns every vector, in
/// radians clockwise. `steepest` is the slope that gives the full length.
fn smear(
    pixels: &mut Pixels,
    field: &[[f32; 2]],
    amount: f32,
    turn: f32,
    streak: Streak,
    steepest: f32,
) {
    let width = usize::from(pixels.width());
    let source = pixels.clone();
    let (sin, cos) = turn.sin_cos();
    for (i, out) in pixels.data_mut().iter_mut().enumerate() {
        let [gx, gy] = field[i];
        let slope = gx.hypot(gy);
        if slope <= 1e-9 {
            continue;
        }
        let (mut dx, mut dy) = (gx / slope, gy / slope);
        (dx, dy) = (dx * cos - dy * sin, dx * sin + dy * cos);
        if streak == Streak::Across {
            (dx, dy) = (-dy, dx);
        }
        let length = match streak {
            Streak::Constant => amount,
            _ => amount * (slope / steepest).min(1.0),
        };
        if length.abs() < 0.05 {
            continue;
        }
        let (from, to) = match streak {
            Streak::Behind | Streak::Fading => (-length, 0.0),
            _ => (-length / 2.0, length / 2.0),
        };
        let samples = ((to - from).abs().ceil() as usize + 1).clamp(2, MOST_SAMPLES);
        let centre = Vec2::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
        let (mut sum, mut weight) = ([0.0f32; 4], 0.0f32);
        for k in 0..samples {
            let s = k as f32 / (samples - 1) as f32;
            let along = from + (to - from) * s;
            let w = match streak {
                // Full at the pixel (s = 1), nothing at the far end.
                Streak::Fading => s,
                _ => 1.0,
            };
            let p = source.sample(
                centre.x + f64::from(dx * along),
                centre.y + f64::from(dy * along),
            );
            for c in 0..4 {
                sum[c] += p[c] * w;
            }
            weight += w;
        }
        *out = sum.map(|c| c / weight);
    }
}

/// `vector-blur`'s plan: the layer smeared along the slope of a level of
/// a map layer (its own drawing without one). The map's level is softened
/// by `softness` and its slope by `ridge`, each a standard deviation in px
/// of the layer, and every vector turns `turn` radians clockwise; lengths
/// reach the grid through `scale`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorBlur {
    /// How long the streaks are at the steepest, in px of the layer.
    pub amount: f32,
    /// How far every vector turns, in radians clockwise.
    pub turn: f32,
    /// The standard deviation the slope is softened by, in px of the layer.
    pub ridge: f32,
    /// The standard deviation the map's level is softened by, in px of the
    /// layer.
    pub softness: f32,
    /// How each streak follows the slope.
    pub streak: Streak,
    /// Which level of the map the slope is taken of.
    pub level: Level,
    /// How many px of the grid one px of the layer is.
    pub scale: f32,
}

impl VectorBlur {
    /// The Gaussian that softens the map's level, if it does.
    pub fn softening(&self) -> Option<Blur> {
        (self.softness > 0.0)
            .then(|| Blur::gaussian([self.softness * self.scale; 2], Edges::Repeat))
    }

    /// The Gaussian that softens the slope, if it does.
    pub fn smoothing(&self) -> Option<Blur> {
        let smoothing = self.ridge * self.scale;
        (smoothing > 0.0).then(|| Blur::gaussian([smoothing; 2], Edges::Repeat))
    }
}

/// [`VectorBlur`] from `vector-blur`'s values in `space`: `natural` both
/// ways as long as the slope is steep, `amount` px at the steepest;
/// `constant-length` always `amount` px; `perpendicular` across the
/// slope; `direction-center` and `direction-fading` behind the pixel
/// only, the latter fading. `softness` and `ridge-smoothness` are
/// blurrinesses, and every vector turns `angle` degrees clockwise.
pub(crate) fn vector_blur_plan(values: &[ParamValue], space: &Space) -> VectorBlur {
    // Lengths in the layer's px, through its scale.
    let scale = {
        let [a, b, c, d, ..] = space.transform.as_coeffs();
        (a * d - b * c).abs().sqrt() as f32
    };
    VectorBlur {
        amount: number(values, 0),
        turn: number(values, 1).to_radians(),
        ridge: number(values, 2) * SIGMA_PER_BLURRINESS,
        softness: number(values, 3) * SIGMA_PER_BLURRINESS,
        streak: Streak::ALL[choice(values, 4)],
        level: Level::ALL[choice(values, 5)],
        scale,
    }
}

/// `vector-blur`, see [`vector_blur_plan`].
pub(crate) fn vector_blur(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let VectorBlur {
        amount,
        turn,
        ridge,
        softness,
        streak,
        level,
        scale,
    } = vector_blur_plan(values, space);
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    if width == 0 || height == 0 || amount == 0.0 {
        return;
    }
    // The map is read where it is, and copied only to be softened.
    let mut map = match &space.input {
        Some(input) => Cow::Borrowed(input.pixels()),
        None => Cow::Borrowed(&*pixels),
    };
    if softness > 0.0 {
        gaussian(map.to_mut(), [softness * scale; 2], Edges::Repeat);
    }
    let levels: Vec<f32> = map.data().iter().map(|p| level.of(*p)).collect();
    let field = slopes(&levels, width, height, ridge * scale);
    let steepest = field
        .iter()
        .map(|[x, y]| x.hypot(*y))
        .fold(0.0f32, f32::max);
    if steepest <= 1e-9 && streak != Streak::Constant {
        return;
    }
    smear(
        pixels,
        &field,
        amount * scale,
        turn,
        streak,
        steepest.max(1e-9),
    );
}

#[cfg(test)]
#[path = "../tests/unit/vector.rs"]
mod tests;
