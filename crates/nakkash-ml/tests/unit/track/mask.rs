use super::*;

/// A mask of `w`×`h` inside where `inside` says.
fn drawn(w: usize, h: usize, inside: impl Fn(f64, f64) -> bool) -> Mask {
    Mask {
        width: w,
        height: h,
        inside: (0..w * h)
            .map(|k| inside((k % w) as f64, (k / w) as f64))
            .collect(),
    }
}

#[test]
fn measures_a_rectangle() {
    let m = drawn(40, 30, |x, y| {
        (10.0..20.0).contains(&x) && (5.0..25.0).contains(&y)
    });
    let s = m.stats(50.0).unwrap();
    assert_eq!(s.centroid, [14.5, 14.5]);
    assert_eq!(s.area, 200.0);
    assert_eq!(s.scale, 2.0);
    // Taller than wide: its long axis is vertical.
    assert!((s.angle.abs() - 90.0).abs() < 1e-9, "{}", s.angle);
    assert_eq!(
        m.corners().unwrap(),
        [[10.0, 5.0], [19.0, 5.0], [19.0, 24.0], [10.0, 24.0]]
    );
    assert!(drawn(4, 4, |_, _| false).stats(1.0).is_none());
    assert!(drawn(4, 4, |_, _| false).corners().is_none());
}

#[test]
fn measures_the_angle_of_a_tilted_bar_clockwise() {
    // A bar along the direction 30° clockwise from +x (y down).
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let m = drawn(200, 200, |x, y| {
        let (dx, dy) = (x - 100.0, y - 100.0);
        let along = dx * c + dy * s;
        let across = -dx * s + dy * c;
        along.abs() < 60.0 && across.abs() < 6.0
    });
    let st = m.stats(0.0).unwrap();
    assert!((st.angle - 30.0).abs() < 0.5, "{}", st.angle);
    assert_eq!(st.scale, 1.0);
}

#[test]
fn reads_logits_at_the_clip_size() {
    // 4×4 logits, positive in the middle two columns.
    let logits = Plane {
        width: 4,
        height: 4,
        data: (0..16)
            .map(|k| if (1..3).contains(&(k % 4)) { 5.0 } else { -5.0 })
            .collect(),
    };
    let m = Mask::from_logits(&logits, 8, 8);
    let s = m.stats(0.0).unwrap();
    assert_eq!(s.centroid[0], 3.5);
    assert!(m.resized(4, 4).inside.iter().filter(|&&i| i).count() >= 8);
}

#[test]
fn cuts_to_a_quadrilateral() {
    let quad = [[2.0, 2.0], [8.0, 2.0], [8.0, 8.0], [2.0, 8.0]];
    assert!(in_polygon([5.0, 5.0], &quad));
    assert!(!in_polygon([9.0, 5.0], &quad));
    let m = drawn(10, 10, |_, _| true).within(&quad);
    assert_eq!(m.inside.iter().filter(|&&i| i).count(), 36);
}
