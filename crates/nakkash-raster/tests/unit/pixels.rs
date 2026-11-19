//! Unit tests for `src/pixels.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::blend::BlendMode;

/// A 4 by 2 grid whose pixel at `x`, `y` is opaque grey `(x + 4y) / 8`.
fn ramp() -> Pixels {
    let mut pixels = Pixels::transparent(4, 2);
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        let v = i as f32 / 8.0;
        *pixel = [v, v, v, 1.0];
    }
    pixels
}

#[test]
fn pixels_go_through_a_pixmap_at_eight_bits_kept_premultiplied() {
    let mut pixels = ramp();
    // A colour channel above its alpha is clamped to it.
    pixels.data_mut()[0] = [0.9, 0.2, 0.0, 0.5];
    let back = Pixels::from_pixmap(&pixels.to_pixmap());
    assert_eq!(
        back.data()[0],
        [128.0 / 255.0, 51.0 / 255.0, 0.0, 128.0 / 255.0]
    );
    for (a, b) in back.data().iter().zip(ramp().data()).skip(1) {
        assert!(
            a.iter()
                .zip(b)
                .all(|(a, b)| (a - b).abs() <= 0.5 / 255.0 + 1e-6),
            "{a:?} {b:?}"
        );
    }
}

#[test]
fn sampling_blends_the_four_nearest_and_fades_out_past_the_edge() {
    let pixels = ramp();
    // On a pixel's centre, the pixel itself.
    assert_eq!(pixels.sample(1.5, 0.5), [0.125, 0.125, 0.125, 1.0]);
    // Halfway between the centres of four pixels.
    assert_eq!(
        pixels.sample(2.0, 1.0)[0],
        (0.125 + 0.25 + 0.625 + 0.75) / 4.0
    );
    // Half a pixel past the edge, half transparent.
    assert_eq!(pixels.sample(-0.0, 0.5)[3], 0.5);
    assert_eq!(pixels.get(-1, 0), [0.0; 4]);
}

#[test]
fn a_transform_moves_the_pixels_and_leaves_the_rest_clear() {
    let moved = ramp().transformed(Affine::translate((1.0, 0.0)));
    assert_eq!(moved.data()[0], [0.0; 4]);
    assert_eq!(moved.data()[1], ramp().data()[0]);
    assert_eq!(moved.data()[7], ramp().data()[6]);
}

#[test]
fn a_transform_that_flattens_the_pixels_leaves_them_clear() {
    for flat in [Affine::scale_non_uniform(1.5, 0.0), Affine::scale(0.0)] {
        let moved = ramp().transformed(flat);
        assert!(moved.data().iter().all(|p| *p == [0.0; 4]), "{flat:?}");
    }
}

#[test]
fn a_pixel_that_is_not_a_number_is_left_out_when_laid() {
    let mut canvas = ramp();
    let mut source = Pixels::transparent(canvas.width(), canvas.height());
    source.data_mut()[0] = [f32::NAN; 4];
    source.data_mut()[1] = [0.5, 0.5, 0.5, f32::NAN];
    source.data_mut()[2] = [f32::NAN, 0.25, 0.25, 0.5];
    canvas.lay(&source, BlendMode::Normal, |_| 1.0);
    assert_eq!(canvas.data()[0], ramp().data()[0]);
    assert_eq!(canvas.data()[1], ramp().data()[1]);
    assert!(
        canvas.data()[2].iter().all(|c| c.is_finite()),
        "{:?}",
        canvas.data()[2]
    );
    // An amount that is not a number lays nothing either.
    let mut canvas = ramp();
    let mut solid = Pixels::transparent(canvas.width(), canvas.height());
    solid.data_mut().fill([0.5, 0.5, 0.5, 1.0]);
    canvas.lay(&solid, BlendMode::Normal, |_| f32::NAN);
    assert_eq!(canvas.data(), ramp().data());
}

#[test]
fn mixing_moves_each_pixel_by_its_own_share() {
    let mut pixels = Pixels::transparent(2, 1);
    let mut white = Pixels::transparent(2, 1);
    white.data_mut().fill([1.0; 4]);
    pixels.mix(&white, |i| [0.25, 2.0][i]);
    assert_eq!(pixels.data(), [[0.25; 4], [1.0; 4]]);
}

#[test]
fn a_dithered_gradient_breaks_its_steps_and_keeps_its_level() {
    // A flat level a third of the way between two bytes: plain rounding
    // gives one byte everywhere, the dither a pattern of the two around it
    // whose mean is the level.
    let level = (100.0 + 1.0 / 3.0) / 255.0;
    let mut pixels = Pixels::transparent(16, 16);
    for p in pixels.data_mut() {
        *p = [level, level, level, 1.0];
    }
    let plain = pixels.to_rgba8();
    assert!(plain.chunks_exact(4).all(|p| p[0] == 100));
    let dithered = pixels.to_rgba8_dithered();
    let reds: Vec<u8> = dithered.chunks_exact(4).map(|p| p[0]).collect();
    assert!(reds.iter().all(|r| (100..=101).contains(r)), "{reds:?}");
    let mean = reds.iter().map(|r| f64::from(*r)).sum::<f64>() / reds.len() as f64;
    assert!((mean - (100.0 + 1.0 / 3.0)).abs() < 0.1, "{mean}");
    // The same pixels give the same bytes, and alpha is not dithered.
    assert_eq!(dithered, pixels.to_rgba8_dithered());
    assert!(dithered.chunks_exact(4).all(|p| p[3] == 255));
}
