use nakkash_baked::{Fit, Made, MaskStats, TrackFrame};
use nakkash_format::Sha256;
use pretty_assertions::assert_eq;

use super::*;

/// A clip of 1920 × 1080 tracked over three frames: the object moves right
/// 100 px a frame, grows and turns; frame 1 has no mask, frame 2 no fit.
fn track() -> Manifest<Track> {
    let fit = |x: f64, rotate: f64| Fit {
        position: [x, 500.0],
        scale: 1.0,
        rotate,
        corners: [
            [x - 50.0, 450.0],
            [x + 50.0, 450.0],
            [x + 50.0, 550.0],
            [x - 50.0, 550.0],
        ],
        guarded: false,
        inliers: 20,
        points: 20,
    };
    let mask = |x: f64, scale: f64| MaskStats {
        centroid: [x, 500.0],
        area: 1e4,
        scale,
        angle: 0.0,
    };
    let track = Track {
        prompt_frame: 0,
        corners: [
            [950.0, 450.0],
            [1050.0, 450.0],
            [1050.0, 550.0],
            [950.0, 550.0],
        ],
        anchors: vec![],
        times: vec![0.0, 1.0 / 30.0, 2.0 / 30.0],
        frames: vec![
            TrackFrame {
                mask: Some(mask(999.5, 1.0)),
                fit: Some(fit(999.5, 170.0)),
            },
            TrackFrame {
                mask: None,
                fit: Some(fit(1099.5, -175.0)),
            },
            TrackFrame {
                mask: Some(mask(1199.5, 1.23456)),
                fit: None,
            },
        ],
        points: vec![],
    };
    Manifest::new(
        json!({"id": "screen"}),
        Sha256::of(b"clip"),
        [1920, 1080],
        Made::here(1),
        track,
    )
}

/// A scene whose video layer `phone` shows the clip in a box half its size.
fn scene(extra: Value) -> Value {
    let mut video = json!({"id": "phone", "video": "clip.mov", "box": [960, 540], "at": "480 270", "offset": "1s", "stretch": 2});
    if let (Some(v), Some(e)) = (video.as_object_mut(), extra.as_object()) {
        v.extend(e.clone());
    }
    json!({"nakkash": 1, "dur": "4s", "layers": [{"id": "title", "text": "Hi"}, video]})
}

fn request(corners: bool) -> PivotRequest {
    PivotRequest {
        video: "phone".parse().unwrap(),
        pivot: "screen".into(),
        corners,
    }
}

#[test]
fn writes_keys_on_a_pivot_under_the_video() {
    let mut json = scene(json!({}));
    let applied = apply(&mut json, &request(true), &track()).unwrap();
    assert_eq!(
        applied.made,
        ["screen", "screen-tl", "screen-tr", "screen-br", "screen-bl"]
    );
    assert_eq!(applied.pivots[0], ("screen".into(), 2));
    let layers = json["layers"].as_array().unwrap();
    assert_eq!(layers.len(), 7, "the pivots follow the video");
    insta::assert_snapshot!(serde_json::to_string_pretty(&layers[2..4]).unwrap());
}

#[test]
fn applying_again_replaces_the_keys_and_makes_nothing() {
    let mut json = scene(json!({}));
    apply(&mut json, &request(true), &track()).unwrap();
    let once = json.clone();
    let applied = apply(&mut json, &request(true), &track()).unwrap();
    assert!(applied.made.is_empty());
    assert_eq!(json, once);
}

#[test]
fn places_clip_pixels_through_the_fit() {
    // `contain` in a box as tall as the clip and twice as wide: the clip
    // lands in the middle half of the box, at its own size.
    let mut json = scene(json!({"box": [3840, 1080], "fit": "contain"}));
    apply(&mut json, &request(false), &track()).unwrap();
    let position = &json["layers"][2]["anim"]["position"];
    assert_eq!(position[0]["v"], json!([1960.0, 500.5]));
}

#[test]
fn works_inside_a_composition() {
    let mut json = json!({"nakkash": 1, "dur": "4s",
        "comps": {"card": {"size": "1920x1080", "dur": "4s", "layers": [{"id": "phone", "video": "clip.mov", "box": [1920, 1080]}]}},
        "layers": [{"id": "shell", "comp": "card"}]});
    let req = PivotRequest {
        video: "shell/phone".parse().unwrap(),
        pivot: "screen".into(),
        corners: false,
    };
    apply(&mut json, &req, &track()).unwrap();
    let inner = json["comps"]["card"]["layers"].as_array().unwrap();
    assert_eq!(inner[1]["parent"], "phone");
    assert_eq!(inner[1]["anim"]["position"][0]["v"], json!([1000.0, 500.5]));
}

#[test]
fn refuses_what_cannot_take_a_track() {
    let kind =
        |mut json: Value, req: PivotRequest| apply(&mut json, &req, &track()).unwrap_err().kind;
    assert_eq!(
        kind(scene(json!({"remap": 0.5})), request(false)),
        Kind::Conflict
    );
    assert_eq!(
        kind(
            scene(json!({"anim": {"remap": "0 -> 1 @0-1s"}})),
            request(false)
        ),
        Kind::Conflict
    );
    let title = PivotRequest {
        video: "title".parse().unwrap(),
        ..request(false)
    };
    assert_eq!(kind(scene(json!({})), title), Kind::Conflict, "not a video");
    let gone = PivotRequest {
        video: "nobody".parse().unwrap(),
        ..request(false)
    };
    assert_eq!(kind(scene(json!({})), gone), Kind::Missing);
    let taken = PivotRequest {
        pivot: "title".into(),
        ..request(false)
    };
    assert_eq!(kind(scene(json!({})), taken), Kind::Conflict, "not a pivot");
    let mut json = scene(json!({}));
    let before = json.clone();
    let bad = PivotRequest {
        pivot: "a b".into(),
        ..request(false)
    };
    assert!(apply(&mut json, &bad, &track()).is_err());
    assert_eq!(json, before, "nothing written on a failure");
}

#[test]
fn writes_times_to_the_microsecond() {
    assert_eq!(seconds(0.0), "0s");
    assert_eq!(seconds(1.0 / 30.0), "0.033333s");
    assert_eq!(seconds(2.5), "2.5s");
    assert_eq!(factor(1.23456), 1.2346);
}
