//! Unit tests for `src/picture.rs`.

use super::*;
use proptest::prelude::*;
use rstest::rstest;

const HALVES: &[u8] = include_bytes!("../fixtures/halves.jpg");
const HALVES_GREY: &[u8] = include_bytes!("../fixtures/halves-grey.jpg");

/// A 2x1 image: opaque red, then white at half alpha.
fn pair() -> Image {
    Image::new(2, 1, vec![255, 0, 0, 255, 255, 255, 255, 128]).unwrap()
}

/// The premultiplied pixel at `(x, y)`.
fn premultiplied(picture: &Picture, x: usize, y: usize) -> [u8; 4] {
    let pixel = picture.levels.full.data()[y * picture.width() as usize + x];
    [pixel.r, pixel.g, pixel.b, pixel.a]
}

#[test]
fn a_png_keeps_its_size_and_alpha_premultiplied() {
    let picture = Picture::decode(&pair().to_png().unwrap()).unwrap();
    assert_eq!((picture.width(), picture.height()), (2, 1));
    assert_eq!(premultiplied(&picture, 0, 0), [255, 0, 0, 255]);
    assert_eq!(premultiplied(&picture, 1, 0), [128, 128, 128, 128]);
}

#[rstest]
#[case::colour(HALVES, [220, 30, 30], [30, 60, 220])]
#[case::grey(HALVES_GREY, [87, 87, 87], [69, 69, 69])]
fn a_jpeg_is_opaque_rgb_whatever_its_colour_model(
    #[case] bytes: &[u8],
    #[case] left: [u8; 3],
    #[case] right: [u8; 3],
) {
    let picture = Picture::decode(bytes).unwrap();
    assert_eq!((picture.width(), picture.height()), (16, 8));
    // JPEG is lossy: each half lands within a few levels of its colour.
    for (x, want) in [(2, left), (13, right)] {
        let [r, g, b, a] = premultiplied(&picture, x, 4);
        assert_eq!(a, 255);
        for (got, want) in [r, g, b].into_iter().zip(want) {
            assert!(got.abs_diff(want) <= 6, "x {x}: {got} vs {want}");
        }
    }
}

#[rstest]
#[case::empty(b"")]
#[case::text(b"not a picture")]
#[case::gif(b"GIF89a\x01\x00\x01\x00")]
fn bytes_of_no_format_are_told_the_formats(#[case] bytes: &[u8]) {
    let e = Picture::decode(bytes).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert_eq!(e.expected.as_deref(), Some("a PNG or JPEG file"));
}

#[rstest]
#[case::png(pair().to_png().unwrap(), "a PNG")]
#[case::jpeg(HALVES.to_vec(), "a JPEG")]
fn a_cut_file_is_its_decoders_error(#[case] whole: Vec<u8>, #[case] expected: &str) {
    let cut = &whole[..whole.len() / 2];
    let e = Picture::decode(cut).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert_eq!(e.expected.as_deref(), Some(expected));
}

#[rstest]
#[case::empty(0, 0)]
#[case::too_wide(65_536, 1)]
fn a_picture_the_rasteriser_cannot_hold_is_out_of_range(#[case] width: u32, #[case] height: u32) {
    let image = Image::new(width, height, vec![0; width as usize * height as usize * 4]).unwrap();
    let e = Picture::from_image(&image).unwrap_err();
    assert_eq!(e.kind, Kind::OutOfRange);
    assert_eq!(e.expected.as_deref(), Some("1 to 65535 pixels a side"));
}

/// A `side` by `side` checkerboard of single black and white pixels.
fn checker(side: u32) -> Picture {
    let pixels = (0..side * side)
        .flat_map(|i| match (i % side + i / side) % 2 {
            0 => [0, 0, 0, 255],
            _ => [255, 255, 255, 255],
        })
        .collect();
    Picture::from_image(&Image::new(side, side, pixels).unwrap()).unwrap()
}

#[test]
fn each_half_averages_the_two_by_two_under_it_down_to_one_pixel() {
    // Opaque red beside transparent, then two greys: 4x2 halves to 2x1, 1x1.
    let image = Image::new(
        4,
        2,
        [
            [255, 0, 0, 255],
            [0, 0, 0, 0],
            [100, 100, 100, 255],
            [200, 200, 200, 255],
        ]
        .repeat(2)
        .concat(),
    )
    .unwrap();
    let picture = Picture::from_image(&image).unwrap();
    let halves = halves(&picture.levels.full);
    let sizes: Vec<(u16, u16)> = halves.iter().map(|p| (p.width(), p.height())).collect();
    assert_eq!(sizes, [(2, 1), (1, 1)]);
    let rgba = |p: &Pixmap, i: usize| {
        let c = p.data()[i];
        [c.r, c.g, c.b, c.a]
    };
    // Premultiplied: the transparent pixel adds alpha 0 and no colour.
    assert_eq!(rgba(&halves[0], 0), [128, 0, 0, 128]);
    assert_eq!(rgba(&halves[0], 1), [150, 150, 150, 255]);
    assert_eq!(rgba(&halves[1], 0), [139, 75, 75, 192]);
}

#[rstest]
#[case::full_size(1.0, 64)]
#[case::just_over_half(0.6, 64)]
#[case::under_half(0.4, 32)]
#[case::a_tenth(0.1, 8)]
#[case::past_the_last_level(1e-9, 1)]
#[case::degenerate(0.0, 64)]
fn a_picture_drawn_small_is_sampled_from_the_copy_its_size_needs(
    #[case] step: f64,
    #[case] side: u32,
) {
    let (_, width, height) = checker(64).paint(step);
    assert_eq!((width, height), (side, side));
}

#[test]
fn a_fine_pattern_drawn_small_averages_instead_of_aliasing() {
    use crate::{Frame, ImageDraw, Node, RenderOptions, render_with};
    use vello_cpu::kurbo::Rect;
    let frame = Frame {
        nodes: vec![Node::Image(ImageDraw {
            picture: checker(64),
            rect: Rect::new(0.0, 0.0, 8.0, 8.0),
        })],
        ..Frame::default()
    };
    let image = render_with(&frame, 8, 8, RenderOptions::GOLDEN).unwrap();
    for pixel in image.pixels().chunks_exact(4) {
        assert!(pixel[0].abs_diff(128) <= 2, "{pixel:?}");
    }
}

#[test]
fn pictures_are_equal_by_their_pixels_and_debug_by_their_size() {
    let a = Picture::from_image(&pair()).unwrap();
    let b = Picture::from_image(&pair()).unwrap();
    let other = Picture::from_image(&Image::new(2, 1, vec![0; 8]).unwrap()).unwrap();
    assert_eq!(a, a.clone());
    assert_eq!(a, b);
    assert_ne!(a, other);
    assert_eq!(format!("{a:?}"), "Picture(2x1)");
}

proptest! {
    #[test]
    fn decoding_never_panics(
        format in prop::sample::select(vec![&b""[..], b"\x89PNG\r\n\x1a\n", &[0xFF, 0xD8, 0xFF]]),
        rest in prop::collection::vec(any::<u8>(), 0..256),
    ) {
        let bytes = [format, &rest[..]].concat();
        let _ = Picture::decode(&bytes);
    }
}
