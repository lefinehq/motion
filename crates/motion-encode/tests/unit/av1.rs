//! Unit tests for `src/av1.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::colour::to_planes;

#[test]
fn the_sequence_header_is_found_among_a_units_obus() {
    // A temporal delimiter, a sequence header of three bytes, a frame of two.
    let unit = [0x12, 0x00, 0x0a, 0x03, 1, 2, 3, 0x32, 0x02, 9, 9];
    assert_eq!(sequence_header(&unit), Some(&unit[2..7]));
}

#[test]
fn an_extension_byte_is_skipped() {
    let unit = [0x0e, 0x00, 0x02, 7, 7];
    assert_eq!(sequence_header(&unit), Some(&unit[..]));
}

#[rstest]
#[case::no_header(&[0x12, 0x00, 0x32, 0x01, 9])]
#[case::cut_short(&[0x12, 0x00, 0x0a, 0x09, 1])]
#[case::without_sizes(&[0x08, 1, 2])]
#[case::empty(&[])]
fn a_unit_without_a_whole_header_has_none(#[case] unit: &[u8]) {
    assert_eq!(sequence_header(unit), None);
}

#[rstest]
#[case(&[0x05], (5, 1))]
#[case(&[0x80, 0x01], (128, 2))]
#[case(&[0xe5, 0x8e, 0x26], (624_485, 3))]
fn leb128_reads_numbers(#[case] bytes: &[u8], #[case] expected: (usize, usize)) {
    assert_eq!(leb128(bytes), Some(expected));
}

#[test]
fn leb128_needs_an_end() {
    assert_eq!(leb128(&[0x80; 9]), None);
}

#[test]
fn time_base_is_one_frame() {
    let base = time_base(29.97);
    assert_eq!((base.num, base.den), (1000, 29_970));
}

#[test]
fn every_frame_comes_out_in_order_and_the_first_is_a_keyframe() {
    let spec = VideoSpec {
        width: 32,
        height: 16,
        fps: 24.0,
    };
    let mut encoder = Av1::new(spec).unwrap();
    let mut packets = Vec::new();
    for level in 0..6u8 {
        let rgba = [level * 40, 0, 255 - level * 40, 255].repeat(32 * 16);
        packets.extend(encoder.encode(&to_planes(&rgba, 32, 16)).unwrap());
    }
    packets.extend(encoder.finish().unwrap());
    let frames: Vec<u64> = packets.iter().map(|p| p.frame).collect();
    assert_eq!(frames, (0..6).collect::<Vec<_>>());
    assert!(packets[0].keyframe);

    let DecoderConfig::Av1 { record } = encoder.config();
    // The record's marker and version, then the sequence header OBU.
    assert_eq!(record[0], 0x81);
    assert_eq!((record[4] >> 3) & 0xf, 1);
    assert_eq!(Some(&record[4..]), sequence_header(&packets[0].data));
}
