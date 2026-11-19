//! A synthetic shot with exact ground truth: a checkered card 240 × 160
//! that moves, turns, grows and tilts through 48 frames of a 640 × 360
//! clip, half covered from frame 22 on. The fake point tracker knows where
//! every surface point is, adds a drift that grows with distance from the
//! frame it was asked on, and loses points under the cover.

use std::ops::Range;

use super::fit::distance;
use super::linalg::{self, Mat3};
use super::*;
use crate::track::follow::Query;

const W: usize = 640;
const H: usize = 360;
const FRAMES: usize = 48;
const CARD: [f64; 2] = [240.0, 160.0];
const COVERED_FROM: usize = 22;

/// The card's map, card units → clip pixels, on frame `t`.
fn card(t: usize) -> Mat3 {
    let f = t as f64 / (FRAMES - 1) as f64;
    let (scale, turn) = (1.0 + 0.4 * f, 0.5 * f);
    let centre = [220.0 + 200.0 * f, 170.0 + 20.0 * f];
    let (s, c) = turn.sin_cos();
    let tilt: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0004 * f, 0.0, 1.0]];
    let place: Mat3 = [
        [scale * c, -scale * s, centre[0]],
        [scale * s, scale * c, centre[1]],
        [0.0, 0.0, 1.0],
    ];
    let to_centre: Mat3 = [
        [1.0, 0.0, -CARD[0] / 2.0],
        [0.0, 1.0, -CARD[1] / 2.0],
        [0.0, 0.0, 1.0],
    ];
    linalg::mul(&place, &linalg::mul(&tilt, &to_centre))
}

/// The card's corners on frame `t`.
fn corners(t: usize) -> [[f64; 2]; 4] {
    [
        [0.0, 0.0],
        [CARD[0], 0.0],
        [CARD[0], CARD[1]],
        [0.0, CARD[1]],
    ]
    .map(|p| linalg::apply(&card(t), p).unwrap())
}

/// The segmenter's logits on frame `t`: +10 where a 256² cell's centre is
/// on the card, −10 elsewhere. The mask sees the whole card, cover or not.
fn logits(t: usize) -> Plane {
    let quad = corners(t);
    let data = (0..256 * 256)
        .map(|k| {
            let x = ((k % 256) as f64 + 0.5) * W as f64 / 256.0 - 0.5;
            let y = ((k / 256) as f64 + 0.5) * H as f64 / 256.0 - 0.5;
            if in_polygon([x, y], &quad) {
                10.0
            } else {
                -10.0
            }
        })
        .collect();
    Plane {
        width: 256,
        height: 256,
        data,
    }
}

/// Frame `t`: the checkered card (20 px squares) over a flat grey.
fn picture(t: usize) -> Rgb8 {
    let back = linalg::inverse(&card(t)).unwrap();
    let mut data = vec![120u8; W * H * 3];
    for y in 0..H {
        for x in 0..W {
            let [u, v] = linalg::apply(&back, [x as f64, y as f64]).unwrap();
            if (0.0..CARD[0]).contains(&u) && (0.0..CARD[1]).contains(&v) {
                let dark = ((u / 20.0) as i64 + (v / 20.0) as i64) % 2 == 0;
                data[(y * W + x) * 3..][..3].fill(if dark { 30 } else { 225 });
            }
        }
    }
    Rgb8 {
        width: W,
        height: H,
        data,
    }
}

/// The fake point tracker.
struct Oracle {
    /// Drift per frame of distance from the query's frame, px.
    drift: f64,
    asked: usize,
}

impl FollowPoints for Oracle {
    fn frames(&self) -> usize {
        FRAMES
    }

    fn window(&self) -> usize {
        FRAMES
    }

