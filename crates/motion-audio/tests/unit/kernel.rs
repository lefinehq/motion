//! Unit tests for `src/kernel.rs`: resampling keeps a sine's frequency and
//! level, removes what would alias, and repeats itself exactly.

use super::*;
use crate::Sound;
use crate::Speaker;

/// `seconds` of a sine of `hertz` at `rate`, at `level`.
fn sine(rate: u32, hertz: f64, level: f32, seconds: f64) -> Vec<f32> {
    let n = (f64::from(rate) * seconds) as usize;
    (0..n)
        .map(|i| level * (std::f64::consts::TAU * hertz * i as f64 / f64::from(rate)).sin() as f32)
        .collect()
}

/// The level of `hertz` in `samples` at `rate`: the amplitude a single
/// bin of the discrete Fourier transform finds, over the middle half so
/// the edges' silence does not count.
fn level(samples: &[f32], rate: u32, hertz: f64) -> f64 {
    let middle = &samples[samples.len() / 4..samples.len() * 3 / 4];
    let (mut re, mut im) = (0.0, 0.0);
    for (i, s) in middle.iter().enumerate() {
        let phase = std::f64::consts::TAU * hertz * i as f64 / f64::from(rate);
        re += f64::from(*s) * phase.cos();
        im += f64::from(*s) * phase.sin();
    }
    2.0 * (re * re + im * im).sqrt() / middle.len() as f64
}

/// `samples` at `from` resampled to `to`.
fn resampled(samples: Vec<f32>, from: u32, to: u32) -> Vec<f32> {
    Sound::new(from, vec![samples], vec![Speaker::FrontCenter])
        .expect("a sound")
        .resampled(to)
        .channels()[0]
        .clone()
}

#[test]
fn resampling_keeps_a_sines_frequency_and_level() {
    for (from, to) in [
        (44_100, 48_000),
        (96_000, 48_000),
        (22_050, 48_000),
        (48_000, 44_100),
    ] {
        let out = resampled(sine(from, 1000.0, 0.5, 0.2), from, to);
        let found = level(&out, to, 1000.0);
        assert!((found - 0.5).abs() < 0.005, "{from} -> {to}: level {found}");
    }
}

#[test]
fn resampling_down_removes_what_would_fold_back() {
    // 30 kHz cannot be held at 48 kHz: it would fold back to 18 kHz.
    let out = resampled(sine(96_000, 30_000.0, 0.5, 0.2), 96_000, 48_000);
    let folded = level(&out, 48_000, 18_000.0);
    assert!(folded < 0.5 * 1e-3, "folded back at {folded}");
}

#[test]
fn reading_is_the_same_every_time() {
    let samples = sine(44_100, 440.0, 0.8, 0.05);
    let first = resampled(samples.clone(), 44_100, 48_000);
    let second = resampled(samples, 44_100, 48_000);
    assert_eq!(first, second);
}

#[test]
fn reading_on_a_sample_gives_it_back() {
    let kernel = Kernel::standard();
    let samples = sine(48_000, 440.0, 0.8, 0.05);
    for i in [100, 1000, 2000] {
        let read = kernel.read(&samples, i as f64, 1.0);
        // The pass band ends just below Nyquist, so a low tone comes back
        // all but exactly.
        assert!(
            (read - samples[i]).abs() < 1e-3,
            "{i}: {read} vs {}",
            samples[i]
        );
    }
}

#[test]
fn outside_the_samples_is_silence() {
    let kernel = Kernel::standard();
    assert_eq!(kernel.read(&[1.0; 8], -100.0, 1.0), 0.0);
    assert_eq!(kernel.read(&[1.0; 8], 100.0, 1.0), 0.0);
    assert_eq!(kernel.read(&[], 0.0, 1.0), 0.0);
}

#[test]
fn a_rate_change_by_phases_gives_what_reading_gives() {
    let kernel = Kernel::standard();
    let samples = sine(44_100, 997.0, 0.7, 0.02);
    let step = 44_100.0 / 48_000.0;
    let by_phases = kernel.resample(&samples, 44_100, 48_000);
    for (n, sample) in by_phases.iter().enumerate() {
        let read = kernel.read(&samples, n as f64 * step, step);
        assert!((sample - read).abs() < 1e-6, "{n}: {sample} vs {read}");
    }
}
