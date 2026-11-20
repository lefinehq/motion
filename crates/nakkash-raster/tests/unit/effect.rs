//! Unit tests for `src/effect.rs`.

use super::*;
use crate::oracle::{every_effect_set, textured};

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

#[test]
fn every_effect_gives_the_same_bits_on_one_thread_and_on_many() {
    let space = Space {
        transform: Affine::translate((150.0, 100.0))
            * Affine::rotate(0.3)
            * Affine::scale(0.5)
            * Affine::translate((-120.0, -80.0)),
        bounds: Some(Rect::new(0.0, 0.0, 240.0, 160.0)),
        input: Some(Input::new(textured(), None)),
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

#[test]
fn only_the_effects_that_say_so_draw_their_layers_time() {
    let at = |time: f64, effect: &Effect| {
        let space = Space {
            transform: Affine::translate((30.0, 20.0)),
            bounds: Some(Rect::new(0.0, 0.0, 240.0, 160.0)),
            input: Some(Input::new(textured(), None)),
            time,
            sound: None,
            outline: None,
            totals: Vec::new(),
        };
        let mut pixels = textured();
        effect.apply(&mut pixels, &space);
        pixels
            .data()
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect::<Vec<_>>()
    };
    for (what, effect) in every_effect_set() {
        if effect.reads_time() {
            continue;
        }
        assert!(
            at(0.25, &effect) == at(7.5, &effect),
            "{what} draws its layer's time: list it in READS_TIME"
        );
    }
}

/// Whether the transform effect with `params` on a 960 by 540 layer, laid
/// with its top-left corner at (480, 270) of a 1920 by 1080 frame, covers
/// the centre of each frame pixel as the original draws it.
fn transform_covers(params: &[(&str, ParamValue)], points: &[(u32, u32, bool)]) -> usize {
    let effect = Effect::new("transform", params).unwrap();
    let moved = effect.movement().unwrap().transform.inverse();
    points
        .iter()
        .filter(|&&(x, y, inside)| {
            let at = moved * Point::new(f64::from(x) + 0.5 - 480.0, f64::from(y) + 0.5 - 270.0);
            let covers = (0.0..960.0).contains(&at.x) && (0.0..540.0).contains(&at.y);
            covers != inside
        })
        .count()
}

/// The transform effect's skew shears after the turn, across its axis:
/// three renders of the original of a grey 960 by 540 solid centred in a
/// 1920 by 1080 composition, sampled at 24 pixels each at least 3 px from
/// an edge, as EffectCraft recorded them (`effects/src/distort.rs`, cases
/// 723, 819 and 820). Every pixel lands on the side of the edge it shows.
#[test]
fn the_transform_skew_shears_after_the_turn_as_the_original_draws_it() {
    let point = |x: f32, y: f32| ParamValue::Point([x, y]);
    let n = ParamValue::Number;
    let centred = [
        ("anchor", point(480.0, 270.0)),
        ("position", point(480.0, 270.0)),
    ];
    let case_723 = [
        ("anchor", point(400.0, 200.0)),
        ("position", point(500.0, 300.0)),
        ("scale", point(1.2, 0.8)),
        ("skew", n(10.0)),
        ("skew-axis", n(20.0)),
        ("rotate", n(30.0)),
    ];
    let points_723 = [
        (1465, 954, true),
        (1127, 821, true),
        (1378, 846, true),
        (1416, 878, true),
        (1486, 850, true),
        (492, 467, true),
        (706, 239, true),
        (1402, 1021, true),
        (518, 440, true),
        (1218, 882, true),
        (454, 448, true),
        (1473, 920, true),
        (1048, 654, true),
        (1050, 486, true),
        (1060, 736, true),
        (932, 804, true),
        (1074, 562, true),
        (653, 591, true),
        (1353, 466, false),
        (88, 1021, false),
        (1855, 886, false),
        (1796, 401, false),
        (497, 913, false),
        (1445, 515, false),
    ];
    assert_eq!(transform_covers(&case_723, &points_723), 0);
    let mut case_819 = centred.to_vec();
    case_819.extend([("skew", n(15.0)), ("skew-axis", n(60.0))]);
    let points_819 = [
        (465, 367, true),
        (554, 793, false),
        (481, 529, true),
        (1451, 599, true),
        (428, 575, true),
        (432, 600, true),
        (463, 444, true),
        (759, 803, false),
        (445, 684, true),
        (1411, 757, true),
        (915, 784, false),
        (480, 737, true),
        (758, 620, true),
        (975, 705, true),
        (1289, 376, true),
        (575, 420, true),
        (1015, 507, true),
        (1377, 448, true),
        (1679, 171, false),
        (194, 435, false),
        (413, 159, false),
        (327, 741, false),
        (1824, 138, false),
        (1093, 29, false),
    ];
    assert_eq!(transform_covers(&case_819, &points_819), 0);
    let case_820 = [
        ("anchor", point(100.0, 50.0)),
        ("position", point(480.0, 270.0)),
        ("scale", point(1.3, 0.7)),
        ("skew", n(15.0)),
        ("skew-axis", n(60.0)),
        ("rotate", n(-25.0)),
    ];
    let points_820 = [
        (1674, 566, true),
        (1425, 317, false),
        (1788, 278, true),
        (1530, 390, true),
        (1531, 652, true),
        (1589, 545, true),
        (1890, 441, true),
        (1724, 400, true),
        (1657, 511, true),
        (1895, 261, true),
        (1697, 344, true),
        (933, 840, true),
        (1413, 467, true),
        (1295, 691, true),
        (1353, 548, true),
        (1293, 548, true),
        (1103, 551, true),
        (973, 758, true),
        (446, 484, false),
        (1009, 420, false),
        (144, 761, false),
        (1812, 111, false),
        (1241, 10, false),
        (900, 854, false),
    ];
    assert_eq!(transform_covers(&case_820, &points_820), 0);
    // A slant like a group's, before the turn, misses some of them.
    let wrong = |params: &[(&str, ParamValue)]| {
        let mut skewless = params.to_vec();
        skewless.retain(|(name, _)| !name.starts_with("skew"));
        transform_covers(&skewless, &points_820)
    };
    assert!(wrong(&case_820) > 0);
}
