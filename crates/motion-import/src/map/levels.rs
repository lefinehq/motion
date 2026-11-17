//! The data a Levels effect keeps for its levels, in its histogram
//! parameter: a 4-byte head, a big-endian `u32` (the channel the panel
//! shows), then five sets of levels as big-endian `f32`, for RGB, red,
//! green, blue and alpha. Each set is input black, input white, gamma,
//! output black and output white, levels from 0 to 1.
//!
//! The sliders beside it are not animatable: they show the set of the
//! channel the panel picks, and the project stores them only as the panel
//! last showed them. The data is what the effect draws with, so it is what
//! the scene writes: each channel's set that changes anything as a `levels`
//! item of that channel, then the RGB set, the order the effect applies
//! them in.

/// The bytes before the sets: the head and the panel's channel.
const SETS_AT: usize = 8;
/// The values in one set.
const SET: usize = 5;

/// The scene's names for the five channels, in the data's order.
pub(super) const CHANNELS: [&str; 5] = ["rgb", "red", "green", "blue", "alpha"];

/// The scene's names for the values of a set, in the data's order.
pub(super) const NAMES: [&str; SET] = [
    "input-black",
    "input-white",
    "gamma",
    "output-black",
    "output-white",
];

/// One channel's levels, in the order of [`NAMES`].
pub(super) type Set = [f64; SET];

/// The set that changes nothing.
pub(super) const IDENTITY: Set = [0.0, 1.0, 1.0, 0.0, 1.0];

/// The five sets in `data`, or `None` for data laid out otherwise.
pub(super) fn read(data: &[u8]) -> Option<[Set; 5]> {
    let values = data.get(SETS_AT..SETS_AT + 5 * SET * 4)?;
    let value = |i: usize| {
        let bytes: [u8; 4] = values[i * 4..i * 4 + 4].try_into().ok()?;
        let value = f64::from(f32::from_be_bytes(bytes));
        value.is_finite().then_some(value)
    };
    let set = |c: usize| -> Option<Set> {
        let mut set = IDENTITY;
        for (i, slot) in set.iter_mut().enumerate() {
            *slot = round(value(c * SET + i)?);
        }
        // A gamma of 0 or less is no power the effect draws with.
        (set[2] > 0.0).then_some(set)
    };
    let [a, b, c, d, e] = [0, 1, 2, 3, 4].map(set);
    Some([a?, b?, c?, d?, e?])
}

/// A value to six places: the data keeps `f32`s, whose last digits only
/// say how far from the decimal they were typed.
fn round(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

#[cfg(test)]
#[path = "../../tests/unit/map/levels.rs"]
pub(super) mod tests;
