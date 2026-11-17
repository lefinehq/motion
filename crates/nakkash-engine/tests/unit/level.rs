//! Unit tests for `src/level.rs`, through `Session`: a track follows the
//! level of a tone as the mix plays it, within a band, smoothed.

use std::path::PathBuf;

use nakkash_audio::{MIX_RATE, Sound, Speaker};
use nakkash_format::property::Prop;

use crate::Session;

/// A folder of its own for test `name`, holding `tone.wav`: 1 s of a 1 kHz
/// sine at half scale, mono at the mix rate.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nakkash-level-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rate = f64::from(MIX_RATE);
    let tone: Vec<f32> = (0..MIX_RATE)
        .map(|i| (0.5 * (std::f64::consts::TAU * 1000.0 * f64::from(i) / rate).sin()) as f32)
        .collect();
    let sound = Sound::new(MIX_RATE, vec![tone], vec![Speaker::FrontCenter]).unwrap();
    std::fs::write(dir.join("tone.wav"), nakkash_audio::wav::encode(&sound)).unwrap();
    dir
}

/// The opacity of `box` at each of `frames` of the 10 fps scene whose box
/// follows the tone, from 0.5 s, by `anim`.
fn opacity(name: &str, anim: &str, frames: &[f64]) -> Vec<f64> {
    let dir = folder(name);
    let path = dir.join("scene.json");
    let scene = format!(
        r#"{{"nakkash":1,"fps":10,"dur":"2s","layers":[
            {{"id":"t","audio":"tone.wav","offset":"0.5s"}},
            {{"id":"box","rect":"10x10","anim":{{"opacity":"{anim}"}}}}]}}"#
    );
    std::fs::write(&path, scene).unwrap();
    let session = Session::open(&path).unwrap();
    frames
        .iter()
        .map(|&f| {
            let state = session.frame(f).unwrap();
            let node = state.nodes.iter().find(|n| n.id == "box").unwrap();
            node.values[Prop::OPACITY.index()].as_number().unwrap()
        })
        .collect()
}

fn close(got: f64, want: f64, tolerance: f64) {
    assert!((got - want).abs() <= tolerance, "{got} is not {want}");
}

#[test]
fn a_track_follows_the_level_of_the_sound_it_hears() {
    // Silent before 0.5 s; inside, a half-scale sine has level 0.5, and
    // between two frames inside it, too.
    let got = opacity("whole", "0 -> 1 @level t", &[2.0, 10.0, 13.5]);
    close(got[0], 0.0, 1e-9);
    close(got[1], 0.5, 1e-3);
    close(got[2], 0.5, 1e-3);
}

#[test]
fn a_band_hears_only_its_frequencies() {
    // 1 kHz falls between two bins of the frame's window: the Hann window
    // reads it a little low.
    let got = opacity("band", "0 -> 1 @level t 900-1100Hz", &[10.0]);
    close(got[0], 0.5, 0.02);
    let got = opacity("off-band", "0 -> 1 @level t 100-200Hz", &[10.0]);
    close(got[0], 0.0, 0.01);
}

#[test]
fn smoothing_rises_towards_the_level_with_its_time_constant() {
    // The tone starts at frame 5 (its first half frame at 4.5); with a
    // 0.1 s time constant at 10 fps each frame closes 1 - 1/e of the gap.
    let got = opacity(
        "smooth",
        "0 -> 1 @level t smooth 0.1s",
        &[5.0, 6.0, 7.0, 12.0],
    );
    let step = 1.0 - (-1.0_f64).exp();
    assert!(got[0] > 0.0 && got[0] < 0.5 * step + 1e-3, "{got:?}");
    assert!(got[0] < got[1] && got[1] < got[2], "{got:?}");
    close(got[3], 0.5, 0.01);
}
