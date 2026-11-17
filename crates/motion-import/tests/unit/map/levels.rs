//! Unit tests for `src/map/levels.rs`.

use pretty_assertions::assert_eq;

use super::*;

/// Levels data holding `sets`, for RGB, red, green, blue and alpha, with
/// the panel showing `channel`.
pub(crate) fn data(channel: u32, sets: [Set; 5]) -> Vec<u8> {
    let mut data = vec![0x00, 0x0F, 0x10, 0xA7];
    data.extend(channel.to_be_bytes());
    for value in sets.iter().flatten() {
        data.extend((*value as f32).to_be_bytes());
    }
    data
}

#[test]
fn reads_each_channels_levels_in_order() {
    let rgb = [0.137_255, 1.0, 2.1, 0.0, 1.0];
    let blue = [0.094_118, 0.9, 1.3, 0.1, 0.8];
    let sets = read(&data(2, [rgb, IDENTITY, IDENTITY, blue, IDENTITY])).unwrap();
    assert_eq!(sets, [rgb, IDENTITY, IDENTITY, blue, IDENTITY]);
}

#[test]
fn the_panels_channel_changes_nothing_read() {
    let red = [0.1, 1.0, 1.2, 0.0, 1.0];
    let sets = [IDENTITY, red, IDENTITY, IDENTITY, IDENTITY];
    assert_eq!(read(&data(1, sets)), read(&data(2, sets)));
}

#[test]
fn data_laid_out_otherwise_reads_as_nothing() {
    let whole = data(1, [IDENTITY; 5]);
    assert_eq!(read(&whole[..whole.len() - 1]), None);
    let no_power = data(
        1,
        [
            [0.0, 1.0, 0.0, 0.0, 1.0],
            IDENTITY,
            IDENTITY,
            IDENTITY,
            IDENTITY,
        ],
    );
    assert_eq!(read(&no_power), None);
    let mut not_a_number = whole.clone();
    not_a_number[8..12].copy_from_slice(&f32::NAN.to_be_bytes());
    assert_eq!(read(&not_a_number), None);
}
