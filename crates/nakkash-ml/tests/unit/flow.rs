use super::*;

fn field(w: usize, h: usize, dx: f32, dy: f32) -> Field {
    Field {
        dx: Plane::filled(w, h, dx),
        dy: Plane::filled(w, h, dy),
    }
}

#[test]
fn warping_reads_where_the_motion_points() {
    let ramp = Plane {
        width: 4,
        height: 1,
        data: vec![0.0, 1.0, 2.0, 3.0],
    };
    let moved = warp(&ramp, &field(4, 1, 1.0, 0.0));
    assert_eq!(moved.data[..2], [1.0, 2.0]);
    assert!(moved.data[3] > 2.9, "held at the edge");
}

#[test]
fn flows_agree_when_they_undo_each_other() {
    let back = field(8, 8, -2.0, 0.0);
    assert!(consistent(&back, &field(8, 8, 2.0, 0.0)).iter().all(|&v| v));
    assert!(
        consistent(&back, &field(8, 8, -2.0, 0.0))
            .iter()
            .all(|&v| !v)
    );
}

#[test]
fn blending_pulls_small_differences_and_keeps_large_ones() {
    let still = field(2, 1, 0.0, 0.0);
    let previous = Plane {
        width: 2,
        height: 1,
        data: vec![0.5, 0.0],
    };
    let matte = Plane {
        width: 2,
        height: 1,
        data: vec![0.52, 1.0],
    };
    let out = blend(&matte, &previous, &still, &[true, true], 0.6, 0.08);
    assert!(
        (out.data[0] - 0.52).abs() > 0.005 && out.data[0] > 0.5,
        "pulled: {}",
        out.data[0]
    );
    assert!(out.data[1] > 0.99, "a real change stays: {}", out.data[1]);
    let kept = blend(&matte, &previous, &still, &[false, false], 0.6, 0.08);
    assert_eq!(kept, matte);
}
