use rstest::rstest;
use serde_json::json;

use super::*;
use crate::{Content, parse_document};

/// The scene of one video layer `clip` with `extra` keys.
fn scene(extra: Value) -> String {
    let mut layer = json!({"id":"clip","video":"clip.mov","box":[1920,1080]});
    for (key, value) in extra.as_object().expect("an object") {
        layer[key] = value.clone();
    }
    json!({"nakkash":1,"dur":"1s","layers":[layer]}).to_string()
}

/// The video content of the layer `clip` in `text`.
fn video(text: &str) -> crate::VideoContent {
    let doc = parse_document(text).unwrap_or_else(|e| panic!("{e}"));
    match &doc.scene().root.layers[0].content {
        Content::Video(video) => video.clone(),
        other => panic!("not a video: {other:?}"),
    }
}

/// The error of `text`: its kind, its place as printed, and whether it has
/// an expected form.
fn error(text: &str) -> (Kind, String, bool) {
    let e = parse_document(text).unwrap_err();
    (
        e.kind,
        e.path.map(|p| p.to_string()).unwrap_or_default(),
        e.expected.is_some(),
    )
}

#[test]
fn reads_a_cutout_with_its_defaults() {
    let video = video(&scene(
        json!({"cutout":{"prompts":[{"frame":3,"box":[10,20,30,40]}]}}),
    ));
    let cutout = video.cutout.expect("a cutout");
    assert_eq!(
        cutout,
        Cutout {
            prompts: vec![Prompt {
                frame: 3,
                bbox: Some([10.0, 20.0, 30.0, 40.0]),
                include: vec![],
                exclude: vec![],
            }],
            edges: Edges::Refine,
            band: 12.0,
            agree: 0.9,
            smooth: None,
            baked: None,
        }
    );
    assert_eq!(cutout.first_frame(), 3);
    assert!(video.tracks.is_empty());
}

#[test]
fn reads_every_setting_and_the_digest() {
    let digest = Sha256::of(b"matte");
    let video = video(&scene(json!({"cutout":{
        "prompts":[{"frame":5,"include":[[1,2]],"exclude":[[3,4]]},{"frame":2,"box":[0,0,9,9]}],
        "edges":"soft","band":6,"agree":0.5,"smooth":{"amount":0.9},
        "baked":digest.to_string()}})));
    let cutout = video.cutout.unwrap();
    assert_eq!(cutout.edges, Edges::Soft);
    assert_eq!((cutout.band, cutout.agree), (6.0, 0.5));
    assert_eq!(
        cutout.smooth,
        Some(Smooth {
            amount: 0.9,
            tolerance: 0.08
        })
    );
    assert_eq!(cutout.baked, Some(digest));
    assert_eq!(cutout.first_frame(), 2);
}

#[test]
fn the_request_fills_defaults_and_leaves_out_the_digest() {
    let written = |extra: Value| {
        video(&scene(json!({ "cutout": extra })))
            .cutout
            .unwrap()
            .request()
    };
    let short = written(json!({"prompts":[{"frame":0,"box":[0,0,9,9]}]}));
    let long = written(json!({"prompts":[{"frame":0,"box":[0,0,9,9],"include":[]}],
        "edges":"refine","band":12,"agree":0.9,"baked":Sha256::of(b"x").to_string()}));
    assert_eq!(short, long);
    assert_eq!(
        short.to_string(),
        r#"{"prompts":[{"frame":0,"box":[0.0,0.0,9.0,9.0],"include":[],"exclude":[]}],"edges":"refine","band":12.0,"agree":0.9,"smooth":null}"#
    );
}

#[test]
fn reads_tracks() {
    let corners = json!([[0, 0], [10, 0], [10, 10], [0, 10]]);
    let video = video(&scene(json!({"tracks":[
        {"id":"screen","prompts":[{"frame":0,"box":[0,0,10,10]}],"corners":corners,
         "anchors":[{"frame":36,"corners":corners}]},
        {"id":"label","prompts":[{"frame":4,"include":[[5,5]]}]}]})));
    assert_eq!(video.tracks.len(), 2);
    let screen = &video.tracks[0];
    assert_eq!(screen.id, "screen");
    assert_eq!(screen.anchors[0].frame, 36);
    assert_eq!(screen.corners.unwrap()[2], [10.0, 10.0]);
    assert_eq!(video.tracks[1].first_frame(), 4);
    assert_eq!(
        screen.request().to_string(),
        r#"{"id":"screen","prompts":[{"frame":0,"box":[0.0,0.0,10.0,10.0],"include":[],"exclude":[]}],"corners":[[0.0,0.0],[10.0,0.0],[10.0,10.0],[0.0,10.0]],"anchors":[{"frame":36,"corners":[[0.0,0.0],[10.0,0.0],[10.0,10.0],[0.0,10.0]]}]}"#
    );
}

