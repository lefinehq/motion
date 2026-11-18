use super::*;

#[test]
fn a_natural_spline_bends_through_its_points() {
    let spline = Spline::new(&[[0.0, 0.0], [0.25, 0.4], [1.0, 1.0]]);
    assert_eq!(spline.at(0.0), 0.0);
    assert!((spline.at(0.25) - 0.4).abs() < 1e-12);
    assert_eq!(spline.at(1.0), 1.0);
    // A straight line through three points stays straight.
    let line = Spline::new(&[[0.0, 0.0], [0.4, 0.4], [1.0, 1.0]]);
    assert!((0..=10).all(|i| (line.at(f64::from(i) / 10.0) - f64::from(i) / 10.0).abs() < 1e-12));
}

#[test]
fn no_points_is_the_straight_line_and_it_is_flat_beyond_its_ends() {
    let straight = Spline::new(&[]);
    assert_eq!(straight.points(), [[0.0, 0.0], [1.0, 1.0]]);
    assert!((straight.at(0.3) - 0.3).abs() < 1e-12);
    let short = Spline::new(&[[0.2, 0.1], [0.8, 0.7]]);
    assert_eq!(short.at(0.0), 0.1);
    assert_eq!(short.at(1.0), 0.7);
}

#[test]
fn more_inputs_between_the_same_ends_give_the_same_spline() {
    let spline = Spline::new(&[[0.0, 0.0], [129.0 / 255.0, 67.0 / 255.0], [1.0, 1.0]]);
    let inputs = [0.0, 41.0, 47.0, 114.0, 129.0, 177.0, 205.0, 255.0].map(|x| x / 255.0);
    let (points, stray) = spline.on(&inputs);
    assert_eq!(points.len(), 8);
    assert!(stray < 1e-12, "{stray}");
}

#[test]
fn an_input_beyond_an_end_takes_the_level_that_keeps_the_curve_closest() {
    let level = |x: f64| x / 255.0;
    let spline = Spline::new(&[
        [level(2.0), 0.0],
        [level(80.0), level(43.0)],
        [level(127.0), level(179.0)],
        [level(194.0), 1.0],
        [1.0, 1.0],
    ]);
    let inputs = [0.0, 2.0, 51.0, 80.0, 98.0, 127.0, 176.0, 194.0, 255.0].map(level);
    let (points, stray) = spline.on(&inputs);
    // The flat level at 0 strays further than the fitted one.
    let flat: Vec<[f64; 2]> = inputs.iter().map(|&x| [x, spline.at(x)]).collect();
    let flat = Spline::new(&flat);
    let flat_stray = (0..=1020)
        .map(|i| f64::from(i) / 1020.0)
        .map(|x| (flat.at(x) - spline.at(x)).abs())
        .fold(0.0, f64::max);
    assert!(stray < flat_stray, "{stray} vs {flat_stray}");
    assert!(stray < level(1.0) && flat_stray > level(2.0));
    assert!(points[0][1] > 0.0);
    assert_eq!(points[1][1], 0.0);
}

#[test]
fn a_far_end_strays_and_says_by_how_much() {
    let spline = Spline::new(&[[0.5, 0.2], [0.55, 1.0]]);
    let (_, stray) = spline.on(&[0.0, 0.5, 0.55, 1.0]);
    assert!(stray > 0.01, "{stray}");
}

#[test]
fn mixing_the_levels_on_shared_inputs_mixes_the_splines() {
    let inputs = [0.0, 0.2, 0.6, 1.0];
    let a = Spline::new(&[[0.0, 0.0], [0.2, 0.5], [0.6, 0.55], [1.0, 1.0]]);
    let b = Spline::new(&[[0.0, 0.1], [0.2, 0.1], [0.6, 0.9], [1.0, 0.8]]);
    let mixed: Vec<[f64; 2]> = a
        .points()
        .iter()
        .zip(b.points())
        .map(|(p, q)| [p[0], 0.25 * p[1] + 0.75 * q[1]])
        .collect();
    let mix = Spline::new(&mixed);
    for x in inputs.iter().chain(&[0.1, 0.33, 0.9]) {
        let want = 0.25 * a.at(*x) + 0.75 * b.at(*x);
        assert!((mix.at(*x) - want).abs() < 1e-12);
    }
}

#[test]
fn a_fitted_level_stays_between_0_and_1() {
    // Rising steeply from its start, the curve would be kept best by a
    // level below 0 at the earlier input.
    let spline = Spline::new(&[[0.3, 0.0], [0.4, 0.9], [1.0, 1.0]]);
    let (points, _) = spline.on(&[0.0, 0.3, 0.4, 1.0]);
    assert!(
        points.iter().all(|p| (0.0..=1.0).contains(&p[1])),
        "{points:?}"
    );
}
