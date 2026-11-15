//! Unit tests for `src/mix.rs`, through `Session::mix`: layers start on
//! their sample, fade without steps, play slower at a lower pitch, go
//! quiet when hidden, and play inside compositions in their time.

use std::path::{Path, PathBuf};

use motion_audio::{MIX_RATE, Sound, Speaker};

use crate::Session;

/// A folder of its own for test `name`, holding `tone.wav`: 1 s of a
/// 1 kHz sine at half scale, mono at the mix rate, so samples line up.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-mix-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rate = f64::from(MIX_RATE);
    let tone: Vec<f32> = (0..MIX_RATE)
        .map(|i| (0.5 * (std::f64::consts::TAU * 1000.0 * f64::from(i) / rate).sin()) as f32)
        .collect();
    let sound = Sound::new(MIX_RATE, vec![tone], vec![Speaker::FrontCenter]).unwrap();
    std::fs::write(dir.join("tone.wav"), motion_audio::wav::encode(&sound)).unwrap();
    dir
}

/// The left channel of the mix of the 10 fps scene of `layers` in `dir`,
/// over its whole 2 s.
fn mixed(dir: &Path, layers: &str) -> Vec<f32> {
    let path = dir.join("scene.json");
    let scene = format!(r#"{{"motion":1,"fps":10,"dur":"2s","layers":[{layers}]}}"#);
    std::fs::write(&path, scene).unwrap();
    let session = Session::open(&path).unwrap();
    session.mix(0.0..20.0).unwrap().channels()[0].clone()
}

/// The amplitude of `hertz` in `samples`.
fn level(samples: &[f32], hertz: f64) -> f64 {
    let rate = f64::from(MIX_RATE);
    let (mut re, mut im) = (0.0, 0.0);
    for (i, s) in samples.iter().enumerate() {
        let phase = std::f64::consts::TAU * hertz * i as f64 / rate;
        re += f64::from(*s) * phase.cos();
        im += f64::from(*s) * phase.sin();
    }
    2.0 * (re * re + im * im).sqrt() / samples.len() as f64
}

/// Output samples a second.
const RATE: usize = MIX_RATE as usize;

#[test]
fn a_layer_starts_on_its_sample_and_plays_its_file() {
    let dir = folder("start");
    let mix = mixed(&dir, r#"{"id":"t","audio":"tone.wav","offset":"0.5s"}"#);
    assert_eq!(mix.len(), 2 * RATE);
    let start = RATE / 2;
    assert!(mix[..start].iter().all(|s| *s == 0.0));
    let file = motion_audio::open(&dir.join("tone.wav")).unwrap();
    assert_eq!(&mix[start..start + RATE], &file.channels()[0][..]);
    assert!(mix[start + RATE..].iter().all(|s| *s == 0.0));
}

#[test]
fn a_fade_rises_without_steps() {
    let dir = folder("fade");
    let mix = mixed(
        &dir,
        r#"{"id":"t","audio":"tone.wav","anim":{"volume":"0 -> 1 @0-10f"}}"#,
    );
    // A tenth of a second in, the gain is a tenth; at the end, all of it.
    let at = |seconds: f64| level(&mix[(seconds * 48_000.0) as usize..][..480], 1000.0);
    assert!((at(0.1) - 0.05).abs() < 0.005, "{}", at(0.1));
    assert!((at(0.5) - 0.25).abs() < 0.005, "{}", at(0.5));
    assert!((at(0.9) - 0.45).abs() < 0.005, "{}", at(0.9));
}

#[test]
fn a_stretched_layer_plays_slower_and_lower() {
    let dir = folder("stretch");
    let mix = mixed(&dir, r#"{"id":"t","audio":"tone.wav","stretch":2}"#);
    let middle = &mix[RATE / 2..RATE];
    assert!((level(middle, 500.0) - 0.5).abs() < 0.01);
    assert!(level(middle, 1000.0) < 0.01);
}

#[test]
fn a_hidden_layer_is_silent_and_a_composition_plays_in_its_time() {
    let dir = folder("nested");
    let hidden = mixed(&dir, r#"{"id":"t","audio":"tone.wav","hidden":true}"#);
    assert!(hidden.iter().all(|s| *s == 0.0));
    let path = dir.join("scene.json");
    let scene = r#"{"motion":1,"fps":10,"dur":"2s",
      "comps":{"c":{"size":"10x10","dur":"0.5s","layers":[{"id":"t","audio":"tone.wav"}]}},
      "layers":[{"id":"c","comp":"c","offset":"1s","volume":0.5}]}"#;
    std::fs::write(&path, scene).unwrap();
    let mix = Session::open(&path)
        .unwrap()
        .mix(0.0..20.0)
        .unwrap()
        .channels()[0]
        .clone();
    // Silent before the comp layer, the tone at half volume for the
    // composition's half second, silent after.
    assert!(mix[..RATE].iter().all(|s| *s == 0.0));
    assert!((level(&mix[RATE..RATE + RATE / 2], 1000.0) - 0.25).abs() < 0.005);
    assert!(mix[RATE + RATE / 2 + 1..].iter().all(|s| *s == 0.0));
}

#[test]
fn a_missing_file_is_an_error_at_its_layers_audio() {
    let dir = folder("missing");
    let path = dir.join("scene.json");
    std::fs::write(
        &path,
        r#"{"motion":1,"dur":"1s","layers":[{"id":"song","audio":"absent.mp3"}]}"#,
    )
    .unwrap();
    let error = Session::open(&path).unwrap().mix(0.0..1.0).unwrap_err();
    assert!(error.to_string().contains("song.audio"), "{error}");
    assert!(error.to_string().contains("absent.mp3"), "{error}");
}

/// How many pixels of frame `frame` of the 10 fps, 100 by 100 scene of
/// `layers` in `dir`, drawn white on the black frame, are more than half lit.
fn lit(dir: &Path, layers: &str, frame: f64) -> usize {
    let path = dir.join("drawn.json");
    let scene =
        format!(r#"{{"motion":1,"size":"100x100","fps":10,"dur":"2s","layers":[{layers}]}}"#);
    std::fs::write(&path, scene).unwrap();
    let image = Session::open(&path).unwrap().render(frame).unwrap();
    image
        .pixels()
        .chunks_exact(4)
        .filter(|p| p[0] > 128)
        .count()
}

#[test]
fn a_spectrum_hears_the_sound_about_its_layers_time() {
    let dir = folder("spectrum");
    // One band at the tone's 1 kHz, as a bar rising from y = 80 midway along the line, on pixel centres:
    // half scale reaches 20 px, 3 px wide.
    let layers = |offset: &str| {
        format!(
            r##"{{"id":"t","audio":"tone.wav","offset":"0.5s"}},
            {{"id":"bars","rect":"100x100","fill":"#FFFFFF","position":[50,50],
              "fx":[{{"audio-spectrum":40,"audio":"t","start":[45.5,80],"end":[65.5,80],
                      "from":1000,"to":1000,"bands":1,"display":"lines","side":"a",
                      "softness":0,"offset":{offset}}}]}}"##
        )
    };
    // Before the tone starts, nothing draws.
    let quiet = lit(&dir, &layers("0"), 2.0);
    assert_eq!(quiet, 0, "{quiet} px before the tone");
    // Inside it, the bar stands about 20 by 3 px.
    let loud = lit(&dir, &layers("0"), 10.0);
    assert!((55..=80).contains(&loud), "{loud} px inside the tone");
    // A window 0.6 s earlier hears silence again.
    let early = lit(&dir, &layers("-600"), 10.0);
    assert_eq!(early, quiet);
    // One 0.6 s later, from frame 0, hears the tone.
    let late = lit(&dir, &layers("600"), 0.0);
    assert!((55..=80).contains(&late), "{late} px heard ahead");
}
