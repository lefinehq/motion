//! Unit tests for `src/map/curves.rs`.

use pretty_assertions::assert_eq;

use super::*;

/// Curves data whose curves have `points`, in 0–255 levels.
pub(crate) fn data(points: [&[[u16; 2]]; 5]) -> Vec<u8> {
    let mut data = vec![0; POINTS_AT];
    for curve in points {
        let mut bytes = vec![0; CURVE];
        for (i, [x, y]) in curve.iter().enumerate() {
            bytes[i * 4..i * 4 + 2].copy_from_slice(&x.to_be_bytes());
            bytes[i * 4 + 2..i * 4 + 4].copy_from_slice(&y.to_be_bytes());
        }
        bytes[64..68].copy_from_slice(&(curve.len() as u32).to_be_bytes());
        data.extend(bytes);
    }
    data
}

const STRAIGHT: &[[u16; 2]] = &[[0, 0], [255, 255]];

#[test]
fn reads_each_curves_points_in_levels_from_0_to_1() {
    let s_curve: &[[u16; 2]] = &[[0, 0], [64, 40], [191, 215], [255, 255]];
    let curves = read(&data([
        s_curve,
        STRAIGHT,
        STRAIGHT,
        &[[0, 51], [255, 255]],
        STRAIGHT,
    ]))
    .unwrap();
    assert_eq!(
        curves[0],
        vec![
            [0.0, 0.0],
            [64.0 / 255.0, 40.0 / 255.0],
            [191.0 / 255.0, 215.0 / 255.0],
            [1.0, 1.0]
        ]
    );
    assert_eq!(curves[3], vec![[0.0, 0.2], [1.0, 1.0]]);
    assert!(is_straight(&curves[1]));
    assert!(!is_straight(&curves[3]));
    assert_eq!(path(&curves[3]), "M 0,0.2 L 1,1");
}

#[test]
fn data_laid_out_otherwise_reads_as_nothing() {
    let good = data([STRAIGHT; 5]);
    assert!(read(&good[..good.len() - 1]).is_none());
    let mut too_many = good.clone();
    too_many[POINTS_AT + 64..POINTS_AT + 68].copy_from_slice(&17u32.to_be_bytes());
    assert!(read(&too_many).is_none());
    let backwards = data([
        &[[200, 0], [100, 255]],
        STRAIGHT,
        STRAIGHT,
        STRAIGHT,
        STRAIGHT,
    ]);
    assert!(read(&backwards).is_none());
}

/// Levels from 0 to 255 as points from 0 to 1.
fn levels(points: &[[u16; 2]]) -> Points {
    points
        .iter()
        .map(|p| p.map(|c| f64::from(c) / 255.0))
        .collect()
}

#[test]
fn keys_with_other_points_share_the_inputs_of_all_of_them() {
    let keys = [
        levels(&[[0, 0], [129, 67], [255, 255]]),
        levels(&[[0, 0], [47, 33], [177, 184], [255, 255]]),
    ];
    let Shared::Points(shared) = on_shared_inputs(&keys) else {
        panic!("not shared");
    };
    let inputs = |points: &Points| points.iter().map(|p| p[0]).collect::<Vec<_>>();
    assert_eq!(inputs(&shared[0]), inputs(&shared[1]));
    assert_eq!(shared[0].len(), 5);
    // Each key keeps its own curve, at its own points and between them.
    for (key, points) in keys.iter().zip(&shared) {
        let (own, other) = (Spline::new(key), Spline::new(points));
        assert!((0..=255).all(|i| {
            let x = f64::from(i) / 255.0;
            (own.at(x) - other.at(x)).abs() < 1e-9
        }));
    }
}

#[test]
fn straight_keys_need_no_track() {
    let keys = [levels(STRAIGHT), Vec::new()];
    assert_eq!(on_shared_inputs(&keys), Shared::Straight);
}

#[test]
fn a_key_that_starts_a_level_later_keeps_every_level() {
    // The starts differ by two levels: the spline through the earlier
    // start keeps the later key's curve within one level.
    let keys = [
        levels(&[[0, 0], [51, 66], [98, 202], [176, 255], [255, 255]]),
        levels(&[[2, 0], [80, 43], [127, 179], [194, 255], [255, 255]]),
    ];
    assert!(matches!(on_shared_inputs(&keys), Shared::Points(_)));
}

#[test]
fn a_key_that_ends_far_from_the_others_is_reported_with_its_stray() {
    let keys = [
        levels(&[[0, 0], [255, 255]]),
        levels(&[[128, 0], [160, 255]]),
    ];
    match on_shared_inputs(&keys) {
        Shared::Strays(stray) => assert!(stray > 1.0 / 255.0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn more_inputs_than_a_curve_keeps_are_reported() {
    let many = |offset: u16| -> Points {
        levels(
            &(0..10)
                .map(|i| [i * 25 + offset, i * 25 + offset])
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(on_shared_inputs(&[many(0), many(1)]), Shared::TooMany(20));
}
