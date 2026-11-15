//! Unit tests for `src/sound.rs`.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn pcm_rounds_to_16_bits_within_a_step_of_dither() {
    let samples = [0.0, 0.5, -0.5, 1.0, -1.0, 2.0];
    let bytes = pcm16(&samples);
    let levels: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    let exact = [0.0, 16_383.5, -16_383.5, 32_767.0, -32_767.0, 32_767.0];
    for (got, want) in levels.iter().zip(exact) {
        assert!((f64::from(*got) - want).abs() <= 1.5, "{got} for {want}");
    }
    // Full scale and past it clamp.
    assert_eq!(levels[5], 32_767);
}

#[test]
fn dither_is_the_same_every_time_and_averages_out() {
    let quiet = vec![0.25 / 32_767.0; 20_000];
    assert_eq!(pcm16(&quiet), pcm16(&quiet));
    let mean = pcm16(&quiet)
        .chunks_exact(2)
        .map(|b| f64::from(i16::from_le_bytes([b[0], b[1]])))
        .sum::<f64>()
        / 20_000.0;
    // A quarter of a step, kept on average where rounding would lose it.
    assert!((mean - 0.25).abs() < 0.03, "{mean}");
}

#[test]
fn a_mov_file_takes_pcm_frame_by_frame() {
    let track = Soundtrack {
        rate: 48_000,
        channels: vec![vec![0.0, 1.0], vec![-1.0, 0.0]],
    };
    let EncodedSound::Pcm16 {
        data,
        frames,
        channels,
        rate,
    } = encode_sound(&track, Container::Mov).unwrap()
    else {
        panic!("PCM");
    };
    assert_eq!((data.len(), frames, channels, rate), (8, 2, 2, 48_000));
    // Left, then right, then the next frame.
    let first_right = i16::from_le_bytes([data[2], data[3]]);
    assert!(first_right <= -32_766, "{first_right}");
}

#[test]
fn a_sound_of_more_than_two_channels_or_uneven_ones_is_refused() {
    let wide = Soundtrack {
        rate: 48_000,
        channels: vec![vec![0.0]; 3],
    };
    assert!(encode_sound(&wide, Container::Mov).is_err());
    let uneven = Soundtrack {
        rate: 48_000,
        channels: vec![vec![0.0], vec![]],
    };
    assert!(encode_sound(&uneven, Container::Mov).is_err());
}

#[test]
fn aac_takes_only_its_own_rates() {
    assert_eq!(frequency_index(48_000), Some(3));
    assert_eq!(frequency_index(44_100), Some(4));
    assert_eq!(frequency_index(47_000), None);
}

#[cfg(target_os = "macos")]
#[test]
fn an_mp4_file_takes_aac_from_the_system() {
    let track = Soundtrack {
        rate: 48_000,
        channels: vec![vec![0.1; 48_000], vec![-0.1; 48_000]],
    };
    let EncodedSound::Aac {
        packets,
        leading,
        frames,
        bitrate,
        ..
    } = encode_sound(&track, Container::Mp4).unwrap()
    else {
        panic!("AAC");
    };
    assert_eq!((frames, bitrate), (48_000, 192_000));
    assert!(packets.len() as u64 * u64::from(AAC_FRAMES) >= frames + u64::from(leading));
}
