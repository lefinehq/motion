//! Unit tests for `src/sound.rs`.

use std::sync::Arc;

use vello_cpu::kurbo::Rect;

use super::*;
use crate::style::Color;

use ParamValue::{Choice, Number, Point as At};

const RATE: f64 = 48_000.0;

/// `length` samples of a sine of `hertz` at `level`.
fn sine(hertz: f64, level: f64, length: usize) -> Vec<f32> {
    (0..length)
        .map(|i| (level * (TAU * hertz * i as f64 / RATE).sin()) as f32)
        .collect()
}

/// The sound a window of `samples` makes on both sides.
fn heard(samples: Vec<f32>) -> Arc<Heard> {
    Arc::new(Heard {
        right: samples.clone(),
        left: samples,
        rate: RATE as f32,
    })
}

/// A transparent `width` by `height` layer's space, kept to its box.
fn space(width: f64, height: f64, sound: Option<Arc<Heard>>) -> Space {
    Space {
        transform: Affine::IDENTITY,
        bounds: Some(Rect::new(0.0, 0.0, width, height)),
        input: None,
        time: 0.0,
        sound,
        outline: None,
    }
}

fn white() -> ParamValue {
    ParamValue::Color(Color::WHITE)
}

/// The rows both effects share: a line along `y` from `x0` to `x1`.
fn shared(x0: f32, x1: f32, y: f32, height: f32, thickness: f32) -> Vec<ParamValue> {
    vec![
        At([x0, y]),
        At([x1, y]),
        Number(height),
        Number(90.0),
        Number(0.0),
        Number(thickness),
        Number(0.0),
        white(),
        white(),
    ]
}

/// Spectrum parameters over `shared`: `bands` from `from` to `to`, drawn as
/// `display` on `side`.
fn spectrum_values(
    from: f32,
    to: f32,
    bands: f32,
    display: u8,
    side: u8,
    composite: u8,
) -> Vec<ParamValue> {
    let mut values = shared(0.0, 90.0, 50.0, 40.0, 2.0);
    values.extend([
        Number(from),
        Number(to),
        Number(bands),
        Number(0.0),
        Choice(display),
        Choice(side),
        Choice(0),
        Choice(0),
        Choice(0),
        Choice(composite),
    ]);
    values
}

/// How many pixels of the column at `x` from `y0` to `y1` draw.
fn drawn(pixels: &Pixels, x: i64, y0: i64, y1: i64) -> usize {
    (y0..y1).filter(|&y| pixels.get(x, y)[3] > 0.5).count()
}

#[test]
fn the_fft_matches_the_discrete_fourier_transform() {
    let n = 16;
    let signal: Vec<f64> = (0..n).map(|i| ((i * 7 + 3) % 11) as f64 - 5.0).collect();
    let (mut re, mut im) = (signal.clone(), vec![0.0; n]);
    fft(&mut re, &mut im);
    for k in 0..n {
        let (mut want_re, mut want_im) = (0.0, 0.0);
        for (i, s) in signal.iter().enumerate() {
            let angle = -TAU * (k * i) as f64 / n as f64;
            want_re += s * angle.cos();
            want_im += s * angle.sin();
        }
        assert!(
            (re[k] - want_re).abs() < 1e-9,
            "bin {k}: {} not {want_re}",
            re[k]
        );
        assert!(
            (im[k] - want_im).abs() < 1e-9,
            "bin {k}: {} not {want_im}",
            im[k]
        );
    }
}

#[test]
fn a_full_scale_sine_on_a_bin_reaches_one() {
    // 4096 samples at 48 kHz: bins 11.72 Hz apart, so bin 64 is 750 Hz.
    let (spectrum, size) = amplitudes(&sine(750.0, 1.0, 4096));
    assert_eq!(size, 4096);
    assert!((spectrum[64] - 1.0).abs() < 1e-3, "{}", spectrum[64]);
    assert!(spectrum[200] < 1e-3);
}

#[test]
fn a_sine_lights_its_own_band_only() {
    // Ten bands 100 Hz apart: 400 Hz is the fourth's.
    let bands = bands_of(&sine(400.0, 0.5, 4320), RATE, 100.0, 1_000.0, 10, false);
    let loudest = (0..10).max_by(|&a, &b| bands[a].total_cmp(&bands[b]));
    assert_eq!(loudest, Some(3));
    assert!((bands[3] - 0.5).abs() < 0.05, "{bands:?}");
    for (k, band) in bands.iter().enumerate().filter(|(k, _)| *k != 3) {
        assert!(*band < 0.1, "band {k} is {band}");
    }
}

#[test]
fn bands_run_from_the_first_frequency_to_the_last_either_way() {
    let samples = sine(440.0, 0.5, 4320);
    let up = bands_of(&samples, RATE, 100.0, 1_000.0, 10, false);
    let mut down = bands_of(&samples, RATE, 1_000.0, 100.0, 10, false);
    down.reverse();
    for (a, b) in up.iter().zip(&down) {
        assert!((a - b).abs() < 1e-9);
    }
}

