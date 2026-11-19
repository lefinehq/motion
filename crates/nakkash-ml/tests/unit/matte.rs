use super::*;

/// A plane `w` by `h`, 1 inside the box `[x0, y0, x1, y1)`.
fn square(w: usize, h: usize, [x0, y0, x1, y1]: [usize; 4]) -> Plane {
    let mut p = Plane::filled(w, h, 0.0);
    for y in y0..y1 {
        for x in x0..x1 {
            p.data[y * w + x] = 1.0;
        }
    }
    p
}

fn mask_of(hard: Plane) -> Mask {
    Mask {
        probability: hard.clone(),
        hard,
    }
}

#[test]
fn the_crop_covers_the_object_near_in_time_with_a_margin() {
    // 256² logits for a 512 by 256 clip: the object at x 10..20, y 30..40 on
    // frame 0 and x 100..110 on frame 4, out of reach of frame 1.
    let mut a = Plane::filled(256, 256, -1.0);
    let mut b = a.clone();
    for y in 30..40 {
        for x in 10..20 {
            a.data[y * 256 + x] = 1.0;
        }
        for x in 100..110 {
            b.data[y * 256 + x] = 1.0;
        }
    }
    let empty = Plane::filled(256, 256, -1.0);
    let logits = [a, empty.clone(), empty.clone(), empty.clone(), b];
    // Frame 1 sees frame 0 only: x 20..40 at the clip's scale of 2, y 30..40.
    let [x, y, w, h] = crop(&logits, 1, 512, 256);
    let margin = 0.2 * 20.0 + 16.0;
    assert_eq!([x, y], [(20.0 - margin) as usize, (30.0 - margin) as usize]);
    assert_eq!(
        [x + w, y + h],
        [(40.0 + margin) as usize, (40.0 + margin) as usize]
    );
    // Frame 2 reaches both.
    let [x2, _, w2, _] = crop(&logits, 2, 512, 256);
    assert!(x2 == x && x2 + w2 >= 220);
    // No object near: the whole frame.
    assert_eq!(crop(&[empty], 0, 512, 256), [0, 0, 512, 256]);
}

#[test]
fn the_band_lies_either_side_of_the_edge() {
    let hard = square(128, 128, [32, 32, 96, 96]);
    let band = edge_band(&hard, 12.0);
    let at = |x: usize, y: usize| band[y * 128 + x];
    assert!(at(32, 64) && at(95, 64) && at(64, 32), "on the edge");
    assert!(at(24, 64) && at(40, 64), "within 12 px of it");
    assert!(!at(64, 64), "deep inside");
    assert!(!at(1, 1), "far outside");
}

#[test]
fn refines_only_the_band_and_only_where_the_two_agree() {
    let hard = square(128, 128, [32, 32, 96, 96]);
    // The edge model sees the object a little softer at its left edge.
    let mut soft = hard.clone();
    for y in 0..128 {
        soft.data[y * 128 + 32] = 0.6;
    }
    let (matte, counted) = refine(&soft, &mask_of(hard.clone()), 12.0, 0.9);
    assert!(counted);
    let at = |x: usize, y: usize| matte.data[y * 128 + x];
    assert!(
        (at(32, 64) - 0.6).abs() < 0.05,
        "the band takes the soft edge"
    );
    assert_eq!(at(64, 64), 1.0, "inside stays opaque");
    assert_eq!(at(2, 2), 0.0, "outside stays clear");
    // The edge model picked something else entirely: the segmentation's
    // probability stays.
    let elsewhere = square(128, 128, [0, 0, 8, 8]);
    let (matte, counted) = refine(&elsewhere, &mask_of(hard.clone()), 12.0, 0.9);
    assert!(!counted);
    assert_eq!(matte, hard);
}

#[test]
fn a_mask_upsamples_its_logits() {
    let logits = Plane {
        width: 2,
        height: 1,
        data: vec![-4.0, 4.0],
    };
    let mask = Mask::of(&logits, 4, 1);
    assert_eq!(mask.hard.data, [0.0, 0.0, 1.0, 1.0]);
    assert!(mask.probability.data[0] < 0.05 && mask.probability.data[3] > 0.95);
}
