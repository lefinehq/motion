//! Unit tests for `src/lib.rs`: each format against an independent
//! decoder, the fold-down to stereo, and the layout for the mix.

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;

/// A file of the test data.
pub(crate) fn data(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data")).join(name)
}

/// The interleaved samples an independent decoder (ffmpeg) gives for the
/// file `name`, kept beside it as raw 32-bit float.
fn reference(name: &str) -> Vec<f32> {
    std::fs::read(data(&format!("{name}.f32")))
        .expect("the reference")
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// `sound`'s samples interleaved, as the reference keeps them.
fn interleaved(sound: &Sound) -> Vec<f32> {
    (0..sound.frames())
        .flat_map(|f| sound.channels().iter().map(move |c| c[f]))
        .collect()
}

#[rstest]
#[case::pcm_16("s16.wav", 44_100, 2, 0.0)]
#[case::pcm_24("s24.wav", 44_100, 2, 0.0)]
#[case::pcm_8_unsigned("u8.wav", 22_050, 1, 0.0)]
#[case::float_32("f32.wav", 44_100, 2, 0.0)]
#[case::float_64("f64.wav", 44_100, 1, 0.0)]
#[case::a_law("alaw.wav", 8_000, 1, 0.0)]
#[case::mu_law("ulaw.wav", 8_000, 1, 0.0)]
#[case::extensible_5_1("surround.wav", 48_000, 6, 0.0)]
#[case::flac_16("s16.flac", 44_100, 2, 0.0)]
#[case::flac_24("s24.flac", 44_100, 2, 0.0)]
// Lossy decoders round differently, far below hearing.
#[case::mp3("s16.mp3", 44_100, 2, 2e-4)]
#[case::vorbis("s16.ogg", 44_100, 2, 1e-5)]
fn each_format_decodes_to_the_samples_an_independent_decoder_gives(
    #[case] name: &str,
    #[case] rate: u32,
    #[case] channels: usize,
    #[case] tolerance: f32,
) {
    let sound = open(&data(name)).expect("decodes");
    assert_eq!((sound.rate(), sound.channels().len()), (rate, channels));
    let ours = interleaved(&sound);
    let theirs = reference(name);
    // Vorbis decoders differ in how much of the last packet they keep.
    let length = match name.ends_with(".ogg") {
        true => ours.len().min(theirs.len()),
        false => theirs.len(),
    };
    assert_eq!(ours.len().min(length), length, "{name}: length");
    let worst = ours[..length]
        .iter()
        .zip(&theirs[..length])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(worst <= tolerance, "{name}: worst difference {worst}");
    assert!(
        theirs.iter().any(|s| s.abs() > 0.1),
        "{name}: the reference is silent"
    );
}

#[test]
fn an_extensible_wav_says_where_its_channels_stand() {
    let sound = open(&data("surround.wav")).expect("decodes");
    assert_eq!(
        sound.speakers(),
        [
            Speaker::FrontLeft,
            Speaker::FrontRight,
            Speaker::FrontCenter,
            Speaker::LowFrequency,
            Speaker::BackLeft,
            Speaker::BackRight
        ]
    );
}

#[test]
fn a_format_without_a_decoder_is_named() {
    let mp4 = b"\0\0\0\x20ftypM4A \0\0\0\0";
    let error = decode(mp4).expect_err("no AAC decoder");
    assert!(error.message.contains("MP4 container (AAC)"), "{error}");
    let error = decode(b"plain text").expect_err("no format");
    assert!(error.message.contains("no sound format"), "{error}");
}

#[test]
fn a_missing_file_is_an_io_error_naming_it() {
    let error = open(&data("absent.wav")).expect_err("missing");
    assert_eq!(error.kind, Kind::Io);
    assert!(error.message.contains("absent.wav"), "{error}");
}

#[test]
fn mono_plays_on_both_sides_at_its_level() {
    let sound = Sound::new(8_000, vec![vec![0.5, -0.25]], vec![Speaker::FrontCenter])
        .expect("a sound")
        .stereo();
    assert_eq!(sound.channels(), [vec![0.5, -0.25], vec![0.5, -0.25]]);
}

#[test]
fn surround_folds_down_by_its_speakers() {
    let speakers = Speaker::default_layout(6);
    // One impulse on each speaker in turn.
    let channels: Vec<Vec<f32>> = (0..6)
        .map(|c| (0..6).map(|f| if f == c { 1.0 } else { 0.0 }).collect())
        .collect();
    let sound = Sound::new(48_000, channels, speakers)
        .expect("a sound")
        .stereo();
    let half = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(sound.channels()[0], [1.0, 0.0, half, 0.0, half, 0.0]);
    assert_eq!(sound.channels()[1], [0.0, 1.0, half, 0.0, 0.0, half]);
}

#[test]
fn a_sound_for_the_mix_is_stereo_at_the_mix_rate_and_as_long() {
    let sound = open(&data("s16.wav")).expect("decodes");
    let seconds = sound.seconds();
    let mixed = sound.for_mix();
    assert_eq!((mixed.rate(), mixed.channels().len()), (MIX_RATE, 2));
    assert!((mixed.seconds() - seconds).abs() < 1.0 / f64::from(MIX_RATE));
}

#[test]
fn a_sound_is_checked_when_made() {
    assert!(Sound::new(0, vec![vec![]], vec![Speaker::Other]).is_err());
    assert!(Sound::new(1, vec![], vec![]).is_err());
    assert!(
        Sound::new(
            1,
            vec![vec![0.0], vec![]],
            vec![Speaker::FrontLeft, Speaker::FrontRight]
        )
        .is_err()
    );
    assert!(Sound::new(1, vec![vec![0.0]], vec![]).is_err());
}
