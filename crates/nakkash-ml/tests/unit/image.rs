use super::*;

fn plane(width: usize, height: usize, data: &[f32]) -> Plane {
    Plane {
        width,
        height,
        data: data.to_vec(),
    }
}

#[test]
fn resampling_to_the_same_size_changes_nothing() {
    let src: Vec<f32> = (0..12).map(|i| i as f32).collect();
    for filter in [Filter::Triangle, Filter::Cubic] {
        assert_eq!(resample(&src, (4, 3), 1, (4, 3), filter), src);
    }
}

#[test]
fn growing_with_the_triangle_is_bilinear_between_centres() {
    // Two pixels, 0 and 4, grown to four: centres at 0.25, 0.75, 1.25, 1.75
    // of the source, so 0, 1, 3, 4 with the edges held.
    let out = resample(&[0.0, 4.0], (2, 1), 1, (4, 1), Filter::Triangle);
    assert_eq!(out, [0.0, 1.0, 3.0, 4.0]);
}

#[test]
fn shrinking_averages_every_pixel() {
    // Halving widens the triangle to reach two source pixels each way: the
    // first output (centre 1.0) weighs pixels 0, 1, 2 by 3, 3, 1, the
    // weights normalised where the picture ends.
    let out = resample(&[0.0, 0.0, 8.0, 8.0], (4, 1), 1, (2, 1), Filter::Triangle);
    assert!(
        (out[0] - 8.0 / 7.0).abs() < 1e-6 && (out[1] - 48.0 / 7.0).abs() < 1e-6,
        "{out:?}"
    );
    // A constant stays constant, whatever the filter and scale.
    for filter in [Filter::Triangle, Filter::Cubic] {
        let out = resample(&[5.0; 30], (6, 5), 1, (4, 3), filter);
        assert!(out.iter().all(|v| (v - 5.0).abs() < 1e-5), "{out:?}");
    }
}

#[test]
fn resizes_colour_by_channel_and_rounds() {
    let rgb = Rgb8 {
        width: 2,
        height: 1,
        data: vec![0, 10, 255, 4, 10, 255],
    };
    let grown = rgb.resized(4, 1, Filter::Triangle);
    assert_eq!(grown.data, [0, 10, 255, 1, 10, 255, 3, 10, 255, 4, 10, 255]);
    assert_eq!(rgb.crop([1, 0, 1, 1]).data, [4, 10, 255]);
    let grey = rgb.grey();
    assert!((grey.data[0] - (0.587 * 10.0 + 0.114 * 255.0)).abs() < 1e-3);
}

#[test]
fn rank_filters_take_the_square_around() {
    let p = plane(3, 3, &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(p.max_filter(3).data, vec![1.0; 9]);
    assert_eq!(p.min_filter(3).data, vec![0.0; 9]);
    assert_eq!(p.max_filter(1), p);
}

#[test]
fn the_blur_keeps_the_total_and_spreads_a_point() {
    let mut data = vec![0.0; 21 * 21];
    data[10 * 21 + 10] = 1.0;
    let blurred = plane(21, 21, &data).gaussian_blur(2.0);
    let total: f32 = blurred.data.iter().sum();
    assert!((total - 1.0).abs() < 1e-5, "{total}");
    assert!(blurred.data[10 * 21 + 10] < 0.05 && blurred.data[10 * 21 + 12] > 0.0);
    // Symmetric about the point.
    assert!((blurred.data[10 * 21 + 8] - blurred.data[10 * 21 + 12]).abs() < 1e-7);
}

#[test]
fn the_logistic_never_overflows() {
    assert_eq!(sigmoid(0.0), 0.5);
    assert!(sigmoid(1e9) <= 1.0 && sigmoid(-1e9) >= 0.0);
}
