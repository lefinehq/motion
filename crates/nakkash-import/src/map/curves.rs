//! The data a Curves effect keeps for its curves (`aRbp`): a 4-byte head,
//! the five curves as 256-level tables (RGB, red, green, blue, alpha), then
//! the same five as their points, which is what the scene writes. Each
//! curve's points are 16 pairs of big-endian `u16` input and output levels,
//! 0 to 255, then a big-endian `u32` count of the pairs used and an `i32`
//! the panel's selected point. The tables are the points joined by a
//! natural cubic spline, rounded, which the scene's curves reproduce.

/// The bytes before the points: the head and the five tables.
const POINTS_AT: usize = 4 + 5 * 256;
/// The bytes of one curve's points.
const CURVE: usize = 16 * 4 + 4 + 4;
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
        let count = usize::try_from(count).ok().filter(|&n| n <= 16)?;
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
