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
#[case::sound("/old/song.mp3", 0, 30.0, Media::Sound("song.mp3"))]
#[case::sound_format("/old/song.m4a", 0, 30.0, Media::SoundFormat("an MP4 container (AAC)"))]
#[case::unread_sound(
    "/old/song.aif",
    0,
    30.0,
    Media::Unsupported("audio footage in a format it cannot read")
)]
#[case::lost_sound(
    "/old/lost.wav",
    0,
    30.0,
    Media::Unsupported("audio footage it cannot find")
)]
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
fn footage_is_a_found_still_clip_or_sound_or_unsupported_with_why(
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
        ("/old/song.mp3".to_owned(), "song.mp3".to_owned()),
        ("/old/song.m4a".to_owned(), "song.m4a".to_owned()),
        ("/old/song.aif".to_owned(), "song.aif".to_owned()),
    ]);
    let videos = BTreeMap::from([
        (
            "/old/clip.mov".to_owned(),
            video("QuickTime Animation", true),
        ),
        ("/old/movie.mp4".to_owned(), video("H.264", false)),
    ]);
    let sounds = BTreeMap::from([
        ("/old/song.mp3".to_owned(), sound("MP3", true)),
        (
            "/old/song.m4a".to_owned(),
            sound("an MP4 container (AAC)", false),
        ),
    ]);
    let height = if width == 0 { 0 } else { 360 };
    let files = Files {
        found: &found,
        videos: &videos,
        sounds: &sounds,
    };
    assert_eq!(
        media(&footage(width, height, duration), stored, files),
        expected
    );
}

