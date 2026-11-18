//! Unit tests for `src/effect.rs`.

use super::*;

#[test]
fn every_effect_has_a_unique_name_and_documented_parameters() {
    for spec in EFFECTS {
        assert_eq!(EFFECTS.iter().filter(|s| s.name == spec.name).count(), 1);
        assert!(!spec.doc.is_empty(), "{}", spec.name);
        for param in spec.params {
            assert!(!param.doc.is_empty(), "{}.{}", spec.name, param.name);
            if let (ParamValue::Number(n), Some((lo, hi))) = (param.default, param.range) {
                assert!((lo..=hi).contains(&n), "{}.{}", spec.name, param.name);
            }
        }
    }
}

#[test]
fn every_effect_says_how_far_it_reaches() {
    for spec in EFFECTS {
        let effect = Effect::new(spec.name, &[]).unwrap();
        if let Reach::Within(by) = effect.reach() {
            assert!(by.is_finite() && by >= 0.0, "{}: {by}", spec.name);
        }
    }
    let reach =
        |name: &str, params: &[(&str, ParamValue)]| Effect::new(name, params).unwrap().reach();
    // Four standard deviations of the Gaussian a blurriness gives.
    let Reach::Within(blur) = reach("gaussian-blur", &[("blurriness", ParamValue::Number(20.0))])
    else {
        panic!("a blur reaches a distance");
    };
    assert!((blur - 24.0).abs() < 1e-4);
    assert_eq!(
        reach("drop-shadow", &[("distance", ParamValue::Number(30.0))]),
        Reach::Within(30.0)
    );
    assert_eq!(reach("invert", &[]), Reach::Within(0.0));
    assert_eq!(reach("wrap", &[]), Reach::Bounds);
    assert_eq!(
        reach("displacement-map", &[("edges", ParamValue::Choice(1))]),
        Reach::Bounds
    );
    assert_eq!(reach("particles", &[]), Reach::Anywhere);
    // A warp grows the box by its farthest move each way, and a pixel.
    assert_eq!(
        reach(
            "bezier-warp",
            &[
                ("top-left", ParamValue::Point([-10.0, -5.0])),
                ("bottom-right", ParamValue::Point([20.0, 0.0])),
            ]
        ),
        Reach::Grows(Growth::By([11.0, 6.0, 21.0, 1.0]))
    );
}

#[test]
fn left_out_parameters_take_their_defaults() {
    let fill = Effect::new("fill", &[]).unwrap();
    assert_eq!(fill.spec().name, "fill");
    assert_eq!(
        fill.values(),
        [
            ParamValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
            ParamValue::Number(1.0),
            ParamValue::Choice(0)
        ]
    );
    let blue = Effect::new(
        "fill",
        &[("color", ParamValue::Color(Color::rgba(0.0, 0.0, 1.0, 1.0)))],
    )
    .unwrap();
    assert_eq!(
        blue.values()[0],
        ParamValue::Color(Color::rgba(0.0, 0.0, 1.0, 1.0))
    );
    assert_eq!(blue.values()[1], ParamValue::Number(1.0));
}

#[test]
fn unknown_names_wrong_types_and_ranges_are_reported_at_the_parameter() {
    let e = Effect::new("sparkle", &[]).unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    let names: Vec<&str> = EFFECTS.iter().map(|spec| spec.name).collect();
    assert_eq!(
        e.expected.as_deref(),
        Some(format!("one of {}", names.join(", ")).as_str())
    );

    let e = Effect::new("fill", &[("colour", ParamValue::Number(1.0))]).unwrap_err();
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::UnknownName, Some("colour"))
    );
    assert_eq!(
        e.expected.as_deref(),
        Some("one of color, opacity, inverted")
    );

    let e = Effect::new("fill", &[("color", ParamValue::Number(1.0))]).unwrap_err();
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::WrongType, Some("color"))
    );
    assert_eq!(e.expected.as_deref(), Some("color"));

    let e = Effect::new("fill", &[("opacity", ParamValue::Number(1.5))]).unwrap_err();
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::OutOfRange, Some("opacity"))
    );
    assert_eq!(e.expected.as_deref(), Some("a number from 0 to 1"));
}

#[test]
fn values_know_their_type() {
    assert_eq!(ParamValue::Number(0.0).param_type(), ParamType::Number);
    assert_eq!(
        ParamValue::Color(Color::BLACK).param_type(),
        ParamType::Color
    );
}

#[test]
fn specs_compare_and_print_by_name() {
    let a = Effect::new("fill", &[]).unwrap();
    assert_eq!(a.spec(), &EFFECTS[0]);
    assert_eq!(format!("{:?}", a.spec()), r#"EffectSpec { name: "fill" }"#);
}

/// A 300 by 200 grid of varied, partly transparent colours: several
/// tasks' worth of rows ([`crate::par::TASK`]).
fn textured() -> Pixels {
    let mut pixels = Pixels::transparent(300, 200);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = ((i % 300) as f32, (i / 300) as f32);
        let a = (0.5 + 0.5 * (x * 0.05).sin() * (y * 0.07).cos()).clamp(0.0, 1.0);
        let c = |k: f32| (0.5 + 0.5 * (x * k + y * (0.1 - k)).sin()) * a;
        *p = [c(0.03), c(0.011), c(0.07), a];
    }
    pixels
}

/// Every effect, with its defaults and with each number moved into its
/// range, as `(what, effect)`.
fn every_effect_set() -> Vec<(String, Effect)> {
    let mut all = Vec::new();
    for spec in EFFECTS {
        all.push((spec.name.to_owned(), Effect::new(spec.name, &[]).unwrap()));
        let moved: Vec<(&str, ParamValue)> = spec
            .params
            .iter()
            .filter_map(|p| match (p.default, p.range) {
                (ParamValue::Number(_), Some((lo, hi))) => {
                    let (lo, hi) = (lo.max(-50.0), hi.min(50.0));
                    Some((p.name, ParamValue::Number(lo + (hi - lo) * 0.37)))
                }
                _ => None,
            })
            .collect();
        all.push((
            format!("{} moved", spec.name),
            Effect::new(spec.name, &moved).unwrap(),
        ));
    }
    all
}

#[test]
fn every_effect_gives_the_same_bits_on_one_thread_and_on_many() {
    let space = Space {
        transform: Affine::translate((150.0, 100.0))
            * Affine::rotate(0.3)
            * Affine::scale(0.5)
            * Affine::translate((-120.0, -80.0)),
        bounds: Some(Rect::new(0.0, 0.0, 240.0, 160.0)),
        input: Some(Input {
            pixels: textured(),
            bounds: None,
        }),
        time: 0.75,
        sound: None,
        outline: None,
        totals: Vec::new(),
    };
    let on = |threads: usize, effect: &Effect| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            let mut pixels = textured();
            effect.apply(&mut pixels, &space);
            pixels
        })
    };
    for (what, effect) in every_effect_set() {
        let one = on(1, &effect);
        let many = on(6, &effect);
        let same = one
            .data()
            .iter()
            .zip(many.data())
            .all(|(a, b)| a.map(f32::to_bits) == b.map(f32::to_bits));
        assert!(same, "{what} differs between one thread and six");
    }
}