#[rstest]
#[case::not_an_object(json!({"cutout":3}), Kind::WrongType, "clip.cutout")]
#[case::no_prompts(json!({"cutout":{}}), Kind::Missing, "clip.cutout.prompts")]
#[case::empty_prompts(json!({"cutout":{"prompts":[]}}), Kind::Missing, "clip.cutout.prompts")]
#[case::unknown_key(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"band-width":3}}), Kind::UnknownName, "clip.cutout.band-width")]
#[case::no_frame(json!({"cutout":{"prompts":[{"box":[0,0,1,1]}]}}), Kind::Missing, "clip.cutout.prompts[0]")]
#[case::no_box_or_point(json!({"cutout":{"prompts":[{"frame":0,"exclude":[[1,1]]}]}}), Kind::Missing, "clip.cutout.prompts[0]")]
#[case::backward_box(json!({"cutout":{"prompts":[{"frame":0,"box":[5,0,1,1]}]}}), Kind::Invalid, "clip.cutout.prompts[0].box")]
#[case::short_box(json!({"cutout":{"prompts":[{"frame":0,"box":[5,0,1]}]}}), Kind::Invalid, "clip.cutout.prompts[0].box")]
#[case::box_left_of_the_clip(json!({"cutout":{"prompts":[{"frame":0,"box":[-1,0,1,1]}]}}), Kind::OutOfRange, "clip.cutout.prompts[0].box")]
#[case::point_above_the_clip(json!({"cutout":{"prompts":[{"frame":0,"include":[[1,-2]]}]}}), Kind::OutOfRange, "clip.cutout.prompts[0].include")]
#[case::excluded_off_the_clip(json!({"cutout":{"prompts":[{"frame":0,"include":[[1,1]],"exclude":[[-1,1]]}]}}), Kind::OutOfRange, "clip.cutout.prompts[0].exclude")]
#[case::corner_off_the_clip(json!({"tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"corners":[[0,0],[1,0],[1,1],[-3,1]]}]}), Kind::OutOfRange, "clip.tracks[0].corners")]
#[case::anchor_corner_off_the_clip(json!({"tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"anchors":[{"frame":3,"corners":[[0,-1],[1,0],[1,1],[0,1]]}]}]}), Kind::OutOfRange, "clip.tracks[0].anchors[0].corners")]
#[case::two_on_a_frame(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]},{"frame":0,"include":[[1,1]]}]}}), Kind::Conflict, "clip.cutout.prompts[1].frame")]
#[case::unknown_edges(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"edges":"crisp"}}), Kind::UnknownName, "clip.cutout.edges")]
#[case::band(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"band":0}}), Kind::OutOfRange, "clip.cutout.band")]
#[case::agree(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"agree":1.5}}), Kind::OutOfRange, "clip.cutout.agree")]
#[case::amount(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"smooth":{"amount":2}}}), Kind::OutOfRange, "clip.cutout.smooth.amount")]
#[case::tolerance(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"smooth":{"tolerance":0}}}), Kind::OutOfRange, "clip.cutout.smooth.tolerance")]
#[case::digest(json!({"cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"baked":"abc"}}), Kind::Invalid, "clip.cutout.baked")]
#[case::tracks_not_a_list(json!({"tracks":{}}), Kind::WrongType, "clip.tracks")]
#[case::track_without_id(json!({"tracks":[{"prompts":[{"frame":0,"box":[0,0,1,1]}]}]}), Kind::Missing, "clip.tracks[0]")]
#[case::bad_track_id(json!({"tracks":[{"id":"Two Words","prompts":[{"frame":0,"box":[0,0,1,1]}]}]}), Kind::Invalid, "clip.tracks[0].id")]
#[case::two_tracks_one_id(json!({"tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}]},{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}]}]}), Kind::Conflict, "clip.tracks[1].id")]
#[case::anchor_on_first(json!({"tracks":[{"id":"a","prompts":[{"frame":2,"box":[0,0,1,1]}],"anchors":[{"frame":2,"corners":[[0,0],[1,0],[1,1],[0,1]]}]}]}), Kind::Conflict, "clip.tracks[0].anchors[0].frame")]
#[case::two_anchors(json!({"tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"anchors":[{"frame":3,"corners":[[0,0],[1,0],[1,1],[0,1]]},{"frame":3,"corners":[[0,0],[1,0],[1,1],[0,1]]}]}]}), Kind::Conflict, "clip.tracks[0].anchors[1].frame")]
#[case::anchor_without_corners(json!({"tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"anchors":[{"frame":3}]}]}), Kind::Missing, "clip.tracks[0].anchors[0]")]
fn reports_each_mistake_where_it_is(#[case] extra: Value, #[case] kind: Kind, #[case] path: &str) {
    let (found, at, expected) = error(&scene(extra));
    assert_eq!((found, at.as_str()), (kind, path));
    assert!(expected, "an expected form at {path}");
}

#[rstest]
#[case::rect(r#"{"id":"a","rect":"1x1","cutout":{}}"#, "a.cutout")]
#[case::image(r#"{"id":"a","image":"a.png","box":[1,1],"tracks":[]}"#, "a.tracks")]
#[case::filter(r##"{"id":"a","fx":[{"fill":"#FFFFFF"}],"cutout":{}}"##, "a.cutout")]
fn only_video_layers_take_them(#[case] layer: &str, #[case] path: &str) {
    let text = format!(r#"{{"nakkash":1,"dur":"1s","layers":[{layer}]}}"#);
    let (kind, at, _) = error(&text);
    assert_eq!((kind, at.as_str()), (Kind::Conflict, path));
}