fn sound(format: &str, decodes: bool) -> Sound {
    Sound {
        format: format.into(),
        decodes,
    }
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

mod sounds {
    //! Footage that is a sound: an audio layer, its levels as its volume,
    //! its switch, and the spectrum effects that read it.

    use std::collections::BTreeMap;

    use serde_json::{Value as Json, json};

    use super::{footage, sound};
    use crate::map::Options;
    use crate::model::{
        Child, Ease, Interpolation, Item, ItemKind, Keyframe, Layer, MaskSettings, Path, Project,
        Source, Stream, Value, Vertex,
    };
    use crate::test_support::build::{comp, group, layer, project, property};

    /// A keyframe of the levels `db` on both sides at `time`, linear.
    fn key(time: f64, db: f64) -> Keyframe {
        let ease = vec![Ease {
            speed: 0.0,
            influence: 1.0 / 6.0,
        }];
        Keyframe {
            time,
            value: Value::Vector2([db, db]),
            in_interpolation: Interpolation::Linear,
            out_interpolation: Interpolation::Linear,
            in_ease: ease.clone(),
            out_ease: ease,
            spatial: None,
            continuous: false,
            auto_bezier: false,
            roving: false,
        }
    }

    /// A layer's audio group holding its `levels`.
    fn levels(levels: Stream) -> Child {
        Child::Group(group(
            "ADBE Audio Group",
            vec![Child::Property(property("ADBE Audio Levels", levels))],
        ))
    }

    /// The layer `id` playing the sound item 9, its audio switch on.
    fn playing(id: u32, children: Vec<Child>) -> Layer {
        let mut song = layer(id, "Song", children);
        song.source = Some(9);
        song.switches.audio = true;
        // A sound has no picture to switch on.
        song.switches.enabled = false;
        song
    }

    /// `project` with item 9, a 30 s sound stored at `/old/song.mp3`.
    fn with_song(mut project: Project) -> Project {
        project.items.push(Item {
            id: 9,
            name: "song.mp3".into(),
            folder: None,
            kind: ItemKind::Footage(crate::model::Footage {
                source: Source::File {
                    path: Some("/old/song.mp3".into()),
                },
                ..footage(0, 0, 30.0)
            }),
        });
        project
    }

    /// `project` imported with the song found as `song.mp3`, in MP3.
    fn import(project: &Project) -> crate::map::Import {
        let options = Options {
            footage: BTreeMap::from([("/old/song.mp3".to_owned(), "song.mp3".to_owned())]),
            sounds: BTreeMap::from([("/old/song.mp3".to_owned(), sound("MP3", true))]),
            ..Options::default()
        };
        crate::map::import(project, &options).expect("imports")
    }

    #[test]
    fn a_found_sound_becomes_an_audio_layer_at_its_levels() {
        let song = playing(
            1,
            vec![levels(Stream::Static(Value::Vector2([-6.0, -6.0])))],
        );
        let import = import(&with_song(project(vec![(
            2,
            "Main",
            comp(vec![song], 2.0),
        )])));
        let layer = &import.json["layers"][0];
        assert_eq!(layer["audio"], "song.mp3");
        // -6 dB is a gain of about a half.
        assert_eq!(layer["volume"], json!(0.501187));
        for drawn in ["box", "pivot", "position", "anchor", "hidden"] {
            assert!(layer.get(drawn).is_none(), "{drawn} in {layer}");
        }
        assert_eq!(import.mapping.counts["footage: audio"].mapped, 1);
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
    }

    #[test]
    fn moving_levels_are_keyed_every_frame_as_a_gain() {
        // From silence (-48 dB) to full over 0.5 s, at 10 frames a second.
        let song = playing(
            1,
            vec![levels(Stream::Animated(vec![
                key(0.5, -48.0),
                key(1.0, 0.0),
            ]))],
        );
        let import = import(&with_song(project(vec![(
            2,
            "Main",
            comp(vec![song], 2.0),
        )])));
        let keys = import.json["layers"][0]["anim"]["volume"]
            .as_array()
            .expect("keys")
            .clone();
        let times: Vec<&str> = keys.iter().filter_map(|k| k["t"].as_str()).collect();
        assert_eq!(times, ["0.5s", "0.6s", "0.7s", "0.8s", "0.9s", "1s"]);
        let gains: Vec<f64> = keys.iter().filter_map(|k| k["v"].as_f64()).collect();
        // Linear in decibels: -48, -38.4, ... 0, as gains.
        let want: Vec<f64> = (0..=5)
            .map(|k| 10_f64.powf((-48.0 + 9.6 * f64::from(k)) / 20.0))
            .collect();
        for (got, want) in gains.iter().zip(&want) {
            assert!((got - want).abs() < 1e-6, "{gains:?}");
        }
    }

    #[test]
    fn levels_that_differ_or_reach_past_twelve_decibels_are_reported() {
        let song = playing(
            1,
            vec![levels(Stream::Static(Value::Vector2([18.0, 14.0])))],
        );
        let import = import(&with_song(project(vec![(
            2,
            "Main",
            comp(vec![song], 2.0),
        )])));
        assert_eq!(import.json["layers"][0]["volume"], json!(4));
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert_eq!(
            what,
            [
                "different levels for left and right (both play at their mean)",
                "levels above +12 dB (played at +12 dB)"
            ]
        );
    }

    #[test]
    fn the_audio_switch_mutes_a_sound_and_a_composition_that_plays_one() {
        let mut quiet = playing(1, vec![]);
        quiet.switches.audio = false;
        let inner = playing(5, vec![]);
        let mut muted = layer(3, "Muted", vec![]);
        muted.source = Some(4);
        let mut heard = layer(6, "Heard", vec![]);
        heard.source = Some(4);
        heard.switches.audio = true;
        let project = with_song(project(vec![
            (2, "Main", comp(vec![quiet, muted, heard], 2.0)),
            (4, "Inner", comp(vec![inner], 2.0)),
        ]));
        let import = import(&project);
        let layers = import.json["layers"].as_array().expect("layers");
        let by_id = |id: &str| {
            layers
                .iter()
                .find(|l| l["id"] == id)
                .unwrap_or_else(|| panic!("no {id}"))
        };
        assert_eq!(by_id("song")["hidden"], true);
        assert_eq!(by_id("muted")["volume"], json!(0));
        assert!(by_id("heard").get("volume").is_none());
    }

    #[test]
    fn a_spectrum_reads_the_sound_its_audio_layer_names() {
        let song = playing(1, vec![]);
        let spectrum = Child::Group(group(
            "ADBE Effect Parade",
            vec![Child::Group(group(
                "ADBE AudSpect",
                vec![
                    Child::Property(property(
                        "ADBE AudSpect-0001",
                        Stream::Static(Value::Layer(1)),
                    )),
                    // No mask: the value the original keeps when it
                    // draws along the line.
                    Child::Property(property(
                        "ADBE AudSpect-0004",
                        Stream::Static(Value::Number(0.0)),
                    )),
                    Child::Property(property(
                        "ADBE AudSpect-0020",
                        Stream::Static(Value::Number(2.0)),
                    )),
                    Child::Property(property(
                        "ADBE AudSpect-0021",
                        Stream::Static(Value::Number(3.0)),
                    )),
                ],
            ))],
        ));
        let bars = layer(2, "Bars", vec![spectrum]);
        let import = import(&with_song(project(vec![(
            3,
            "Main",
            comp(vec![song, bars], 2.0),
        )])));
        let bars: &Json = &import.json["layers"]
            .as_array()
            .expect("layers")
            .iter()
            .find(|l| l["id"] == "bars")
            .expect("bars")
            .clone();
        let effect = &bars["fx"][0];
        assert_eq!(effect["audio"], "song", "{bars}");
        assert_eq!(effect["display"], "lines");
        assert_eq!(effect["side"], "both");
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
    }

    /// A layer whose masks are `masks`, each a name and the id its `mkif`
    /// gives it, under a spectrum whose path parameter is `path`.
    fn along(masks: &[(&str, u32)], path: Value) -> crate::map::Import {
        let song = playing(1, vec![]);
        let shape = Path {
            closed: false,
            vertices: [[0.0, 0.0], [10.0, 0.0]]
                .map(|point| Vertex {
                    point,
                    in_tangent: [0.0, 0.0],
                    out_tangent: [0.0, 0.0],
                })
                .to_vec(),
        };
        let masks = masks
            .iter()
            .map(|&(name, id)| {
                Child::Group(crate::model::Group {
                    name: Some(name.to_owned()),
                    mask: Some(MaskSettings {
                        inverted: false,
                        motion_blur: 0,
                        linear_falloff: false,
                        mode: 1,
                        id,
                    }),
                    ..group(
                        "ADBE Mask Atom",
                        vec![Child::Property(property(
                            "ADBE Mask Shape",
                            Stream::Static(Value::Path(shape.clone())),
                        ))],
                    )
                })
            })
            .collect();
        let spectrum = Child::Group(group(
            "ADBE Effect Parade",
            vec![Child::Group(group(
                "ADBE AudSpect",
                vec![
                    Child::Property(property(
                        "ADBE AudSpect-0001",
                        Stream::Static(Value::Layer(1)),
                    )),
                    Child::Property(property("ADBE AudSpect-0004", Stream::Static(path))),
                ],
            ))],
        ));
        let bars = layer(
            2,
            "Bars",
            vec![Child::Group(group("ADBE Mask Parade", masks)), spectrum],
        );
        import(&with_song(project(vec![(
            3,
            "Main",
            comp(vec![song, bars], 2.0),
        )])))
    }

    /// The layer `bars` of `import`.
    fn bars_of(import: &crate::map::Import) -> Json {
        import.json["layers"]
            .as_array()
            .expect("layers")
            .iter()
            .find(|l| l["id"] == "bars")
            .expect("bars")
            .clone()
    }

    #[test]
    fn a_spectrum_follows_the_mask_its_path_names_by_the_masks_own_id() {
        // The id is neither the mask's index nor the number in its name.
        let import = along(&[("Mask 1", 2), ("Mask 5", 6)], Value::Mask(6));
        let bars = bars_of(&import);
        assert_eq!(bars["fx"][0]["along"], "mask-5", "{bars}");
        assert_eq!(bars["mask"][1]["id"], "mask-5");
        // A mask no effect follows keeps no id.
        assert!(bars["mask"][0].get("id").is_none(), "{bars}");
        assert!(import.mapping.unsupported.is_empty());
    }

    #[test]
    fn a_path_naming_a_mask_the_layer_lacks_is_reported() {
        let import = along(&[("Mask 1", 2)], Value::Mask(6));
        let bars = bars_of(&import);
        assert!(bars["fx"][0].get("along").is_none(), "{bars}");
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert_eq!(
            what,
            ["a path parameter naming a mask its layer does not have"]
        );
        // None, stored as 0, draws along the line.
        let none = along(&[("Mask 1", 2)], Value::Number(0.0));
        assert!(bars_of(&none)["fx"][0].get("along").is_none());
        assert!(none.mapping.unsupported.is_empty());
    }
}
