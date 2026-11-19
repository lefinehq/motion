use serde_json::json;

use super::*;
use crate::Matte;
use crate::store::tests::scratch;

fn manifest(request: Value) -> Manifest<Matte> {
    Manifest::new(
        request,
        Sha256::of(b"clip"),
        [4, 2],
        Made::here(2),
        Matte {
            frames: vec![Sha256::of(b"frame")],
        },
    )
}

#[test]
fn opens_what_it_keeps() {
    let store = scratch("manifest-keep");
    let request = json!({"prompts":[{"frame":0,"box":[0,0,2,2]}]});
    let made = manifest(request.clone());
    let digest = made.put(&store).unwrap();
    let opened = Manifest::<Matte>::open(&store, &digest, &request, &Sha256::of(b"clip")).unwrap();
    assert_eq!(opened, made);
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn names_the_bake_for_a_missing_asset() {
    let store = scratch("manifest-missing");
    let error = Manifest::<Matte>::open(
        &store,
        &Sha256::of(b"none"),
        &json!({}),
        &Sha256::of(b"clip"),
    )
    .unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert!(error.message.contains(BAKE_COMMAND), "{}", error.message);
}

#[test]
fn refuses_a_bake_of_another_request_or_clip() {
    let store = scratch("manifest-stale");
    let request = json!({"prompts":[{"frame":0,"box":[0,0,2,2]}]});
    let digest = manifest(request.clone()).put(&store).unwrap();
    let moved = json!({"prompts":[{"frame":0,"box":[1,0,2,2]}]});
    let error = Manifest::<Matte>::open(&store, &digest, &moved, &Sha256::of(b"clip")).unwrap_err();
    assert_eq!(error.kind, Kind::Conflict);
    assert!(
        error.message.contains("another request"),
        "{}",
        error.message
    );
    let error =
        Manifest::<Matte>::open(&store, &digest, &request, &Sha256::of(b"other")).unwrap_err();
    assert_eq!(error.kind, Kind::Conflict);
    assert!(error.message.contains("another clip"), "{}", error.message);
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn refuses_an_asset_of_another_kind() {
    let store = scratch("manifest-kind");
    let mut made = manifest(json!({}));
    made.asset = "track".into();
    let digest = made.put(&store).unwrap();
    let error =
        Manifest::<Matte>::open(&store, &digest, &json!({}), &Sha256::of(b"clip")).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    let junk = store.put(b"not json").unwrap();
    let error =
        Manifest::<Matte>::open(&store, &junk, &json!({}), &Sha256::of(b"clip")).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn records_each_graph_once() {
    let mut made = Made::here(4);
    assert_eq!(made.threads, 4);
    assert!(made.platform.contains('-'), "{}", made.platform);
    let run = crate::GraphRun {
        model: "segment.video".into(),
        graph: "image-encoder".into(),
        sha256: Sha256::of(b"graph"),
        backend: "cpu".into(),
    };
    made.ran(run.clone());
    made.ran(run);
    assert_eq!(made.graphs.len(), 1);
}

#[test]
fn keeps_a_track_and_opens_it() {
    use crate::{Fit, MaskStats, PointTrack, Track, TrackFrame};
    let store = scratch("manifest-track");
    let track = Track {
        prompt_frame: 0,
        corners: [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]],
        anchors: vec![],
        times: vec![0.0, 0.04],
        frames: vec![
            TrackFrame {
                mask: Some(MaskStats {
                    centroid: [1.0, 1.0],
                    area: 4.0,
                    scale: 1.0,
                    angle: 0.0,
                }),
                fit: Some(Fit {
                    position: [1.0, 1.0],
                    scale: 1.0,
                    rotate: 0.0,
                    corners: [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]],
                    guarded: false,
                    inliers: 8,
                    points: 8,
                }),
            },
            TrackFrame {
                mask: None,
                fit: None,
            },
        ],
        points: vec![PointTrack {
            frame: 0,
            at: vec![[1.0, 1.0], [1.5, 1.0]],
            seen: vec![true, false],
        }],
    };
    let request = json!({"id":"screen"});
    let digest = Manifest::new(
        request.clone(),
        Sha256::of(b"clip"),
        [2, 2],
        Made::here(1),
        track.clone(),
    )
    .put(&store)
    .unwrap();
    let opened = Manifest::<Track>::open(&store, &digest, &request, &Sha256::of(b"clip")).unwrap();
    assert_eq!(opened.content, track);
    std::fs::remove_dir_all(store.root()).unwrap();
}
