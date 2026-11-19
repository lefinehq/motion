use super::*;

/// A 512² picture: grey, with a 4×4 checkerboard of 32 px squares whose
/// top left is at (`x`, `y`).
fn board(x: usize, y: usize) -> Rgb8 {
    let mut data = vec![128u8; 512 * 512 * 3];
    for py in y..y + 128 {
        for px in x..x + 128 {
            let dark = ((px - x) / 32 + (py - y) / 32).is_multiple_of(2);
            let v = if dark { 20 } else { 235 };
            data[(py * 512 + px) * 3..][..3].fill(v);
        }
    }
    Rgb8 {
        width: 512,
        height: 512,
        data,
    }
}

fn everywhere(w: usize, h: usize) -> Mask {
    Mask {
        width: w,
        height: h,
        inside: vec![true; w * h],
    }
}

#[test]
fn chooses_the_inner_corners_of_a_checkerboard() {
    let picture = board(100, 200);
    let chosen = choose(&picture, &everywhere(512, 512));
    // The 3×3 inner crossings are the strongest corners. A 7×7 box window
    // makes each a plateau 6 px wide, and the first of it in raster order
    // is taken, so a point lies within 3 px of its crossing.
    for i in 1..4 {
        for j in 1..4 {
            let crossing = [100.0 + 32.0 * i as f64 - 0.5, 200.0 + 32.0 * j as f64 - 0.5];
            assert!(
                chosen.iter().any(|p| (p[0] - crossing[0]).abs() <= 3.0 && (p[1] - crossing[1]).abs() <= 3.0),
                "no point near {crossing:?} in {chosen:?}"
            );
        }
    }
    // Never two closer than the suppression allows.
    for (i, a) in chosen.iter().enumerate() {
        for b in &chosen[i + 1..] {
            assert!(
                (a[0] - b[0]).abs() > MIN_APART as f64 || (a[1] - b[1]).abs() > MIN_APART as f64
            );
        }
    }
    assert!(chosen.len() <= MAX_POINTS);
    assert_eq!(
        choose(&picture, &everywhere(512, 512)),
        chosen,
        "the same on every run"
    );
}

#[test]
fn chooses_only_inside_the_mask_and_away_from_the_border() {
    let picture = board(100, 200);
    let mut half = everywhere(512, 512);
    half.inside
        .iter_mut()
        .enumerate()
        .for_each(|(k, i)| *i = k % 512 < 164);
    assert!(choose(&picture, &half).iter().all(|p| p[0] < 170.0));
    let none = Mask {
        width: 512,
        height: 512,
        inside: vec![false; 512 * 512],
    };
    assert!(choose(&picture, &none).is_empty());
    // A board against the edge: no point within the border.
    assert!(
        choose(&board(0, 0), &everywhere(512, 512))
            .iter()
            .all(|p| p[0] >= BORDER as f64 - 1.0)
    );
}

#[test]
fn returns_points_in_the_frames_pixels() {
    // The same board on a 1024² frame is found at twice the coordinates.
    let small = board(100, 200);
    let big = small.resized(1024, 1024, Filter::Triangle);
    let a = choose(&small, &everywhere(512, 512));
    let b = choose(&big, &everywhere(1024, 1024));
    assert!(!a.is_empty());
    for p in &a {
        let want = [2.0 * p[0] + 0.5, 2.0 * p[1] + 0.5];
        assert!(
            b.iter()
                .any(|q| (q[0] - want[0]).abs() < 3.0 && (q[1] - want[1]).abs() < 3.0),
            "{p:?}"
        );
    }
}
