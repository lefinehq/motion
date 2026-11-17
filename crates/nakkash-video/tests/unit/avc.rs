//! Unit tests for `src/avc.rs`, on the test clips' parameter sets and on
//! bits written by hand.

use std::fs::File;

use super::*;
use crate::container;

/// The first sequence parameter set of the test clip `name`.
fn sps(name: &str) -> Sps {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let track = container::read(&mut File::open(path).unwrap()).unwrap();
    let (_, record) = track
        .extensions
        .iter()
        .find(|(kind, _)| kind == b"avcC")
        .unwrap();
    Record::parse(record).unwrap().sps
}

#[test]
fn the_signal_type_of_each_test_clip_is_read() {
    let signal = |full_range, matrix| {
        Some(Signal {
            full_range,
            description: Some([2, 2, matrix]),
        })
    };
    for (name, want) in [
        ("h264-bt709.mp4", signal(false, 1)),
        ("h264-bt601.mp4", signal(false, 6)),
        ("h264-full.mp4", signal(true, 1)),
        ("h264-box-wins.mp4", signal(false, 6)),
        ("h264-untagged-sd.mp4", None),
        ("h264-untagged-hd.mp4", None),
        ("bframes.mp4", None),
    ] {
        assert_eq!(sps(name).signal, want, "{name}");
    }
    assert_eq!(sps("h264-bt601.mp4").chroma_site, Some(1));
    assert_eq!(sps("h264-bt709.mp4").chroma_site, None);
}

/// Bits, most significant first, packed into bytes and padded with zeros.
fn packed(bits: &str) -> Vec<u8> {
    let bits: Vec<u8> = bits
        .bytes()
        .filter(|b| *b != b' ')
        .map(|b| b - b'0')
        .collect();
    bits.chunks(8)
        .map(|byte| (0..8).fold(0u8, |acc, i| (acc << 1) | byte.get(i).copied().unwrap_or(0)))
        .collect()
}

#[test]
fn exp_golomb_numbers_read_as_the_standard_writes_them() {
    // 0, 1, 2, 3, 7 unsigned, then 1, -1, 2 signed.
    let bytes = packed("1 010 011 00100 0001000 010 011 00100");
    let mut bits = Bits::new(&bytes);
    let unsigned: Vec<u32> = (0..5).map(|_| bits.golomb().unwrap()).collect();
    assert_eq!(unsigned, [0, 1, 2, 3, 7]);
    let signed: Vec<i32> = (0..3).map(|_| bits.signed_golomb().unwrap()).collect();
    assert_eq!(signed, [1, -1, 2]);
    // Past the end is none, not a panic.
    let mut bits = Bits::new(&[0, 0]);
    assert_eq!(bits.golomb(), None);
}

#[test]
fn emulation_prevention_bytes_are_dropped() {
    assert_eq!(
        unescape(&[0, 0, 3, 1, 0, 0, 3, 0, 3]),
        [0, 0, 1, 0, 0, 0, 3]
    );
    assert_eq!(unescape(&[0, 3, 0, 0]), [0, 3, 0, 0]);
}

/// A baseline sequence parameter set with a VUI that has `vui`'s bits.
fn baseline(vui: &str) -> Vec<u8> {
    // profile 66, constraints and level, id 0, log2_max_frame_num 0, poc
    // type 2, 1 reference, no gaps, 4 by 2 macroblocks, frames only,
    // direct 8x8, no cropping, then the VUI.
    let mut nal = vec![0x67];
    nal.extend(packed(&format!(
        "01000010 00000000 00011110 1 1 011 010 0 00100 010 1 1 0 1 {vui} 1"
    )));
    nal
}

#[test]
fn a_vui_gives_its_range_matrix_and_chroma_siting() {
    // No aspect ratio or overscan; a signal type (format 5, full range,
    // primaries 1, transfer 13, matrix 5); chroma siting 2 for both fields.
    let sps = Sps::parse(&baseline(
        "0 0 1 101 1 1 00000001 00001101 00000101 1 011 011",
    ))
    .unwrap();
    assert_eq!(
        sps.signal,
        Some(Signal {
            full_range: true,
            description: Some([1, 13, 5]),
        })
    );
    assert_eq!(sps.chroma_site, Some(2));
    // A signal type without a colour description, after an extended
    // aspect ratio.
    let sps = Sps::parse(&baseline(
        "1 11111111 0000000000000001 0000000000000001 0 1 101 0 0 0",
    ))
    .unwrap();
    assert_eq!(
        sps.signal,
        Some(Signal {
            full_range: false,
            description: None,
        })
    );
    // A VUI cut short keeps what it had.
    let mut cut = baseline("0 0 1 101 1 1 00000001");
    cut.truncate(cut.len() - 1);
    assert!(Sps::parse(&cut).is_ok());
}

#[test]
fn a_record_without_what_decoding_needs_says_so() {
    let error = |record: &[u8]| Record::parse(record).unwrap_err().message;
    assert!(error(&[1, 100, 0]).contains("cut-short"));
    assert!(error(&[0, 100, 0, 30, 0xff, 0xe1]).contains("version 0"));
    assert!(error(&[1, 100, 0, 30, 0xfe, 0xe1]).contains("3 bytes"));
    assert!(error(&[1, 100, 0, 30, 0xff, 0xe0, 1]).contains("no sequence parameter set"));
    assert!(error(&[1, 100, 0, 30, 0xff, 0xe1, 0, 9, 0x67]).contains("past the end"));
    assert!(error(&[1, 100, 0, 30, 0xff, 0xe1, 0, 1, 0x68]).contains("not one"));
}

#[test]
fn a_sample_is_disposable_when_none_of_its_slices_is_a_reference() {
    // Length-prefixed NAL units: an SEI (6), then slices (1) with
    // `nal_ref_idc` 0 and 2.
    let unit = |header: u8| [0, 0, 0, 2, header, 0x88];
    let sample = |headers: &[u8]| headers.iter().flat_map(|&h| unit(h)).collect::<Vec<u8>>();
    assert!(disposable(&sample(&[0x06, 0x01, 0x01]), 4));
    assert!(!disposable(&sample(&[0x01, 0x41]), 4));
    // An IDR slice is a reference; a sample without slices is kept.
    assert!(!disposable(&sample(&[0x65]), 4));
    assert!(!disposable(&sample(&[0x06]), 4));
    // Cut short.
    assert!(!disposable(&sample(&[0x01])[..5], 4));
    // Two-byte lengths.
    assert!(disposable(&[0, 2, 0x01, 0x88], 2));
}
