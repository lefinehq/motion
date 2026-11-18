//! Unit tests for `src/map/cycle.rs`.

use pretty_assertions::assert_eq;

use super::*;

/// Output cycle data holding `stops`, each a position (0 to 65,535) and
/// alpha, red, green and blue, with `count` stored; the records past the
/// stops are zero but for one stray byte, as saved data may hold.
pub(crate) fn data(stops: &[(u32, [u8; 4])], count: u32) -> Vec<u8> {
    let mut data = vec![0; 520];
    for (i, (at, argb)) in stops.iter().enumerate() {
        data[i * 8..i * 8 + 4].copy_from_slice(&at.to_be_bytes());
        data[i * 8 + 4..i * 8 + 8].copy_from_slice(argb);
    }
    data[40 * 8 + 3] = 0x80;
    data[512..516].copy_from_slice(&count.to_be_bytes());
    data
}

/// Six stops round the turn, red to magenta.
const HUES: [(u32, [u8; 4]); 6] = [
    (0, [255, 255, 0, 0]),
    (10_922, [255, 255, 255, 0]),
    (21_845, [255, 0, 255, 0]),
    (32_768, [255, 0, 255, 255]),
    (43_690, [255, 0, 0, 255]),
    (54_613, [255, 255, 0, 255]),
];

#[test]
fn reads_the_stored_count_of_stops() {
    let stops = read(&data(&HUES, 6)).unwrap();
    assert_eq!(stops.len(), 6);
    assert_eq!(
        stops[0],
        Stop {
            at: 0.0,
            color: [1.0, 0.0, 0.0, 1.0]
        }
    );
    assert!((stops[3].at - 0.5).abs() < 1e-4);
    assert_eq!(stops[3].color, [0.0, 1.0, 1.0, 1.0]);
}

#[test]
fn without_a_count_the_stops_run_while_their_positions_rise() {
    assert_eq!(read(&data(&HUES, 0)), read(&data(&HUES, 6)));
}

#[test]
fn stops_out_of_order_or_too_many_are_not_read() {
    let mut backwards = HUES;
    backwards.swap(1, 2);
    assert_eq!(read(&data(&backwards, 6)), None);
    assert_eq!(read(&data(&HUES, 65)), None);
    assert_eq!(read(&[0; 100]), None);
}

#[test]
fn the_curves_hold_a_point_per_stop_and_channel() {
    let stops = read(&data(&HUES[..2], 2)).unwrap();
    let [red, green, blue, alpha] = curves(&stops);
    assert_eq!(red, [[0.0, 1.0], [stops[1].at, 1.0]]);
    assert_eq!(green, [[0.0, 0.0], [stops[1].at, 1.0]]);
    assert_eq!(blue, [[0.0, 0.0], [stops[1].at, 0.0]]);
    assert_eq!(alpha, [[0.0, 1.0], [stops[1].at, 1.0]]);
}