    fn follow(&mut self, frames: Range<usize>, queries: &[Query]) -> Result<Vec<PointTrack>> {
        self.asked += 1;
        Ok(queries
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let k = q.frame as usize;
                let at_card = linalg::apply(&linalg::inverse(&card(k)).unwrap(), q.at).unwrap();
                // Each point drifts its own way, the same on every run.
                let dir = (i as f64 * 2.399).sin_cos();
                let at = frames
                    .clone()
                    .map(|f| {
                        let p = linalg::apply(&card(f), at_card).unwrap();
                        let d = self.drift * (f as f64 - k as f64).abs();
                        [p[0] + d * dir.0, p[1] + d * dir.1]
                    })
                    .collect();
                let seen = frames
                    .clone()
                    .map(|f| f < COVERED_FROM || at_card[0] > CARD[0] / 2.0)
                    .collect();
                PointTrack {
                    frame: q.frame,
                    at,
                    seen,
                }
            })
            .collect())
    }
}

fn times() -> Vec<f64> {
    (0..FRAMES).map(|t| t as f64 / 25.0).collect()
}

fn bake_with(spec: &TrackSpec, drift: f64) -> Track {
    let logits: Vec<Plane> = (0..FRAMES).map(logits).collect();
    let times = times();
    let mut frame = |t: usize| Ok(picture(t));
    let clip = TrackClip {
        size: (W, H),
        times: &times,
        logits: &logits,
        frame: &mut frame,
    };
    bake(spec, clip, &mut Oracle { drift, asked: 0 }).unwrap()
}

fn corner_error(track: &Track, frames: Range<usize>) -> f64 {
    let errors: Vec<f64> = frames
        .flat_map(|t| {
            let fit = track.frames[t].fit.unwrap();
            (0..4).map(move |i| distance(fit.corners[i], corners(t)[i]))
        })
        .collect();
    errors.iter().sum::<f64>() / errors.len() as f64
}

fn spec(anchors: Vec<Anchor>) -> TrackSpec {
    TrackSpec {
        prompt_frame: 0,
        corners: Some(corners(0)),
        anchors,
    }
}

#[test]
fn the_mask_centroid_follows_the_card() {
    let track = bake_with(&spec(vec![]), 0.0);
    let mut worst: f64 = 0.0;
    for t in 0..FRAMES {
        let q = corners(t);
        // The centroid of the quad, by its two triangles' areas.
        let tri = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
            let area = ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2.0;
            (
                area,
                [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0],
            )
        };
        let ((a1, c1), (a2, c2)) = (tri(q[0], q[1], q[2]), tri(q[0], q[2], q[3]));
        let truth = [
            (a1 * c1[0] + a2 * c2[0]) / (a1 + a2),
            (a1 * c1[1] + a2 * c2[1]) / (a1 + a2),
        ];
        worst = worst.max(distance(track.frames[t].mask.unwrap().centroid, truth));
    }
    assert!(worst < 1.0, "worst centroid error {worst} px");
    let last = track.frames[FRAMES - 1].mask.unwrap();
    assert!((last.scale - 1.4).abs() < 0.05, "scale {}", last.scale);
    assert_eq!(track.frames[0].mask.unwrap().scale, 1.0);
}

#[test]
fn exact_points_give_exact_corners_through_the_cover() {
    let track = bake_with(&spec(vec![]), 0.0);
    let before = corner_error(&track, 0..COVERED_FROM);
    let after = corner_error(&track, COVERED_FROM..FRAMES);
    assert!(before < 0.05 && after < 0.05, "{before} {after}");
    assert!(track.frames.iter().all(|f| !f.fit.unwrap().guarded));
    // Rotation from the points: half a radian by the last frame, within
    // what the tilt bends it by.
    let turned = track.frames[FRAMES - 1].fit.unwrap().rotate;
    assert!((turned - 0.5f64.to_degrees()).abs() < 3.0, "{turned}");
}

