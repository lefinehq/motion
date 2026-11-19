use super::*;

/// A grid of points spread over a 400 px square.
pub(crate) fn grid() -> Vec<[f64; 2]> {
    (0..6)
        .flat_map(|i| {
            (0..6).map(move |j| {
                [
                    100.0 + 80.0 * i as f64,
                    200.0 + 70.0 * j as f64 + 3.0 * i as f64,
                ]
            })
        })
        .collect()
}

fn through(h: &Mat3, points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    points
        .iter()
        .map(|p| linalg::apply(h, *p).unwrap())
        .collect()
}

const PERSPECTIVE: Mat3 = [
    [1.1, 0.05, 30.0],
    [-0.08, 0.95, 12.0],
    [0.0002, -0.0001, 1.0],
];

#[test]
fn recovers_a_similarity() {
    let s = Similarity {
        scale: 1.7,
        angle: 0.4,
        shift: [12.0, -30.0],
    };
    let from = grid();
    let to: Vec<_> = from.iter().map(|p| s.apply(*p)).collect();
    let fitted = Similarity::fit(&from, &to).unwrap();
    assert!((fitted.scale - 1.7).abs() < 1e-12 && (fitted.angle - 0.4).abs() < 1e-12);
    assert!((fitted.shift[0] - 12.0).abs() < 1e-9 && (fitted.shift[1] + 30.0).abs() < 1e-9);
    assert!(Similarity::fit(&from[..1], &to[..1]).is_none());
    let twice = s.after(&s);
    let p = [3.0, 4.0];
    let direct = s.apply(s.apply(p));
    assert!(distance(twice.apply(p), direct) < 1e-9);
}

#[test]
fn recovers_a_homography_from_four_or_many_points() {
    let from = grid();
    let to = through(&PERSPECTIVE, &from);
    // The grid's four corners (its first four points lie on one line), or
    // all of it.
    let corners = [0, 5, 30, 35];
    let pick = |p: &[[f64; 2]]| corners.map(|i| p[i]).to_vec();
    for (a, b) in [(pick(&from), pick(&to)), (from.clone(), to.clone())] {
        let h = homography(&a, &b).unwrap();
        for (p, q) in from.iter().zip(&to) {
            assert!(
                distance(linalg::apply(&h, *p).unwrap(), *q) < 1e-6,
                "{} points",
                a.len()
            );
        }
    }
}

#[test]
fn refuses_collinear_points() {
    let line: Vec<[f64; 2]> = (0..5).map(|i| [i as f64 * 10.0, i as f64 * 5.0]).collect();
    assert!(homography(&line, &line).is_none());
    assert!(homography(&line[..3], &line[..3]).is_none(), "too few");
}

#[test]
fn ransac_ignores_outliers_and_repeats_itself() {
    let from = grid();
    let mut to = through(&PERSPECTIVE, &from);
    for (i, p) in to.iter_mut().enumerate().filter(|(i, _)| i % 4 == 0) {
        *p = [p[0] + 50.0 + i as f64, p[1] - 40.0];
    }
    let (h, inl) = ransac_homography(&from, &to);
    let h = h.unwrap();
    assert_eq!(inl.iter().filter(|&&i| i).count(), from.len() - 9);
    let p = linalg::apply(&h, from[1]).unwrap();
    assert!(distance(p, to[1]) < 1e-6);
    assert_eq!(
        ransac_homography(&from, &to),
        (Some(h), inl),
        "the same on every run"
    );
}

#[test]
fn the_rng_is_seeded_and_unbiased_in_range() {
    let mut a = Rng::new(7);
    let mut b = Rng::new(7);
    let xs: Vec<usize> = (0..100).map(|_| a.below(10)).collect();
    assert_eq!(xs, (0..100).map(|_| b.below(10)).collect::<Vec<_>>());
    assert!(xs.iter().all(|&x| x < 10));
    assert!((0..10).all(|v| xs.contains(&v)));
}

#[test]
fn the_guard_keeps_a_good_homography() {
    let from = grid();
    let to = through(&PERSPECTIVE, &from);
    let corners = [
        [100.0, 200.0],
        [500.0, 215.0],
        [500.0, 565.0],
        [100.0, 550.0],
    ];
    let fit = fit_frame(&from, &to, &corners).unwrap();
    assert!(!fit.guarded);
    assert_eq!((fit.inliers, fit.points), (from.len(), from.len()));
    for (c, got) in corners.iter().zip(&fit.corners) {
        assert!(distance(linalg::apply(&PERSPECTIVE, *c).unwrap(), *got) < 1e-6);
    }
}

#[test]
fn the_guard_falls_back_with_too_few_inliers_or_a_wild_homography() {
    let corners = [
        [100.0, 200.0],
        [500.0, 215.0],
        [500.0, 565.0],
        [100.0, 550.0],
    ];
    // Seven points: never enough inliers.
    let from = &grid()[..7];
    let to = through(&PERSPECTIVE, from);
    let fit = fit_frame(from, &to, &corners).unwrap();
    assert!(fit.guarded);
    assert_eq!(fit.corners, corners.map(|c| fit.similarity.apply(c)));
    // Points bunched in one corner of the object, moved by a strong
    // perspective: the homography is exact for them, but sends the far
    // corners much further than the similarity does.
    let bunched: Vec<[f64; 2]> = (0..9)
        .map(|i| [100.0 + (i % 3) as f64 * 10.0, 200.0 + (i / 3) as f64 * 10.0])
        .collect();
    let wild: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.002, 0.0015, 1.0]];
    let to = through(&wild, &bunched);
    let fit = fit_frame(&bunched, &to, &corners).unwrap();
    assert!(fit.guarded, "{fit:?}");
    assert!(fit_frame(&bunched[..2], &to[..2], &corners).is_none());
}
