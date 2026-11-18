//! The colour cycle: a phase read off each pixel, from its own colour and
//! optionally another layer's, sent round a cycle of colours that replace
//! the pixel's own, in whole or in some of its channels.
//!
//! The cycle is four curves of the phase, red, green, blue and alpha, a
//! point per stop of the cycle, each joined straight from point to point
//! and round from the last back to the first, or held at each point's
//! level until the next with `interpolate` `off`.

use crate::channel::{Level, Reading};
use crate::colour::{choice, number};
use crate::effect::{Curve, Input, ParamValue, Space};
use crate::pixels::Pixels;

/// The levels a phase is read from, in the order of `phase-from`'s and
/// `add-from`'s choices.
const PHASE_LEVELS: [Level; 10] = [
    Level::Intensity,
    Level::Red,
    Level::Green,
    Level::Blue,
    Level::Hue,
    Level::Lightness,
    Level::Saturation,
    Level::Value,
    Level::Alpha,
    Level::Off,
];

/// The curve in `values[i]`.
fn curve(values: &[ParamValue], i: usize) -> Curve {
    match values[i] {
        ParamValue::Curve(c) => c,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// The level of the cyclic `curve` at phase `t`, 0 to 1: straight between
/// its points, or held at each point's level until the next, and round
/// the turn from its last point back to its first. No points is `empty`.
fn level_at(curve: &Curve, t: f32, smooth: bool, empty: f32) -> f32 {
    let points = curve.points();
    let (Some(&[x_first, y_first]), Some(&[x_last, y_last])) = (points.first(), points.last())
    else {
        return empty;
    };
    let (x0, y0, x1, y1) = match points.iter().position(|p| p[0] > t) {
        Some(next) if next > 0 => {
            let ([x0, y0], [x1, y1]) = (points[next - 1], points[next]);
            (x0, y0, x1, y1)
        }
        // Before the first point or from the last on: the segment that
        // closes the turn.
        _ => (x_last, y_last, x_first + 1.0, y_first),
    };
    let span = x1 - x0;
    match smooth && span > 0.0 {
        true => y0 + (y1 - y0) * (t - x0).rem_euclid(1.0) / span,
        false => y0,
    }
}

/// The straight colour `[r, g, b]` as hue (a turn), lightness and
/// saturation.
fn to_hls([r, g, b]: [f32; 3]) -> [f32; 3] {
    let pixel = [r, g, b, 1.0];
    [
        Level::Hue.of(pixel, Reading::Straight),
        Level::Lightness.of(pixel, Reading::Straight),
        Level::Saturation.of(pixel, Reading::Straight),
    ]
}

/// Hue (a turn), lightness and saturation as a straight colour.
fn from_hls([h, l, s]: [f32; 3]) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h6 = h.rem_euclid(1.0) * 6.0;
    let x = c * (1.0 - (h6.rem_euclid(2.0) - 1.0).abs());
    let [r, g, b] = match h6 as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// `color-cycle`'s plan: each pixel's phase is a level of its straight
/// colour (`from`), plus a level of the added layer at the same place
/// (`add_from`) combined by `add_mode` when the effect reads one. The
/// phase is repeated `repetitions` times and shifted by `shift` degrees of
/// a turn, then wrapped into 0 to 1, and the cycle's colour there replaces
/// the channels `modify` names: some of red, green and blue, or of hue,
/// lightness and saturation. With `modify_alpha`, the cycle's alpha
/// multiplies the pixel's. `blend` keeps that much of the pixel as it was.
/// Pixels the layer left empty stay empty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorCycle {
    /// The cycle's red, green, blue and alpha, each a curve of the phase.
    pub cycle: [Curve; 4],
    /// How far the phase is shifted, in degrees of a turn.
    pub shift: f32,
    /// How many times the cycle repeats over phases 0 to 1.
    pub repetitions: f32,
    /// How much of the pixel as it was stays, 0 to 1.
    pub blend: f32,
    /// The level of the pixel the phase is read from.
    pub from: Level,
    /// The level of the added layer's pixel added to the phase.
    pub add_from: Level,
    /// How the added level combines with the phase: 0 wraps their sum
    /// into 0 to 1, 1 clamps it at 1, 2 takes their mean, 3 screens them.
    pub add_mode: usize,
    /// The channels the cycle's colour replaces, by `modify`'s choices: 0
    /// red, green and blue, 1 to 3 one of them, 4 to 6 red and green, green
    /// and blue, red and blue; 7 to 9 hue, lightness or saturation, 10 to
    /// 12 hue and lightness, lightness and saturation, hue and saturation;
    /// any other none.
    pub modify: usize,
    /// Whether the cycle runs straight between its points, rather than
    /// holding each point's level until the next.
    pub smooth: bool,
    /// Whether the cycle's alpha multiplies the pixel's.
    pub modify_alpha: bool,
}

/// [`ColorCycle`] from `color-cycle`'s values.
pub(crate) fn color_cycle_plan(values: &[ParamValue], _: &Space) -> ColorCycle {
    ColorCycle {
        cycle: [0, 1, 2, 3].map(|i| curve(values, i)),
        shift: number(values, 4),
        repetitions: number(values, 5),
        blend: number(values, 6),
        from: PHASE_LEVELS[choice(values, 7)],
        add_from: PHASE_LEVELS[choice(values, 8)],
        add_mode: choice(values, 9),
        modify: choice(values, 10),
        smooth: choice(values, 11) == 0,
        modify_alpha: choice(values, 12) == 0,
    }
}

/// `color-cycle`, see [`ColorCycle`].
pub(crate) fn color_cycle(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let ColorCycle {
        cycle,
        shift,
        repetitions,
        blend,
        from,
        add_from,
        add_mode,
        modify,
        smooth,
        modify_alpha,
    } = color_cycle_plan(values, space);
    let added = space.input.as_ref().map(Input::pixels);
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        let alpha = pixel[3];
        if alpha <= 0.0 {
            continue;
        }
        let mut phase = from.of(*pixel, Reading::Straight);
        if let Some(added) = added {
            let q = add_from.of(added.data()[i], Reading::Straight);
            phase = match add_mode {
                0 => (phase + q).rem_euclid(1.0),
                1 => (phase + q).min(1.0),
                2 => (phase + q) / 2.0,
                _ => 1.0 - (1.0 - phase) * (1.0 - q),
            };
        }
        let t = (phase * repetitions + shift / 360.0).rem_euclid(1.0);
        let [cr, cg, cb, ca] = std::array::from_fn(|c| {
            // An empty curve is the phase itself in a colour, opaque in
            // alpha.
            let empty = if c == 3 { 1.0 } else { t };
            level_at(&cycle[c], t, smooth, empty).clamp(0.0, 1.0)
        });
        let was = [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha];
        let rgb = match modify {
            // All, red, green, blue, red and green, green and blue, red and blue.
            0..=6 => {
                let takes: [[bool; 3]; 7] = [
                    [true, true, true],
                    [true, false, false],
                    [false, true, false],
                    [false, false, true],
                    [true, true, false],
                    [false, true, true],
                    [true, false, true],
                ];
                let take = takes[modify];
                let new = [cr, cg, cb];
                std::array::from_fn(|c| if take[c] { new[c] } else { was[c] })
            }
            // Hue, lightness, saturation, then pairs of them.
            7..=12 => {
                let takes: [[bool; 3]; 6] = [
                    [true, false, false],
                    [false, true, false],
                    [false, false, true],
                    [true, true, false],
                    [false, true, true],
                    [true, false, true],
                ];
                let take = takes[modify - 7];
                let (old, new) = (to_hls(was), to_hls([cr, cg, cb]));
                from_hls(std::array::from_fn(
                    |c| if take[c] { new[c] } else { old[c] },
                ))
            }
            _ => was,
        };
        let a = if modify_alpha { alpha * ca } else { alpha };
        let mixed: [f32; 3] = std::array::from_fn(|c| rgb[c] + (was[c] - rgb[c]) * blend);
        let a = a + (alpha - a) * blend;
        *pixel = [mixed[0] * a, mixed[1] * a, mixed[2] * a, a];
    }
}

#[cfg(test)]
#[path = "../tests/unit/cycle.rs"]
mod tests;
