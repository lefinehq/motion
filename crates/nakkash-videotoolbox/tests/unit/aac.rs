//! Unit tests for `src/aac.rs`.

use super::*;

/// `seconds` of a 440 Hz sine, stereo at 48 kHz.
fn sine(seconds: f64) -> Vec<f32> {
    let frames = (48_000.0 * seconds) as usize;
    (0..frames)
        .flat_map(|i| {
            let v = (0.5 * (std::f64::consts::TAU * 440.0 * i as f64 / 48_000.0).sin()) as f32;
            [v, v]
        })
        .collect()
}

#[test]
fn a_second_of_sound_takes_its_frames_and_the_priming_in_packets() {
    let aac = encode_aac(&sine(1.0), 48_000.0, 2, 128_000).expect("encode");
    assert!(aac.leading > 0 && aac.leading < 4096, "{}", aac.leading);
    let covered = aac.packets.len() as u32 * FRAMES_PER_PACKET;
    assert!(covered >= 48_000 + aac.leading, "{covered} frames");
    assert!(
        covered < 48_000 + aac.leading + 2 * FRAMES_PER_PACKET,
        "{covered} frames"
    );
    assert!(aac.packets.iter().all(|p| !p.is_empty()));
    // No more than 128 kbit a second; a pure tone takes far less.
    let bytes: usize = aac.packets.iter().map(Vec::len).sum();
    assert!((1_000..=16_000).contains(&bytes), "{bytes} bytes");
}

#[test]
fn the_same_sound_encodes_to_the_same_packets() {
    let samples = sine(0.25);
    let once = encode_aac(&samples, 48_000.0, 2, 128_000).expect("encode");
    let twice = encode_aac(&samples, 48_000.0, 2, 128_000).expect("encode");
    assert_eq!(once, twice);
}

#[test]
fn silence_and_no_sound_encode() {
    let silent = encode_aac(&vec![0.0; 9600], 48_000.0, 2, 128_000).expect("encode");
    assert!(!silent.packets.is_empty());
    let empty = encode_aac(&[], 48_000.0, 2, 128_000).expect("encode");
    // The priming alone.
    assert!(empty.packets.len() <= 3, "{}", empty.packets.len());
}
