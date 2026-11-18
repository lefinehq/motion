//! Particles: a system that sends out particles over time, drawn in place
//! of what the layer drew. It is worked out afresh at every frame, so any
//! frame draws alone: particle `i` is born when the running total of
//! `rate` from time 0 first passes `i` (at `i / rate` seconds for a rate
//! that holds), its random draws come from its index and the seed, and
//! where it is at an age has a closed form.
//!
//! Motion is in the layer's own px and seconds: a particle leaves the
//! producer at its speed, is pulled down by gravity, and slowed by
//! resistance (a drag in proportion to its velocity).

use vello_cpu::kurbo::Point;

use crate::colour::{choice, number, rgb};
use crate::effect::{ParamValue, Space};
use crate::pixels::Pixels;
use crate::total::Total;

/// The point in `values[i]`.
fn point(values: &[ParamValue], i: usize) -> [f32; 2] {
    match values[i] {
        ParamValue::Point(p) => p,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// The most particles one frame draws, which keeps a frame's cost bounded.
pub const MOST_PARTICLES: usize = 20_000;

/// A random number from 0 to 1, the same for the same seed, particle and draw.
fn random(seed: u32, particle: u64, draw: u32) -> f32 {
    let mut h = u64::from(seed).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ particle.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ u64::from(draw).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Where a particle is after `age` seconds from `start` at `speed`, under
/// `gravity` (downwards) and `resistance` (per second).
fn flight(start: [f32; 2], speed: [f32; 2], gravity: f32, resistance: f32, age: f32) -> [f32; 2] {
    let pull = [0.0, gravity];
    match resistance > 1e-4 {
        true => {
            let k = resistance;
            let slow = (1.0 - (-k * age).exp()) / k;
            std::array::from_fn(|i| start[i] + speed[i] * slow + pull[i] / k * (age - slow))
        }
        false => std::array::from_fn(|i| start[i] + speed[i] * age + 0.5 * pull[i] * age * age),
    }
}

/// `particles`: `rate` particles a second from an ellipse of `radius-x` by
/// `radius-y` px around `position`, each living `longevity` seconds. The
/// births follow the running total of `rate` the space gives, or `rate`
/// held from time 0 without one, so a rate that drops leaves the
/// particles already born to live out their time. They
/// fly out at `velocity` px a second, every way (`explosive`) or towards
/// `direction` (degrees clockwise from up, spread by `extra`), under
/// `gravity` and `resistance`. Each grows from `birth-size` to `death-size`
/// px across, varied by `size-variation`, turns from `birth-color` to
/// `death-color`, and shows at up to `max-opacity` as `opacity-map` says.
pub(crate) fn particles(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let [rate, longevity] = [number(values, 0), number(values, 1)];
    let position = point(values, 2);
    let [
        radius_x,
        radius_y,
        velocity,
        gravity,
        resistance,
        direction,
        extra,
    ] = std::array::from_fn(|i| number(values, 3 + i));
    let [birth_size, death_size, variation, max_opacity] =
        std::array::from_fn(|i| number(values, 10 + i));
    let (birth_color, death_color) = (rgb(values, 14), rgb(values, 15));
    let seed = number(values, 16).max(0.0) as u32;
    let (animation, shape, fade, transfer) = (
        choice(values, 17),
        choice(values, 18),
        choice(values, 19),
        choice(values, 20),
    );
    let mut out = Pixels::transparent(pixels.width(), pixels.height());
    let now = space.time as f32;
    let held;
    let births = match space.total(0) {
        Some(total) => total,
        None => {
            held = Total::constant(rate);
            &held
        }
    };
    if longevity > 0.0 && now >= 0.0 {
        // Those born before `now - longevity` have died.
        let first = births.at(now - longevity).ceil().max(0.0) as u64;
        let last = births.at(now).floor().max(0.0) as u64;
        // Past the cap, the oldest are left out.
        let first = first.max((last + 1).saturating_sub(MOST_PARTICLES as u64));
        let [a, b, c, d, ..] = space.transform.as_coeffs();
        let scale = ((a * d - b * c).abs().sqrt()) as f32;
        for i in first..=last {
            // The total never falls, so once one is never born, none after it is.
            let Some(born) = births.passes(i as f32) else {
                break;
            };
            let age = now - born;
            if !(0.0..longevity).contains(&age) {
                continue;
            }
            let life = age / longevity;
            let draw = |n: u32| random(seed, i, n);
            // A start within the producer's ellipse.
            let (turn, reach) = (draw(0) * std::f32::consts::TAU, draw(1).sqrt());
            let start = [
                position[0] + turn.cos() * reach * radius_x,
                position[1] + turn.sin() * reach * radius_y,
            ];
            let heading = match animation {
                // Every way.
                0 => draw(2) * std::f32::consts::TAU,
                // Towards the direction, spread by `extra`.
                _ => {
                    (direction + (draw(2) - 0.5) * extra * 180.0).to_radians()
                        - std::f32::consts::FRAC_PI_2
                }
            };
            let speed = velocity * (1.0 + (draw(3) - 0.5) * extra.min(2.0));
            let at = flight(
                start,
                [heading.cos() * speed, heading.sin() * speed],
                gravity,
                resistance,
                age,
            );
            let size = (birth_size + (death_size - birth_size) * life)
                * (1.0 + (draw(4) - 0.5) * 2.0 * variation);
            let opacity = max_opacity
                * match fade {
                    // Fade out.
                    0 => 1.0 - life,
                    // Constant.
                    1 => 1.0,
                    // Fade out sharp: holds, then drops in the last fifth.
                    2 => ((1.0 - life) * 5.0).min(1.0),
                    // Fade in.
                    3 => life,
                    // Fade in and out.
                    _ => 1.0 - (2.0 * life - 1.0).abs(),
                };
            let color: [f32; 3] =
                std::array::from_fn(|c| birth_color[c] + (death_color[c] - birth_color[c]) * life);
            let centre = space.transform * Point::new(f64::from(at[0]), f64::from(at[1]));
            splat(
                &mut out,
                [centre.x as f32, centre.y as f32],
                (size * scale / 2.0).max(0.35),
                color,
                opacity.clamp(0.0, 1.0),
                shape,
                transfer,
            );
        }
    }
    *pixels = out;
}

/// Draws one particle of `radius` px at `centre`: a soft dot (`faded`), a
/// solid one (`disc`), or a ring (`bubble`), laid on by `transfer`:
/// over (0), screen (1) or add (2).
fn splat(
    out: &mut Pixels,
    centre: [f32; 2],
    radius: f32,
    color: [f32; 3],
    opacity: f32,
    shape: usize,
    transfer: usize,
) {
    if opacity <= 0.0 {
        return;
    }
    let (width, height) = (i64::from(out.width()), i64::from(out.height()));
    let reach = radius + 1.0;
    let (x0, x1) = (
        (centre[0] - reach).floor() as i64,
        (centre[0] + reach).ceil() as i64,
    );
    let (y0, y1) = (
        (centre[1] - reach).floor() as i64,
        (centre[1] + reach).ceil() as i64,
    );
    let data = out.data_mut();
    for y in y0.max(0)..y1.min(height) {
        for x in x0.max(0)..x1.min(width) {
            let d = ((x as f32 + 0.5 - centre[0]).powi(2) + (y as f32 + 0.5 - centre[1]).powi(2))
                .sqrt();
            let cover = match shape {
                // A soft dot, densest at its centre.
                0 => {
                    let t = (1.0 - d / radius).max(0.0);
                    t * t
                }
                // A solid dot with a soft pixel at its edge.
                1 => (radius - d + 0.5).clamp(0.0, 1.0),
                // A ring a fifth of its radius thick.
                _ => (radius * 0.1 + 0.5 - (d - radius * 0.9).abs()).clamp(0.0, 1.0),
            };
            let a = cover * opacity;
            if a <= 0.0 {
                continue;
            }
            let p = &mut data[(y * width + x) as usize];
            let src = [color[0] * a, color[1] * a, color[2] * a, a];
            *p = match transfer {
                1 => std::array::from_fn(|c| p[c] + src[c] - p[c] * src[c]),
                2 => std::array::from_fn(|c| (p[c] + src[c]).min(1.0)),
                _ => std::array::from_fn(|c| src[c] + p[c] * (1.0 - a)),
            };
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/particles.rs"]
mod tests;