#[test]
fn averaging_keeps_a_steady_sine_as_loud() {
    let samples = sine(1_000.0, 0.5, 8192);
    let plain = bands_of(&samples, RATE, 1_000.0, 1_000.0, 1, false)[0];
    let averaged = bands_of(&samples, RATE, 1_000.0, 1_000.0, 1, true)[0];
    assert!((plain - averaged).abs() < 0.05, "{plain} and {averaged}");
}

#[test]
fn silence_has_no_band() {
    let bands = bands_of(&vec![0.0; 4320], RATE, 20.0, 2_000.0, 8, true);
    assert_eq!(bands, vec![0.0; 8]);
    assert_eq!(bands_of(&[], RATE, 20.0, 2_000.0, 3, false), vec![0.0; 3]);
}

#[test]
fn a_sample_between_two_is_read_linearly() {
    let samples = [0.0, 1.0, -1.0];
    assert_eq!(sample_at(&samples, 0.25), 0.5);
    assert_eq!(sample_at(&samples, 0.75), 0.0);
    assert_eq!(sample_at(&samples, 2.0), -1.0);
    assert_eq!(sample_at(&[], 0.5), 0.0);
}

#[test]
fn a_loud_band_rises_from_the_line_on_its_side() {
    // Ten bands 100 Hz apart along y = 50 from x = 0 to 90, as bars: 440
    // Hz at half scale reaches 20 px from the line, the fourth band, at
    // x = 30.
    let values = spectrum_values(100.0, 1_000.0, 10.0, 1, 1, 0);
    let sound = heard(sine(400.0, 0.5, 4320));
    let mut pixels = Pixels::transparent(100, 100);
    spectrum(
        &mut pixels,
        &values,
        &space(100.0, 100.0, Some(sound.clone())),
    );
    // Side `a` is to the left going right along the line: up the picture.
    let above = drawn(&pixels, 30, 20, 49);
    assert!((17..=21).contains(&above), "{above} px above");
    assert_eq!(drawn(&pixels, 30, 52, 100), 0);
    // A quiet band hardly rises.
    assert!(drawn(&pixels, 80, 20, 47) <= 2);
    // Silence draws nothing at all.
    let mut silent = Pixels::transparent(100, 100);
    spectrum(
        &mut silent,
        &values,
        &space(100.0, 100.0, Some(heard(vec![0.0; 4320]))),
    );
    assert!(silent.data().iter().all(|p| p[3] == 0.0));
    // `both` mirrors it below.
    let values = spectrum_values(100.0, 1_000.0, 10.0, 1, 0, 0);
    let mut pixels = Pixels::transparent(100, 100);
    spectrum(&mut pixels, &values, &space(100.0, 100.0, Some(sound)));
    let below = drawn(&pixels, 30, 51, 80);
    assert!((17..=21).contains(&below), "{below} px below");
}

#[test]
fn digital_bands_are_columns_of_squares() {
    let values = spectrum_values(100.0, 1_000.0, 10.0, 0, 1, 0);
    let mut pixels = Pixels::transparent(100, 100);
    spectrum(
        &mut pixels,
        &values,
        &space(100.0, 100.0, Some(heard(sine(400.0, 0.5, 4320)))),
    );
    // Squares 2 px thick, 2 px apart: about half the column is drawn.
    let column = drawn(&pixels, 30, 25, 50);
    assert!((8..=16).contains(&column), "{column} px");
}

/// `path` measured, as an effect follows it.
fn outline(svg: &str) -> Path {
    Path::Outline(Outline::new(&BezPath::from_svg(svg).unwrap()).unwrap())
}

fn pt([x, y]: [f64; 2]) -> Point {
    Point::new(x, y)
}

fn vec([x, y]: [f64; 2]) -> Vec2 {
    Vec2::new(x, y)
}

/// Whether `a` and `b` are within a thousandth of each other.
fn near(a: (Point, Vec2), b: ([f64; 2], [f64; 2])) -> bool {
    (a.0 - pt(b.0)).hypot() < 1e-3 && (a.1 - vec(b.1)).hypot() < 1e-3
}

#[test]
fn points_along_an_open_outline_run_end_to_end_by_length() {
    // 60 px right then 40 down: 100 px, five points 25 px apart, the
    // first and last on its ends. Left going right is up; going down, it
    // is to the right of the picture.
    let path = outline("M0,0 L60,0 L60,40");
    assert!(!path.closed());
    let want = [
        ([0.0, 0.0], [0.0, -1.0]),
        ([25.0, 0.0], [0.0, -1.0]),
        ([50.0, 0.0], [0.0, -1.0]),
        ([60.0, 15.0], [1.0, 0.0]),
        ([60.0, 40.0], [1.0, 0.0]),
    ];
    for (k, want) in want.into_iter().enumerate() {
        assert!(near(path.at(k, 5), want), "point {k}: {:?}", path.at(k, 5));
    }
    // A single point stands in the middle.
    assert!(near(path.at(0, 1), ([50.0, 0.0], [0.0, -1.0])));
}

