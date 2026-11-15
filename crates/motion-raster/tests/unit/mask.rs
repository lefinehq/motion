//! Unit tests for `src/mask.rs`: combining masks into one coverage.

use vello_cpu::kurbo::{Affine, Rect, Shape};

use super::*;

/// A mask of `mode` over the unit square, every value at its default.
fn mask(mode: MaskMode) -> LayerMask {
    LayerMask {
        path: Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1),
        mode,
        inverted: false,
        opacity: 1.0,
        feather: [0.0; 2],
        expand: 0.0,
    }
}

/// The coverage of `masks` on a one-pixel frame, each mask covering what
/// `covers` gives for it, in order.
fn one_pixel(masks: &[LayerMask], covers: &[f32]) -> Option<f32> {
    let next = std::cell::Cell::new(0);
    coverage(masks, Affine::IDENTITY, 1, 1, |_| {
        let i = next.get();
        next.set(i + 1);
        vec![covers[i]]
    })
    .map(|c| c[0])
}

#[test]
fn each_mode_combines_as_its_formula_says() {
    let cases = [
        (MaskMode::Add, 0.4 + 0.5 - 0.2),
        (MaskMode::Subtract, 0.4 * 0.5),
        (MaskMode::Intersect, 0.2),
        (MaskMode::Lighten, 0.5),
        (MaskMode::Darken, 0.4),
        (MaskMode::Difference, 0.1),
        (MaskMode::None, 0.4),
    ];
    for (mode, want) in cases {
        let got = one_pixel(&[mask(MaskMode::Add), mask(mode)], &[0.4, 0.5]).unwrap();
        assert!((got - want).abs() < 1e-6, "{mode:?}: {got} for {want}");
    }
}

#[test]
fn the_first_mask_that_cuts_decides_where_the_coverage_starts() {
    // Adding starts from nothing; subtracting and intersecting from all.
    assert_eq!(one_pixel(&[mask(MaskMode::Add)], &[0.25]), Some(0.25));
    assert_eq!(one_pixel(&[mask(MaskMode::Subtract)], &[0.25]), Some(0.75));
    assert_eq!(one_pixel(&[mask(MaskMode::Intersect)], &[0.25]), Some(0.25));
    // A mask left out is skipped; only left-out masks cut nothing.
    let none = mask(MaskMode::None);
    assert_eq!(
        one_pixel(&[none.clone(), mask(MaskMode::Subtract)], &[0.25]),
        Some(0.75)
    );
    assert_eq!(one_pixel(&[none], &[]), None);
}

#[test]
fn keeping_the_outside_and_opacity_change_what_a_mask_covers() {
    let outside = LayerMask {
        inverted: true,
        opacity: 0.5,
        ..mask(MaskMode::Add)
    };
    assert_eq!(one_pixel(&[outside], &[0.2]), Some(0.4));
}

#[test]
fn expanding_grows_and_shrinks_by_whole_pixels() {
    let mut line = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
    spread(&mut line, 6, 1, 1.0);
    assert_eq!(line, [0.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
    spread(&mut line, 6, 1, -1.0);
    assert_eq!(line, [0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
    // Coverage reaching the frame's edge is not eaten from there.
    let mut full = vec![1.0; 4];
    spread(&mut full, 2, 2, -1.0);
    assert_eq!(full, [1.0; 4]);
}

#[test]
fn feather_fades_an_edge_about_it() {
    let mut half = vec![0.0f32; 40];
    half[..20].fill(1.0);
    let faded = feathered(half, 40, 1, [10.0 * SIGMA_PER_FEATHER, 0.0]);
    assert!((faded[19] + faded[20] - 1.0).abs() < 0.05);
    assert!(faded[14] > 0.9 && faded[25] < 0.1);
}
