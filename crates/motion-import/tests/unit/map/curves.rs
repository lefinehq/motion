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
