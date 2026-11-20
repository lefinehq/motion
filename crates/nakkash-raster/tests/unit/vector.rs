//! Unit tests for `src/vector.rs`. No reference render holds a vector
//! blur, so these check the field and the streaks on maps built here.

use vello_cpu::kurbo::{Affine, Rect};

use super::*;
use crate::effect::Input;

/// A `width` by `height` map whose red level is `level(x, y)`.
fn map(width: u16, height: u16, level: impl Fn(f32, f32) -> f32) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    let w = usize::from(width);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let l = level((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
        *p = [l, l, l, 1.0];
    }
    pixels
}

/// The space of a layer reading `map`.
fn reading(map: Pixels) -> Space {
    let bounds = Rect::new(0.0, 0.0, f64::from(map.width()), f64::from(map.height()));
    Space {
        transform: Affine::IDENTITY,
        bounds: Some(bounds),
        input: Some(Input::new(map, Some(bounds))),
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
    }
}

/// The values of `vector-blur`, reading red (choice 1), with no softening.
fn values(amount: f32, angle: f32, streak: u8) -> [ParamValue; 6] {
    [
        ParamValue::Number(amount),
        ParamValue::Number(angle),
        ParamValue::Number(0.0),
        ParamValue::Number(0.0),
        ParamValue::Choice(streak),
        ParamValue::Choice(1),
    ]
}

/// A grid lit at one pixel.
fn dot(width: u16, height: u16, x: usize, y: usize) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    pixels.data_mut()[y * usize::from(width) + x] = [1.0; 4];
    pixels
}

/// Which pixels are lit.
fn lit(pixels: &Pixels, x: i64, y: i64) -> bool {
    pixels.get(x, y)[3] > 1e-4
}

#[test]
fn a_natural_blur_runs_along_the_slope() {
    let space = reading(map(41, 41, |x, _| x / 41.0));
    let mut pixels = dot(41, 41, 20, 20);
    vector_blur(&mut pixels, &values(10.0, 0.0, 0), &space);
    assert!(lit(&pixels, 24, 20) && lit(&pixels, 16, 20));
    assert!(!lit(&pixels, 20, 24) && !lit(&pixels, 20, 16));
    assert!(!lit(&pixels, 27, 20));
}

#[test]
fn a_perpendicular_blur_runs_along_the_contours() {
    let space = reading(map(41, 41, |x, _| x / 41.0));
    let mut pixels = dot(41, 41, 20, 20);
    vector_blur(&mut pixels, &values(10.0, 0.0, 2), &space);
    assert!(lit(&pixels, 20, 24) && lit(&pixels, 20, 16));
    assert!(!lit(&pixels, 24, 20));
}

#[test]
fn an_angle_turns_the_streaks() {
    let space = reading(map(41, 41, |x, _| x / 41.0));
    let mut turned = dot(41, 41, 20, 20);
    vector_blur(&mut turned, &values(10.0, 90.0, 0), &space);
    let mut across = dot(41, 41, 20, 20);
    vector_blur(&mut across, &values(10.0, 0.0, 2), &space);
    for (a, b) in turned.data().iter().zip(across.data()) {
        assert!((a[3] - b[3]).abs() < 1e-4);
    }
}

/// On a map twice as steep on its left as on its right, a natural streak
/// is half as long on the right; one of constant length is not.
#[test]
fn natural_streaks_follow_the_steepness_and_constant_ones_do_not() {
    let level = |x: f32, _: f32| match x < 40.0 {
        true => x / 80.0,
        false => 0.5 + (x - 40.0) / 160.0,
    };
    let space = reading(map(81, 21, level));
    let reach = |streak: u8, x: usize| {
        let mut pixels = dot(81, 21, x, 10);
        vector_blur(&mut pixels, &values(16.0, 0.0, streak), &space);
        (0..10)
            .take_while(|d| lit(&pixels, (x + d) as i64, 10))
            .count()
    };
    let (steep, gentle) = (reach(0, 20), reach(0, 60));
    assert!(steep >= 7 && gentle <= 5, "{steep} {gentle}");
    assert_eq!(reach(1, 20), reach(1, 60));
}

/// A streak behind the pixel takes it back down the slope only, so a dot
/// smears uphill; fading, it fades as it goes.
#[test]
fn a_streak_behind_runs_one_way_and_may_fade() {
    let space = reading(map(41, 21, |x, _| x / 41.0));
    let mut behind = dot(41, 21, 20, 10);
    vector_blur(&mut behind, &values(10.0, 0.0, 3), &space);
    assert!(lit(&behind, 26, 10) && !lit(&behind, 15, 10));
    let mut fading = dot(41, 21, 20, 10);
    vector_blur(&mut fading, &values(10.0, 0.0, 4), &space);
    let along: Vec<f32> = (21..29).map(|x| fading.get(x, 10)[3]).collect();
    assert!(along.windows(2).all(|w| w[1] < w[0]), "{along:?}");
}

#[test]
fn a_flat_map_changes_nothing() {
    let space = reading(map(21, 21, |_, _| 0.5));
    let mut pixels = dot(21, 21, 10, 10);
    let before = pixels.clone();
    vector_blur(&mut pixels, &values(10.0, 0.0, 0), &space);
    assert_eq!(pixels, before);
}

/// Along the ridge of a tent-shaped map the slope points both ways;
/// softening the field there shortens the streaks rather than letting
/// them flip from one side to the other.
#[test]
fn ridge_smoothness_shortens_streaks_along_a_ridge() {
    let space = reading(map(61, 21, |x, _| 1.0 - (x - 30.5).abs() / 30.5));
    let spread = |ridge: f32| {
        // A px off the ridge, where the slope is already full.
        let mut pixels = dot(61, 21, 31, 10);
        let mut v = values(10.0, 0.0, 0);
        v[2] = ParamValue::Number(ridge);
        vector_blur(&mut pixels, &v, &space);
        pixels.get(31, 10)[3]
    };
    assert!(
        spread(20.0) > spread(0.0) * 2.0,
        "{} {}",
        spread(20.0),
        spread(0.0)
    );
}

/// Without a map, the layer is its own: its alpha's slope runs across its
/// edges, so a soft-edged disc smears in and out across its edge.
#[test]
fn without_a_map_the_layer_is_its_own() {
    let mut pixels = map(41, 41, |x, y| {
        let r = (x - 20.5).hypot(y - 20.5);
        (1.0 - (r - 8.0) / 8.0).clamp(0.0, 1.0)
    });
    let before = pixels.clone();
    let space = Space {
        input: None,
        ..reading(Pixels::transparent(1, 1))
    };
    let mut v = values(6.0, 0.0, 0);
    v[5] = ParamValue::Choice(5);
    vector_blur(&mut pixels, &v, &space);
    assert_ne!(pixels, before);
}
