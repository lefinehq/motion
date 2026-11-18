//! The data a colour cycle effect keeps for its output cycle: 64 stop
//! records of 8 bytes, then a big-endian `u32` count and 4 bytes the
//! import does not read. A stop is a big-endian `u32` position round the
//! cycle, 0 to 65,535 for a whole turn, then its colour as 8-bit alpha,
//! red, green and blue. The beliefs and their checks are in
//! `docs/design/aep-evidence.md` (Colour cycles).
//!
//! The scene draws the cycle as four curves of the phase, one per channel,
//! a point per stop.

use binrw::BinRead;

use crate::record::parse;

/// The scene's names for the cycle's four curves, in the order
/// [`curves`] gives them.
pub(super) const NAMES: [&str; 4] = ["red", "green", "blue", "alpha"];
/// The most stops the scene draws: a curve's most points.
pub(super) const MOST: usize = 16;
/// The most stops the data holds.
const STOPS: usize = 64;
/// A position that is a whole turn.
const TURN: f64 = 65_535.0;

/// One stop of the data.
#[derive(BinRead, Debug, Clone, Copy, PartialEq)]
struct StopRecord {
    position: u32,
    alpha: u8,
    red: u8,
    green: u8,
    blue: u8,
}

/// The output cycle's data.
#[derive(BinRead, Debug)]
struct CycleRecord {
    stops: [StopRecord; STOPS],
    count: u32,
}

/// One stop: its position round the cycle, 0 to 1, and its straight
/// colour, red, green, blue and alpha, each 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Stop {
    pub at: f64,
    pub color: [f64; 4],
}

/// The cycle's stops in `data`, in order, or `None` for data laid out
/// otherwise: the count the data stores, or, where it stores 0, the stops
/// from the first whose positions rise. Positions must rise and stay in a
/// turn.
pub(super) fn read(data: &[u8]) -> Option<Vec<Stop>> {
    let record: CycleRecord = parse(data, "output cycle", ()).ok()?;
    let rising = 1 + record
        .stops
        .windows(2)
        .take_while(|w| w[1].position > w[0].position)
        .count();
    let count = match usize::try_from(record.count).ok()? {
        0 => rising,
        n if n <= STOPS => n,
        _ => return None,
    };
    let stops: Vec<Stop> = record.stops[..count]
        .iter()
        .map(|s| Stop {
            at: f64::from(s.position) / TURN,
            color: [s.red, s.green, s.blue, s.alpha].map(|c| f64::from(c) / 255.0),
        })
        .collect();
    let fits = stops.iter().all(|s| s.at <= 1.0) && stops.windows(2).all(|w| w[0].at < w[1].at);
    fits.then_some(stops)
}

/// The four curves of the cycle, red, green, blue and alpha, as points of
/// phase and level, one per stop.
pub(super) fn curves(stops: &[Stop]) -> [Vec<[f64; 2]>; 4] {
    std::array::from_fn(|c| stops.iter().map(|s| [s.at, s.color[c]]).collect())
}

#[cfg(test)]
#[path = "../../tests/unit/map/cycle.rs"]
pub(super) mod tests;
