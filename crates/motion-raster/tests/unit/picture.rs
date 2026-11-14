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
    let pixel = picture.pixels.data()[y * picture.width() as usize + x];
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
