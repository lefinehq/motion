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

#[test]
fn from_png_rejects_other_formats() {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[1, 2, 3]).unwrap();
    writer.finish().unwrap();
    let e = Image::from_png(&bytes).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert_eq!(e.expected.as_deref(), Some("an 8-bit RGBA PNG"));
    assert_eq!(
        Image::from_png(b"not a png").unwrap_err().kind,
        Kind::Invalid
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
