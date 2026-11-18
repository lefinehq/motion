//! The data a Curves effect keeps for its curves (`aRbp`): a 4-byte head,
//! the five curves as 256-level tables (RGB, red, green, blue, alpha), then
//! the same five as their points, which is what the scene writes. Each
//! curve's points are 16 pairs of big-endian `u16` input and output levels,
//! 0 to 255, then a big-endian `u32` count of the pairs used and an `i32`
//! the panel's selected point. The tables are the points joined by a
//! natural cubic spline, rounded, which the scene's curves reproduce.
//!
//! An animated curve keeps this data at each key, and the keys may have
//! different points. Between two keys the original moves from one curve's
//! levels to the other's, input by input. The scene moves a path key into
//! the next vertex by vertex, which is the same when the keys share their
//! inputs ([`Spline`] is linear in the levels on fixed inputs), so
//! [`on_shared_inputs`] puts every key on the inputs of all of them.

use nakkash_format::spline::Spline;

/// The bytes before the points: the head and the five tables.
const POINTS_AT: usize = 4 + 5 * 256;
/// The bytes of one curve's points.
const CURVE: usize = MOST_POINTS * 4 + 4 + 4;
/// The scene's names for the five curves, in the data's order.
pub(super) const NAMES: [&str; 5] = ["rgb", "red", "green", "blue", "alpha"];

/// One curve: its points from input to output level, each 0 to 1.
pub(super) type Points = Vec<[f64; 2]>;

/// The five curves in `data`, or `None` for data laid out otherwise.
pub(super) fn read(data: &[u8]) -> Option<[Points; 5]> {
    let curves = data.get(POINTS_AT..POINTS_AT + 5 * CURVE)?;
    let curve = |i: usize| -> Option<Points> {
        let bytes = &curves[i * CURVE..(i + 1) * CURVE];
        let count = u32::from_be_bytes(bytes[64..68].try_into().ok()?);
        let count = usize::try_from(count).ok().filter(|&n| n <= MOST_POINTS)?;
        let level = |at: usize| f64::from(u16::from_be_bytes([bytes[at], bytes[at + 1]])) / 255.0;
        let points: Points = (0..count)
            .map(|p| [level(p * 4), level(p * 4 + 2)])
            .collect();
        let fits = points.iter().flatten().all(|c| (0.0..=1.0).contains(c))
            && points.windows(2).all(|w| w[0][0] < w[1][0]);
        fits.then_some(points)
    };
    let [a, b, c, d, e] = [0, 1, 2, 3, 4].map(curve);
    Some([a?, b?, c?, d?, e?])
}

/// The most points a curve keeps, in the data and in the scene.
pub(super) const MOST_POINTS: usize = 16;

/// The largest stray, in levels from 0 to 1, within which a key's curve
/// is kept: one 8-bit level. The project keeps each key as a table of
/// whole levels, so the original is within half a level of the curve
/// between keys, and the scene's own table rounds to the nearest level.
const ONE_LEVEL: f64 = 1.0 / 255.0;

/// The keys of one animated curve on one set of inputs.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Shared {
    /// Every key is the straight line.
    Straight,
    /// Each key's points, all on the same inputs.
    Points(Vec<Points>),
    /// The keys' inputs together are more than a curve keeps.
    TooMany(usize),
    /// A key starts or ends at an input where another does not, and the
    /// spline through its points on the shared inputs strays from its own
    /// by this much, in levels from 0 to 1, more than one level.
    Strays(f64),
}

/// `keys` (each key's points) on the inputs of all of them, so the scene
/// can move from key to key point by point.
///
/// Between a key's own ends, the spline through its levels at more inputs
/// is the key's own spline exactly. Beyond them a key is flat, which a
/// spline does not keep exactly; that stray is
/// fitted and measured ([`Spline::on`]), and more than one level is
/// [`Shared::Strays`].
pub(super) fn on_shared_inputs(keys: &[Points]) -> Shared {
    if keys.iter().all(|points| is_straight(points)) {
        return Shared::Straight;
    }
    let splines: Vec<Spline> = keys.iter().map(|points| Spline::new(points)).collect();
    let mut inputs: Vec<f64> = splines
        .iter()
        .flat_map(|spline| spline.points().iter().map(|p| p[0]))
        .collect();
    inputs.sort_by(f64::total_cmp);
    inputs.dedup();
    if inputs.len() > MOST_POINTS {
        return Shared::TooMany(inputs.len());
    }
    let (points, strays): (Vec<Points>, Vec<f64>) =
        splines.iter().map(|spline| spline.on(&inputs)).unzip();
    let stray = strays.into_iter().fold(0.0, f64::max);
    match stray > ONE_LEVEL {
        true => Shared::Strays(stray),
        false => Shared::Points(points),
    }
}

/// Whether `points` is the straight line, which changes nothing: no
/// points, or only the two ends at their own levels.
pub(super) fn is_straight(points: &[[f64; 2]]) -> bool {
    matches!(points, [] | [[0.0, 0.0], [1.0, 1.0]])
}

/// `points` as the scene's path text, `M x,y L x,y …`.
/// No points is the straight line from end to end.
pub(super) fn path(points: &[[f64; 2]]) -> String {
    let straight = [[0.0, 0.0], [1.0, 1.0]];
    let points = if points.is_empty() {
        &straight[..]
    } else {
        points
    };
    points
        .iter()
        .enumerate()
        .map(|(i, [x, y])| {
            let verb = if i == 0 { "M" } else { "L" };
            format!("{verb} {},{}", round(*x), round(*y))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A level to six places, which keeps every 8-bit level apart.
fn round(level: f64) -> f64 {
    (level * 1e6).round() / 1e6
}

#[cfg(test)]
#[path = "../../tests/unit/map/curves.rs"]
pub(super) mod tests;
