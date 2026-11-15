//! Unit tests for `src/colour.rs`.

use pretty_assertions::assert_eq;
use proptest::prelude::*;
use rstest::rstest;

use super::*;

/// A `width` by `height` frame of one opaque colour.
fn solid([r, g, b]: [u8; 3], width: u32, height: u32) -> Vec<u8> {
    [r, g, b, 255].repeat((width * height) as usize)
}

/// The published 8-bit limited-range BT.709 values of the primaries.
#[rstest]
#[case::white([255, 255, 255], [235, 128, 128])]
#[case::black([0, 0, 0], [16, 128, 128])]
#[case::red([255, 0, 0], [63, 102, 240])]
#[case::green([0, 255, 0], [173, 42, 26])]
#[case::blue([0, 0, 255], [32, 240, 118])]
fn primaries_take_their_reference_values(#[case] rgb: [u8; 3], #[case] ycbcr: [u8; 3]) {
    let planes = to_planes(&solid(rgb, 2, 2), 2, 2);
    assert_eq!(planes.y, vec![ycbcr[0]; 4]);
    assert_eq!((planes.cb, planes.cr), (vec![ycbcr[1]], vec![ycbcr[2]]));
}

#[test]
fn a_clear_pixel_is_black() {
    let planes = to_planes(&[255, 255, 255, 0], 1, 1);
    assert_eq!(
        (planes.y, planes.cb, planes.cr),
        (vec![16], vec![128], vec![128])
    );
}

#[test]
fn half_alpha_is_drawn_over_black() {
    let half = to_planes(&[255, 255, 255, 128], 1, 1);
    let grey = to_planes(&[128, 128, 128, 255], 1, 1);
    assert_eq!(half, grey);
}

#[test]
fn chroma_halves_and_rounds_up() {
    let planes = to_planes(&solid([0, 0, 0], 5, 3), 5, 3);
    assert_eq!((planes.chroma_width(), planes.chroma_height()), (3, 2));
    assert_eq!((planes.cb.len(), planes.cr.len()), (6, 6));
}

#[test]
fn an_edge_block_averages_the_pixels_it_has() {
    // Three pixels in a row: black, black, red. The second block holds red alone.
    let mut rgba = solid([0, 0, 0], 2, 1);
    rgba.extend_from_slice(&[255, 0, 0, 255]);
    let planes = to_planes(&rgba, 3, 1);
    assert_eq!((planes.cb, planes.cr), (vec![128, 102], vec![128, 240]));
}

#[test]
fn chroma_averages_a_block() {
    // A 2x2 block of two white and two black pixels is mid grey in chroma.
    let rgba = [[255u8, 255, 255, 255], [0, 0, 0, 255]].repeat(2).concat();
    let planes = to_planes(&rgba, 2, 2);
    assert_eq!(planes.y, vec![235, 16, 235, 16]);
    assert_eq!((planes.cb, planes.cr), (vec![128], vec![128]));
}

proptest! {
    #[test]
    fn grey_stays_grey(level: u8) {
        let planes = to_planes(&solid([level; 3], 2, 2), 2, 2);
        prop_assert_eq!((planes.cb[0], planes.cr[0]), (128, 128));
        let y = planes.y[0];
        prop_assert!((16..=235).contains(&y));
    }

    #[test]
    fn luma_rises_with_grey(a: u8, b: u8) {
        let luma = |level| to_planes(&solid([level; 3], 1, 1), 1, 1).y[0];
        let (low, high) = (a.min(b), a.max(b));
        prop_assert!(luma(low) <= luma(high));
    }
}

#[test]
fn ayuv16_keeps_straight_alpha_and_scales_to_16_bits() {
    // Half-clear white keeps its white; a clear pixel keeps its colour.
    let frame = to_ayuv16(&[255, 255, 255, 128, 255, 0, 0, 0], 2, 1);
    assert_eq!(
        frame.samples[..4],
        [128 * 257, 235 * 256, 128 * 256, 128 * 256]
    );
    assert_eq!(frame.samples[4], 0);
    let red = to_planes(&[255, 0, 0, 255], 1, 1);
    let scaled = |v: u8| (f32::from(v) * 256.0) as i32;
    for (sample, eight) in frame.samples[5..]
        .iter()
        .zip([red.y[0], red.cb[0], red.cr[0]])
    {
        // The same level as 8 bits, to within the 8-bit rounding.
        assert!(
            (i32::from(*sample) - scaled(eight)).abs() <= 128,
            "{sample} {eight}"
        );
    }
}

/// The planes pixel by pixel, as `to_planes` once made them: what the
/// banded, one-read pass must match sample for sample.
fn planes_one_by_one(rgba: &[u8], width: u32, height: u32) -> Planes {
    let (w, h) = (width as usize, height as usize);
    let pixel = |x: usize, y: usize| -> [f32; 3] {
        let at = (y * w + x) * 4;
        let alpha = f32::from(rgba[at + 3]) / 255.0;
        [0, 1, 2].map(|c| f32::from(rgba[at + c]) / 255.0 * alpha)
    };
    let mut y_plane = Vec::new();
    for y in 0..h {
        for x in 0..w {
            y_plane.push(quantise(16.0 + 219.0 * luma(pixel(x, y))));
        }
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let (mut cb, mut cr) = (Vec::new(), Vec::new());
    for by in 0..ch {
        for bx in 0..cw {
            let mut sum = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in (by * 2)..(by * 2 + 2).min(h) {
                for x in (bx * 2)..(bx * 2 + 2).min(w) {
                    let p = pixel(x, y);
                    sum = [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]];
                    count += 1.0;
                }
            }
            let (b, r) = chroma(sum.map(|s| s / count));
            cb.push(quantise(128.0 + 224.0 * b));
            cr.push(quantise(128.0 + 224.0 * r));
        }
    }
    Planes {
        width,
        height,
        y: y_plane,
        cb,
        cr,
    }
}

proptest! {
    #[test]
    fn bands_give_the_samples_a_pixel_by_pixel_pass_gives(
        width in 1u32..40,
        height in 1u32..90,
        seed: u64,
    ) {
        let mut state = seed | 1;
        let rgba: Vec<u8> = (0..width * height * 4)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        prop_assert_eq!(to_planes(&rgba, width, height), planes_one_by_one(&rgba, width, height));
    }
}

#[test]
fn a_tall_frame_split_into_bands_matches_too() {
    // Enough block rows for several bands.
    let (w, h) = (7, 301);
    let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i * 31 % 253) as u8).collect();
    assert_eq!(to_planes(&rgba, w, h), planes_one_by_one(&rgba, w, h));
}