#[test]
fn points_round_a_closed_outline_go_once_round_from_its_start() {
    // A 40 px square: four points at its corners, the closing side
    // counted, the last corner not repeated.
    let path = outline("M0,0 L40,0 L40,40 L0,40 Z");
    assert!(path.closed());
    for (k, corner) in [[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]]
        .into_iter()
        .enumerate()
    {
        assert!((path.at(k, 4).0 - pt(corner)).hypot() < 1e-3, "{k}");
    }
    // Round a circle drawn counterclockwise from its top, as a mask's
    // ellipse runs, the points are a quarter apart and left is inward.
    let r = 50.0;
    let k = 0.552_284_75 * r;
    let circle = outline(&format!(
        "M0,{t} C{n},{t} {t},{n} {t},0 C{t},{k} {n},{r} 0,{r} C{k},{r} {r},{k} {r},0 \
         C{r},{n} {k},{t} 0,{t} Z",
        t = -r,
        n = -k,
    ));
    for (j, (at, inward)) in [
        ([0.0, -r], [0.0, 1.0]),
        ([-r, 0.0], [1.0, 0.0]),
        ([0.0, r], [0.0, -1.0]),
        ([r, 0.0], [-1.0, 0.0]),
    ]
    .into_iter()
    .enumerate()
    {
        let (point, out) = circle.at(j, 4);
        assert!((point - pt(at)).hypot() < 0.05, "{j}: {point:?}");
        assert!((out - vec(inward)).hypot() < 1e-3, "{j}: {out:?}");
    }
}

#[test]
fn a_loud_band_rises_from_the_outline_it_follows() {
    // Ten bands up the outline from (50, 90) to (50, 0): the fourth, 440
    // Hz, stands at y = 60 and rises 20 px to the left going up, left
    // of the picture.
    let values = spectrum_values(100.0, 1_000.0, 10.0, 1, 1, 0);
    let mut space = space(100.0, 100.0, Some(heard(sine(400.0, 0.5, 4320))));
    space.outline = Some(Arc::new(BezPath::from_svg("M50,90 L50,0").unwrap()));
    let mut pixels = Pixels::transparent(100, 100);
    spectrum(&mut pixels, &values, &space);
    let row = |x0: i64, x1: i64| (x0..x1).filter(|&x| pixels.get(x, 60)[3] > 0.5).count();
    assert!((17..=21).contains(&row(20, 49)), "{} px", row(20, 49));
    assert_eq!(row(52, 100), 0);
    // The line of `start` and `end` is not used.
    assert_eq!(drawn(&pixels, 30, 20, 49), 0);
}

#[test]
fn without_sound_the_layer_clears_unless_it_composites() {
    let mut pixels = Pixels::transparent(4, 4);
    pixels.data_mut().fill([0.5, 0.5, 0.5, 1.0]);
    let mut kept = pixels.clone();
    spectrum(
        &mut pixels,
        &spectrum_values(20.0, 2_000.0, 8.0, 1, 0, 0),
        &space(4.0, 4.0, None),
    );
    assert!(pixels.data().iter().all(|p| *p == [0.0; 4]));
    spectrum(
        &mut kept,
        &spectrum_values(20.0, 2_000.0, 8.0, 1, 0, 1),
        &space(4.0, 4.0, None),
    );
    assert!(kept.data().iter().all(|p| *p == [0.5, 0.5, 0.5, 1.0]));
}

#[test]
fn the_wave_follows_the_sound() {
    // A window holding one cycle drawn as 9 bars over 0..80: the bar a
    // quarter along rises a full `height` to the left of the line, the bar
    // three quarters along falls as far to the right.
    let mut values = shared(0.0, 80.0, 50.0, 30.0, 2.0);
    values.extend([Number(9.0), Choice(0), Choice(0), Choice(0), Choice(0)]);
    let cycle: Vec<f32> = (0..=400)
        .map(|i| (TAU * i as f64 / 400.0).sin() as f32)
        .collect();
    let mut pixels = Pixels::transparent(100, 100);
    waveform(
        &mut pixels,
        &values,
        &space(100.0, 100.0, Some(heard(cycle))),
    );
    let up = drawn(&pixels, 20, 10, 49);
    let down = drawn(&pixels, 60, 51, 90);
    assert!((27..=31).contains(&up), "{up} px up");
    assert!((27..=31).contains(&down), "{down} px down");
    assert_eq!(drawn(&pixels, 20, 52, 90), 0);
}

#[test]
fn a_level_is_one_for_a_full_scale_sine_and_hears_only_its_band() {
    let samples = sine(750.0, 1.0, 4096);
    assert!((level(&samples, RATE, None) - 1.0).abs() < 1e-3);
    assert!((level(&sine(750.0, 0.25, 4096), RATE, None) - 0.25).abs() < 1e-3);
    // 750 Hz sits on a bin of the 4096-sample window.
    assert!((level(&samples, RATE, Some((700.0, 800.0))) - 1.0).abs() < 1e-3);
    assert!(level(&samples, RATE, Some((100.0, 200.0))) < 1e-3);
    // A band narrower than a bin reads the bin nearest its middle.
    assert!((level(&samples, RATE, Some((749.0, 751.0))) - 1.0).abs() < 1e-3);
    assert_eq!(level(&[], RATE, None), 0.0);
}
