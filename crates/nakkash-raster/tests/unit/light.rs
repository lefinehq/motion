//! Unit tests for `src/light.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::style::Color;

use ParamValue::Number;

const ONE: Space = Space {
    transform: Affine::IDENTITY,
    bounds: None,
    input: None,
    time: 0.0,
    sound: None,
    outline: None,
    totals: Vec::new(),
};

/// A 40 by 40 grid with an opaque mid-grey square from 10 to 30.
fn square() -> Pixels {
    let mut pixels = Pixels::transparent(40, 40);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = (i % 40, i / 40);
        if (10..30).contains(&x) && (10..30).contains(&y) {
            *p = [0.5, 0.5, 0.5, 1.0];
        }
    }
    pixels
}

/// The values of a bevel `thickness` px thick lit from `angle` degrees.
fn bevel_of(thickness: f32, angle: f32, intensity: f32) -> [ParamValue; 4] {
    [
        Number(thickness),
        Number(angle),
        ParamValue::Color(Color::rgba(1.0, 1.0, 1.0, 1.0)),
        Number(intensity),
    ]
}

#[test]
fn depth_runs_in_from_the_outline() {
    let depth = depth(&square());
    let at = |x: usize, y: usize| depth[y * 40 + x];
    assert_eq!(at(5, 5), 0.0);
    assert!((at(10, 20) - 0.5).abs() < 1e-9);
    assert!((at(13, 20) - 3.5).abs() < 1e-9);
    // The corner is as far from both sides as its nearest.
    assert!((at(12, 12) - 2.5).abs() < 1e-9);
}

#[test]
fn a_bevel_lights_the_slope_facing_the_light_and_shades_the_other() {
    // Light from straight up: the top slope brightens, the bottom one
    // darkens, the flat middle and the sides keep their grey.
    let mut pixels = square();
    bevel(&mut pixels, &bevel_of(4.0, 0.0, 1.0), &ONE);
    let grey = |x: i64, y: i64| pixels.get(x, y)[0];
    assert!(grey(20, 11) > 0.9, "{}", grey(20, 11));
    assert!(grey(20, 28) < 0.1, "{}", grey(20, 28));
    assert_eq!(grey(20, 20), 0.5);
    assert!((grey(11, 20) - 0.5).abs() < 1e-6, "{}", grey(11, 20));
    assert_eq!(pixels.get(20, 11)[3], 1.0);
    assert_eq!(pixels.get(5, 5), [0.0; 4]);
}

#[test]
fn a_bevel_without_intensity_or_thickness_changes_nothing() {
    for values in [bevel_of(4.0, 0.0, 0.0), bevel_of(0.0, 0.0, 1.0)] {
        let mut pixels = square();
        bevel(&mut pixels, &values, &ONE);
        assert_eq!(pixels, square());
    }
}
