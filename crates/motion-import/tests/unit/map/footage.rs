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
#[case::clip("/old/clip.mov", 640, 8.0, Media::Clip("clip.mov"))]
#[case::codec("/old/movie.mp4", 640, 8.0, Media::Codec("H.264"))]
#[case::unread("/old/odd.mov", 640, 8.0, Media::Unsupported("video footage"))]
#[case::lost_clip(
    "/old/lost.mov",
    640,
    8.0,
    Media::Unsupported("video footage it cannot find")
)]
#[case::folder(
    "/old/frames",
    640,
    8.0,
    Media::Unsupported("video footage it cannot find")
)]
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
fn footage_is_a_found_still_or_clip_or_unsupported_with_why(
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
        ("/old/movie.mp4".to_owned(), "movie.mp4".to_owned()),
        ("/old/odd.mov".to_owned(), "odd.mov".to_owned()),
    ]);
    let videos = BTreeMap::from([
        (
            "/old/clip.mov".to_owned(),
            video("QuickTime Animation", true),
        ),
        ("/old/movie.mp4".to_owned(), video("H.264", false)),
    ]);
    assert_eq!(
        media(&footage(width, 360, duration), stored, &found, &videos),
        expected
    );
}

fn video(codec: &str, decodes: bool) -> Video {
    Video {
        codec: codec.into(),
        decodes,
    }
}

/// A project whose comp has one layer of an 8-second 640 by 360 clip
/// stored at `/old/clip.mov`, imported with the clip found as `clip.mov`
/// in `codec`.
fn clip_import(codec: &str, decodes: bool) -> crate::map::Import {
    use crate::test_support::build::{comp, layer, project};
    let mut clip = layer(1, "Clip", vec![]);
    clip.source = Some(9);
    let mut project = project(vec![(2, "Main", comp(vec![clip], 8.0))]);
    project.items.push(crate::model::Item {
        id: 9,
        name: "clip.mov".into(),
        folder: None,
        kind: crate::model::ItemKind::Footage(Footage {
            source: Source::File {
                path: Some("/old/clip.mov".into()),
            },
            ..footage(640, 360, 8.0)
        }),
    });
    let options = crate::map::Options {
        footage: BTreeMap::from([("/old/clip.mov".to_owned(), "clip.mov".to_owned())]),
        videos: BTreeMap::from([("/old/clip.mov".to_owned(), video(codec, decodes))]),
        ..crate::map::Options::default()
    };
    crate::map::import(&project, &options).expect("imports")
}

#[test]
fn a_found_clip_with_a_decoder_becomes_a_video_layer_filling_its_size() {
    let import = clip_import("QuickTime Animation", true);
    let layer = &import.json["layers"][0];
    assert_eq!(layer["video"], "clip.mov");
    assert_eq!(layer["box"], serde_json::json!([640, 360]));
    assert!(layer.get("pivot").is_none(), "{layer}");
    assert_eq!(import.mapping.counts["footage: video"].mapped, 1);
    assert!(
        import.mapping.unsupported.is_empty(),
        "{:?}",
        import.mapping.unsupported
    );
}

#[test]
fn a_found_clip_without_a_decoder_is_reported_by_its_codec() {
    let import = clip_import("H.264", false);
    let layer = &import.json["layers"][0];
    assert_eq!(layer["pivot"], true);
    assert!(layer.get("video").is_none(), "{layer}");
    let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
    assert_eq!(what, ["video footage in H.264 (drawn as nothing)"]);
}

#[rstest]
#[case("/a/b/c.png", "c.png")]
#[case(r"C:\a\b\c.png", "c.png")]
#[case("c.png", "c.png")]
#[case("/a/b/", "")]
fn the_file_name_is_the_last_part_whatever_the_separator(#[case] stored: &str, #[case] name: &str) {
    assert_eq!(file_name(stored), name);
}
