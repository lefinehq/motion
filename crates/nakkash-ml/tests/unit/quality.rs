use super::*;

fn plane(data: &[f32]) -> Plane {
    Plane {
        width: data.len(),
        height: 1,
        data: data.to_vec(),
    }
}

#[test]
fn measures_overlap_and_difference() {
    let a = plane(&[1.0, 1.0, 0.0, 0.0]);
    let b = plane(&[1.0, 0.0, 1.0, 0.0]);
    assert_eq!(iou(&a, &b), 1.0 / 3.0);
    assert_eq!(iou(&plane(&[0.0]), &plane(&[0.2])), 1.0);
    assert!((sad(&a, &b) - 0.002).abs() < 1e-12);
}

#[test]
fn a_matte_that_moves_with_the_truth_has_no_temporal_error() {
    let truth = [plane(&[0.0, 1.0]), plane(&[1.0, 0.0])];
    let off_by_a_constant = [plane(&[0.1, 0.9]), plane(&[0.9, 0.1])];
    assert!((dtssd(&off_by_a_constant, &truth) - 200.0).abs() < 1e-3);
    let lagging = [plane(&[0.0, 1.0]), plane(&[0.0, 1.0])];
    assert!((dtssd(&lagging, &truth) - 1000.0).abs() < 1e-9);
    assert_eq!(dtssd(&truth[..1], &truth[..1]), 0.0);
}
