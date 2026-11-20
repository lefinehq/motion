//! Unit tests for `src/radial.rs`.

use std::f64::consts::TAU;

use rstest::rstest;
use vello_cpu::kurbo::{Affine, Point};

use super::*;

const ONE: Space = Space {
    transform: Affine::IDENTITY,
    bounds: None,
    input: None,
    time: 0.0,
    sound: None,
    outline: None,
    totals: Vec::new(),
};

/// A smooth, opaque test picture: colours that change over tens of px.
fn smooth(width: u16, height: u16) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    let w = usize::from(width);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = ((i % w) as f32, (i / w) as f32);
        *p = [
            0.5 + 0.4 * (x / 7.0).sin(),
            0.5 + 0.4 * (y / 5.0).cos(),
            0.5 + 0.4 * ((x + y) / 9.0).sin(),
            1.0,
        ];
    }
    pixels
}

/// The mean along the stretch from `near` to `far` of `line(u)`, by
/// `fade`, from many samples: what the running sums stand for.
fn sampled(near: f64, far: f64, fade: Fade, line: impl Fn(f64) -> Pixel) -> [f64; 4] {
    const N: usize = 4_000;
    let (mut sum, mut weight) = ([0.0; 4], 0.0);
    for k in 0..N {
        let s = (k as f64 + 0.5) / N as f64;
        let w = match fade {
            Fade::Flat => 1.0,
            Fade::Linear => 1.0 - s,
        };
        let p = line(near + (far - near) * s);
        for c in 0..4 {
            sum[c] += w * f64::from(p[c]);
        }
        weight += w;
    }
    sum.map(|s| s / weight)
}

/// The radial blur of `pixels` worked out pixel by pixel from many
/// samples along each stretch, beyond the grid nothing.
fn brute(pixels: &Pixels, centre: Point, zoom: bool, stretch: Stretch, fade: Fade) -> Pixels {
    brute_with(pixels, centre, zoom, stretch, fade, Edges::Transparent)
}

