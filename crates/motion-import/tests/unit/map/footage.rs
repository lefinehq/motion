//! Unit tests for `src/map/footage.rs`.

use rstest::rstest;

use super::*;
use crate::model::Source;

/// Footage of `width` by `height` lasting `duration` seconds.
fn footage(width: u32, height: u32, duration: f64) -> Footage {
    Footage {
        width,
        height,
        pixel_aspect: 1.0,
        frame_rate: if duration > 0.0 { 25.0 } else { 0.0 },
        duration,
        source: Source::File { path: None },
    }
}

#[rstest]
#[case::found_png(
    "/Users/someone/Footage/a.png",
    640,
    0.0,
    Media::Still("Footage/a.png")
)]
#[case::found_jpeg_any_case(r"C:\work\B.JPEG", 640, 0.0, Media::Still("b.jpeg"))]
#[case::lost_jpg(
    "/old/lost.jpg",
    640,
    0.0,
    Media::Unsupported("image footage it cannot find")
)]
#[case::sequence("/old/seq_0000.jpg", 640, 2.0, Media::Unsupported("image sequences"))]
#[case::video("/old/clip.mov", 640, 8.0, Media::Unsupported("video footage"))]
#[case::folder("/old/frames", 640, 8.0, Media::Unsupported("video footage"))]
#[case::audio("/old/song.wav", 0, 30.0, Media::Unsupported("audio footage"))]
#[case::vector(
    "/old/logo.ai",
    640,
    0.0,
    Media::Unsupported("footage in a format it cannot read")
)]
#[case::no_extension(
    "/old/logo",
    640,
    0.0,
    Media::Unsupported("footage in a format it cannot read")
)]
fn footage_is_a_found_still_or_unsupported_with_why(
    #[case] stored: &str,
    #[case] width: u32,
    #[case] duration: f64,
    #[case] expected: Media<'static>,
) {
    let found = BTreeMap::from([
        (
            "/Users/someone/Footage/a.png".to_owned(),
            "Footage/a.png".to_owned(),
        ),
        (r"C:\work\B.JPEG".to_owned(), "b.jpeg".to_owned()),
        ("/old/clip.mov".to_owned(), "clip.mov".to_owned()),
    ]);
    assert_eq!(
        media(&footage(width, 360, duration), stored, &found),
        expected
    );
}

#[rstest]
#[case("/a/b/c.png", "c.png")]
#[case(r"C:\a\b\c.png", "c.png")]
#[case("c.png", "c.png")]
#[case("/a/b/", "")]
fn the_file_name_is_the_last_part_whatever_the_separator(#[case] stored: &str, #[case] name: &str) {
    assert_eq!(file_name(stored), name);
}
