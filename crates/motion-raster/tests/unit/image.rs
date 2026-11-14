//! Unit tests for `src/image.rs`.

use super::*;
use proptest::prelude::*;

fn checker() -> Image {
    let pixels = vec![
        255, 0, 0, 255, /* */ 0, 255, 0, 128, //
        0, 0, 255, 0, /* */ 10, 20, 30, 40,
    ];
    Image::new(2, 2, pixels).unwrap()
}

#[test]
fn new_checks_the_byte_count() {
    let e = Image::new(2, 2, vec![0; 15]).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert_eq!(e.expected.as_deref(), Some("16 bytes: four per pixel"));
    assert!(Image::new(0, 0, Vec::new()).is_ok());
}

#[test]
fn pixels_are_read_by_position() {
    let image = checker();
    assert_eq!((image.width(), image.height()), (2, 2));
    assert_eq!(image.pixel(1, 0), Some([0, 255, 0, 128]));
    assert_eq!(image.pixel(1, 1), Some([10, 20, 30, 40]));
    assert_eq!(image.pixel(2, 0), None);
    assert_eq!(image.pixel(0, 2), None);
    assert_eq!(image.pixels().len(), 16);
}

#[test]
fn png_round_trips_exactly_and_deterministically() {
    let image = checker();
    let bytes = image.to_png().unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(Image::from_png(&bytes).unwrap(), image);
    assert_eq!(image.to_png().unwrap(), bytes);
}

/// A one-pixel PNG of `color` type and `depth` holding `data`.
fn encoded(color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(color);
    encoder.set_depth(depth);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
    bytes
}

#[test]
fn from_png_reads_every_colour_type_as_rgba() {
    use png::{BitDepth, ColorType};
    let cases: [(ColorType, BitDepth, &[u8], [u8; 4]); 5] = [
        (ColorType::Rgb, BitDepth::Eight, &[1, 2, 3], [1, 2, 3, 255]),
        (ColorType::Grayscale, BitDepth::Eight, &[9], [9, 9, 9, 255]),
        (
            ColorType::GrayscaleAlpha,
            BitDepth::Eight,
            &[9, 7],
            [9, 9, 9, 7],
        ),
        (
            ColorType::Rgba,
            BitDepth::Sixteen,
            &[1, 0, 2, 0, 3, 0, 4, 0],
            [1, 2, 3, 4],
        ),
        (
            ColorType::Rgb,
            BitDepth::Sixteen,
            &[200, 9, 100, 9, 50, 9],
            [200, 100, 50, 255],
        ),
    ];
    for (color, depth, data, expected) in cases {
        let image = Image::from_png(&encoded(color, depth, data)).unwrap();
        assert_eq!(image.pixel(0, 0), Some(expected), "{color:?} {depth:?}");
    }
    let e = Image::from_png(b"not a png").unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_deref()),
        (Kind::Invalid, Some("a PNG"))
    );
}

proptest! {
    #[test]
    fn from_png_never_panics_on_corrupted_files(
        flips in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8),
        cut in any::<prop::sample::Index>(),
    ) {
        let mut bytes = checker().to_png().unwrap();
        for (at, byte) in flips {
            let i = at.index(bytes.len());
            bytes[i] = byte;
        }
        let end = cut.index(bytes.len() + 1);
        let _ = Image::from_png(&bytes[..end]);
    }

    #[test]
    fn from_png_never_panics_on_random_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = Image::from_png(&bytes);
    }
}