#[test]
fn an_anchor_is_met_exactly_and_cuts_the_drift_after_it() {
    let alone = bake_with(&spec(vec![]), 0.4);
    let anchored = bake_with(
        &spec(vec![Anchor {
            frame: 36,
            corners: corners(36),
        }]),
        0.4,
    );
    let fit = anchored.frames[36].fit.unwrap();
    for i in 0..4 {
        assert!(distance(fit.corners[i], corners(36)[i]) < 1e-9);
    }
    // Right after the anchor the drift is gone: about 17 px against under 1.
    let (drifted, corrected) = (
        corner_error(&alone, 36..41),
        corner_error(&anchored, 36..41),
    );
    assert!(
        corrected < drifted / 5.0,
        "after the anchor: {drifted} → {corrected} px"
    );
    let (drifted, corrected) = (
        corner_error(&alone, 36..FRAMES),
        corner_error(&anchored, 36..FRAMES),
    );
    assert!(
        corrected < drifted,
        "to the end: {drifted} → {corrected} px"
    );
    // Near the prompt the far anchor, weighed by distance and inliers,
    // changes almost nothing.
    let (drifted, corrected) = (corner_error(&alone, 1..8), corner_error(&anchored, 1..8));
    assert!(
        corrected < 1.1 * drifted + 0.05,
        "near the prompt: {drifted} → {corrected} px"
    );
    let met = anchored.frames[36].fit.unwrap();
    assert_eq!(
        (met.inliers, met.points),
        (34, 34),
        "the points seen on the anchor"
    );
    assert_eq!(anchored.anchors, [36]);
    assert!(
        anchored.points.iter().any(|p| p.frame == 36),
        "points chosen again at the anchor"
    );
}

#[test]
fn bakes_the_same_bytes_every_time() {
    let s = spec(vec![Anchor {
        frame: 30,
        corners: corners(30),
    }]);
    let a = serde_json::to_vec(&bake_with(&s, 0.3)).unwrap();
    let b = serde_json::to_vec(&bake_with(&s, 0.3)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn finds_corners_from_the_mask_when_none_are_given() {
    let track = bake_with(
        &TrackSpec {
            prompt_frame: 0,
            corners: None,
            anchors: vec![],
        },
        0.0,
    );
    for (got, want) in track.corners.iter().zip(corners(0)) {
        assert!(distance(*got, want) < 3.0, "{got:?} against {want:?}");
    }
    assert_eq!(track.times, times());
}

#[test]
fn refuses_what_does_not_fit_the_clip() {
    let logits: Vec<Plane> = (0..FRAMES).map(logits).collect();
    let times = times();
    let mut frame = |t: usize| Ok(picture(t));
    let mut run = |spec: &TrackSpec, logits: &[Plane]| {
        let clip = TrackClip {
            size: (W, H),
            times: &times,
            logits,
            frame: &mut frame,
        };
        bake(
            spec,
            clip,
            &mut Oracle {
                drift: 0.0,
                asked: 0,
            },
        )
        .unwrap_err()
        .kind
    };
    assert_eq!(
        run(&spec(vec![]), &logits[..10]),
        Kind::Invalid,
        "masks and times disagree"
    );
    assert_eq!(
        run(
            &TrackSpec {
                prompt_frame: 48,
                ..spec(vec![])
            },
            &logits
        ),
        Kind::OutOfRange
    );
    let twice = vec![
        Anchor {
            frame: 10,
            corners: corners(10),
        },
        Anchor {
            frame: 10,
            corners: corners(10),
        },
    ];
    assert_eq!(run(&spec(twice), &logits), Kind::Conflict);
    let outside = vec![Anchor {
        frame: 99,
        corners: corners(0),
    }];
    assert_eq!(run(&spec(outside), &logits), Kind::OutOfRange);
    let mut empty = logits.clone();
    empty[0].data.fill(-10.0);
    assert_eq!(
        run(&spec(vec![]), &empty),
        Kind::Invalid,
        "no object on the prompted frame"
    );
}