/// [`brute`] with nothing or the edge pixels beyond the grid.
fn brute_with(
    pixels: &Pixels,
    centre: Point,
    zoom: bool,
    stretch: Stretch,
    fade: Fade,
    edges: Edges,
) -> Pixels {
    let read = |x: f64, y: f64| match edges {
        Edges::Transparent => pixels.sample(x, y),
        Edges::Repeat => pixels.sample_repeating(x, y),
    };
    let mut out = pixels.clone();
    let w = usize::from(pixels.width());
    for (i, p) in out.data_mut().iter_mut().enumerate() {
        let at = Point::new((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        let (r, a) = (
            at.distance(centre),
            (centre.y - at.y).atan2(at.x - centre.x),
        );
        let line = |u: f64| match zoom {
            true => {
                let u = u.max(0.0);
                read(centre.x + u * a.cos(), centre.y - u * a.sin())
            }
            false => read(centre.x + r * u.cos(), centre.y - r * u.sin()),
        };
        let (near, extent) = match (zoom, stretch) {
            (true, Stretch::Behind(s) | Stretch::Across(s)) => (r, r * s),
            (false, Stretch::Behind(s) | Stretch::Across(s)) => (a, s),
        };
        // A zoom's stretch ends at the centre; the mean weighs each part
        // of the stretch by its length.
        let end = |far: f64| if zoom { far.max(0.0) } else { far };
        let fars = match stretch {
            Stretch::Behind(_) => vec![end(near - extent)],
            Stretch::Across(_) => vec![end(near - extent / 2.0), end(near + extent / 2.0)],
        };
        let (mut sum, mut weight) = ([0.0; 4], 0.0);
        for far in fars {
            let length = (far - near).abs();
            if length < 1e-9 {
                continue;
            }
            let part = sampled(near, far, fade, line);
            for c in 0..4 {
                sum[c] += part[c] * length;
            }
            weight += length;
        }
        let mean: [f64; 4] = match weight > 0.0 {
            true => sum.map(|s| s / weight),
            false => line(near).map(f64::from),
        };
        *p = mean.map(|c| c as f32);
    }
    out
}

/// The mean and largest difference of two grids over every channel.
fn difference(a: &Pixels, b: &Pixels) -> (f32, f32) {
    let (mut sum, mut most, mut n) = (0.0f32, 0.0f32, 0usize);
    for (p, q) in a.data().iter().zip(b.data()) {
        for c in 0..4 {
            let d = (p[c] - q[c]).abs();
            sum += d;
            most = most.max(d);
            n += 1;
        }
    }
    (sum / n as f32, most)
}

#[rstest]
#[case::behind(Stretch::Behind(0.4), Fade::Flat)]
#[case::behind_fading(Stretch::Behind(0.4), Fade::Linear)]
#[case::outwards(Stretch::Behind(-0.3), Fade::Flat)]
#[case::to_the_centre(Stretch::Behind(1.0), Fade::Linear)]
#[case::across(Stretch::Across(0.5), Fade::Flat)]
#[case::across_fading(Stretch::Across(0.5), Fade::Linear)]
fn a_zoom_is_the_mean_along_each_ray(#[case] stretch: Stretch, #[case] fade: Fade) {
    let source = smooth(61, 47);
    let centre = Point::new(27.3, 21.8);
    let mut pixels = source.clone();
    zoom(&mut pixels, centre, stretch, fade, Edges::Transparent);
    let want = brute(&source, centre, true, stretch, fade);
    let (mean, most) = difference(&pixels, &want);
    assert!(mean < 0.006 && most < 0.08, "mean {mean}, most {most}");
}

#[rstest]
#[case::left(Point::new(-40.0, 20.0))]
#[case::below_right(Point::new(90.0, 75.0))]
fn a_zoom_from_outside_the_grid_reads_only_what_it_reaches(#[case] centre: Point) {
    let source = smooth(61, 47);
    let mut pixels = source.clone();
    zoom(
        &mut pixels,
        centre,
        Stretch::Behind(0.3),
        Fade::Linear,
        Edges::Transparent,
    );
    let want = brute(&source, centre, true, Stretch::Behind(0.3), Fade::Linear);
    let (mean, most) = difference(&pixels, &want);
    assert!(mean < 0.006 && most < 0.08, "mean {mean}, most {most}");
}

#[rstest]
#[case::behind(Stretch::Behind(0.8), Fade::Flat)]
#[case::behind_fading(Stretch::Behind(-0.8), Fade::Linear)]
#[case::across(Stretch::Across(1.2), Fade::Flat)]
#[case::past_a_turn(Stretch::Across(8.0), Fade::Linear)]
fn a_spin_is_the_mean_along_each_ring(#[case] stretch: Stretch, #[case] fade: Fade) {
    let source = smooth(61, 47);
    let centre = Point::new(30.0, 25.5);
    let mut pixels = source.clone();
    spin(&mut pixels, centre, stretch, fade, Edges::Transparent);
    let want = brute(&source, centre, false, stretch, fade);
    let (mean, most) = difference(&pixels, &want);
    assert!(mean < 0.01 && most < 0.12, "mean {mean}, most {most}");
}

#[test]
fn a_spin_from_outside_the_grid_reads_only_what_it_reaches() {
    let source = smooth(61, 47);
    let centre = Point::new(-30.0, -10.0);
    let mut pixels = source.clone();
    spin(
        &mut pixels,
        centre,
        Stretch::Behind(0.3),
        Fade::Flat,
        Edges::Transparent,
    );
    let want = brute(&source, centre, false, Stretch::Behind(0.3), Fade::Flat);
    let (mean, most) = difference(&pixels, &want);
    assert!(mean < 0.01 && most < 0.12, "mean {mean}, most {most}");
}

/// A dot 20 px right of the centre streaks outwards, from 20 to 40 px
/// with half the distance behind each pixel, thinning as the stretch
/// grows; with a fade, it fades as well.
#[test]
fn a_dot_streaks_out_from_the_centre() {
    let mut dot = Pixels::transparent(101, 41);
    dot.data_mut()[20 * 101 + 70] = [1.0; 4];
    let centre = Point::new(50.5, 20.5);
    let alpha = |p: &Pixels, x: i64| p.get(x, 20)[3];
    let mut flat = dot.clone();
    zoom(
        &mut flat,
        centre,
        Stretch::Behind(0.5),
        Fade::Flat,
        Edges::Transparent,
    );
    assert!(alpha(&flat, 65) < 1e-6, "{}", alpha(&flat, 65));
    assert!(alpha(&flat, 92) < 1e-6, "{}", alpha(&flat, 92));
    // At 30 px the stretch is 15 px long and holds the dot's one px.
    assert!(
        (alpha(&flat, 80) - 1.0 / 15.0).abs() < 0.01,
        "{}",
        alpha(&flat, 80)
    );
    let mut fading = dot.clone();
    zoom(
        &mut fading,
        centre,
        Stretch::Behind(0.5),
        Fade::Linear,
        Edges::Transparent,
    );
    let along: Vec<f32> = (74..88).map(|x| alpha(&fading, x)).collect();
    assert!(along.windows(2).all(|w| w[1] < w[0]), "{along:?}");
    let mut inwards = dot;
    zoom(
        &mut inwards,
        centre,
        Stretch::Behind(-0.5),
        Fade::Flat,
        Edges::Transparent,
    );
    assert!(alpha(&inwards, 66) > 0.01 && alpha(&inwards, 80) < 1e-6);
}

#[test]
fn a_whole_turn_is_the_mean_of_its_ring() {
    let source = smooth(41, 41);
    let centre = Point::new(20.5, 20.5);
    let mut pixels = source.clone();
    spin(
        &mut pixels,
        centre,
        Stretch::Across(TAU),
        Fade::Flat,
        Edges::Transparent,
    );
    // Two pixels on the same ring, 12 px from the centre.
    let (a, b) = (pixels.get(32, 20), pixels.get(20, 8));
    for c in 0..3 {
        assert!((a[c] - b[c]).abs() < 0.03, "{a:?} {b:?}");
    }
}

#[test]
fn no_amount_changes_nothing() {
    let source = smooth(31, 17);
    let mut pixels = source.clone();
    let values = [
        ParamValue::Number(0.0),
        ParamValue::Point([10.0, 5.0]),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
        ParamValue::Choice(1),
        ParamValue::Choice(1),
    ];
    radial_blur(&mut pixels, &values, &ONE);
    assert_eq!(pixels, source);
}

/// The centre is a point of the layer, which the layer's transform takes
/// to the grid: a zoom about it streaks from where it lands.
#[test]
fn the_centre_moves_with_the_layer() {
    let mut dot = Pixels::transparent(81, 41);
    dot.data_mut()[20 * 81 + 70] = [1.0; 4];
    let values = [
        ParamValue::Number(50.0),
        ParamValue::Point([20.0, 10.0]),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
    ];
    let space = Space {
        transform: Affine::translate((30.5, 10.5)),
        ..ONE
    };
    radial_blur(&mut dot, &values, &space);
    // The centre lands on (50.5, 20.5): the dot, 20 px right, streaks on.
    assert!(dot.get(75, 20)[3] > 0.01 && dot.get(65, 20)[3] < 1e-6);
}

#[rstest]
#[case::across_inside(Point::new(27.3, 21.8), Stretch::Across(0.6), Fade::Flat)]
#[case::behind_inside(Point::new(27.3, 21.8), Stretch::Behind(0.4), Fade::Linear)]
#[case::outwards_inside(Point::new(27.3, 21.8), Stretch::Behind(-0.5), Fade::Flat)]
#[case::across_outside(Point::new(-40.0, 20.0), Stretch::Across(0.8), Fade::Flat)]
#[case::to_the_centre_outside(Point::new(90.0, 75.0), Stretch::Behind(1.0), Fade::Linear)]
#[case::past_the_centre(Point::new(20.0, 10.0), Stretch::Across(3.0), Fade::Flat)]
fn a_zoom_with_repeated_edges_reads_them_wherever_it_reaches(
    #[case] centre: Point,
    #[case] stretch: Stretch,
    #[case] fade: Fade,
) {
    let source = smooth(61, 47);
    let mut pixels = source.clone();
    zoom(&mut pixels, centre, stretch, fade, Edges::Repeat);
    let want = brute_with(&source, centre, true, stretch, fade, Edges::Repeat);
    let (mean, most) = difference(&pixels, &want);
    assert!(mean < 0.006 && most < 0.08, "mean {mean}, most {most}");
}

#[rstest]
#[case::inside(Point::new(30.0, 25.5), Stretch::Across(1.2))]
#[case::outside(Point::new(-30.0, -10.0), Stretch::Behind(0.9))]
fn a_spin_with_repeated_edges_reads_its_whole_rings(
    #[case] centre: Point,
    #[case] stretch: Stretch,
) {
    let source = smooth(61, 47);
    let mut pixels = source.clone();
    spin(&mut pixels, centre, stretch, Fade::Flat, Edges::Repeat);
    let want = brute_with(&source, centre, false, stretch, Fade::Flat, Edges::Repeat);
    let (mean, most) = difference(&pixels, &want);
    assert!(mean < 0.01 && most < 0.12, "mean {mean}, most {most}");
}

/// A flat opaque layer stays flat and opaque to its edges through a zoom
/// whose edges repeat, as the original's spin/zoom blur keeps a flat
/// solid (EffectCraft, case 713); with nothing beyond them it fades.
#[test]
fn repeated_edges_keep_a_flat_layer_flat() {
    let grey = Pixels::made(96, 54, |_| [0.5, 0.5, 0.5, 1.0]);
    let centre = Point::new(60.0, 30.0);
    let mut kept = grey.clone();
    zoom(
        &mut kept,
        centre,
        Stretch::Across(0.2),
        Fade::Flat,
        Edges::Repeat,
    );
    let (mean, most) = difference(&kept, &grey);
    assert!(mean < 1e-6 && most < 1e-5, "mean {mean}, most {most}");
    let mut faded = grey;
    zoom(
        &mut faded,
        centre,
        Stretch::Across(0.2),
        Fade::Flat,
        Edges::Transparent,
    );
    assert!(faded.get(0, 0)[3] < 0.99, "{:?}", faded.get(0, 0));
}

/// A horizontal ramp across a 1,920 px frame zoomed by Amount 20 about its
/// middle, a stretch of a fifth of each pixel's distance, half either
/// side, edges repeated: the original drew 2/255 at x = 2 and 253/255 at
/// x = 1,917 on the middle row (EffectCraft, case 829). The scene draws
/// 3.4 and 251.6, opaque. The ramp does not change down the frame, so
/// three rows draw the middle one as the full frame would.
#[test]
fn a_zoomed_ramp_keeps_its_ends_as_measured() {
    let ramp = Pixels::made(1920, 3, |i| {
        let v = ((i % 1920) as f32 + 0.5) / 1920.0;
        [v, v, v, 1.0]
    });
    let mut pixels = ramp;
    let centre = Point::new(960.0, 1.5);
    zoom(
        &mut pixels,
        centre,
        Stretch::Across(0.2),
        Fade::Flat,
        Edges::Repeat,
    );
    for (x, measured) in [(2, 2.0), (1917, 253.0)] {
        let p = pixels.get(x, 1);
        assert!((p[3] - 1.0).abs() < 1e-5, "{x}: {p:?}");
        let level = p[0] * 255.0;
        assert!(
            (level - measured).abs() < 1.5,
            "{x}: {level} for {measured}"
        );
    }
}
