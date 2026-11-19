use std::sync::{Arc, Mutex};

use super::*;
use crate::Outputs;
use crate::segment::tests::{Fake, Greys};
use crate::track::follow_all;

/// A tracker whose features grids hold each frame's grey, and whose tracks
/// stay where they were asked for, seen except on the window's last frame;
/// it records the frames whose features were computed and the shapes the
/// tracker read.
fn fake(
    features_run: Arc<Mutex<Vec<f32>>>,
    read: Arc<Mutex<Vec<Vec<usize>>>>,
) -> PointTracker<'static> {
    let features = Fake {
        inputs: &["frames"],
        f: Box::new(move |inputs: Vec<(String, Tensor)>| {
            let (_, frame) = &inputs[0];
            assert_eq!(frame.shape, [1, SIZE, SIZE, 3]);
            let Data::F32(v) = &frame.data else { panic!() };
            features_run.lock().unwrap().push(v[0]);
            Outputs(
                GRIDS
                    .iter()
                    .map(|g| ((*g).to_string(), Tensor::f32([1, 2, 2, 1], vec![v[0]; 4])))
                    .collect(),
            )
        }),
    };
    let tracker = Fake {
        // As exported: the second fine grid is not read.
        inputs: &["low0", "low1", "hi1", "query_points"],
        f: Box::new(move |inputs: Vec<(String, Tensor)>| {
            read.lock()
                .unwrap()
                .extend(inputs.iter().map(|(_, t)| t.shape.clone()));
            let q = &inputs.iter().find(|(n, _)| n == "query_points").unwrap().1;
            let t = inputs[0].1.shape[0];
            let Data::F32(q) = &q.data else { panic!() };
            let n = q.len() / 3;
            let mut tracks = Vec::new();
            let mut occlusion = Vec::new();
            for i in 0..n {
                for k in 0..t {
                    tracks.extend([q[i * 3 + 2], q[i * 3 + 1]]);
                    occlusion.push(if k + 1 == t { 10.0 } else { -10.0 });
                }
            }
            Outputs(vec![
                ("tracks".into(), Tensor::f32([n, t, 2], tracks)),
                ("occlusion".into(), Tensor::f32([n, t], occlusion)),
                (
                    "expected_dist".into(),
                    Tensor::f32([n, t], vec![-10.0; n * t]),
                ),
            ])
        }),
    };
    PointTracker::from_graphs(Box::new(features), Box::new(tracker), Box::new(Greys(60)))
}

#[test]
fn follows_points_in_clip_pixels_through_the_model() {
    let (run, read) = (Arc::default(), Arc::default());
    let mut tracker = fake(Arc::clone(&run), Arc::clone(&read));
    let tracks = tracker
        .follow(
            3..7,
            &[
                Query {
                    frame: 4,
                    at: [0.0, 5.0],
                },
                Query {
                    frame: 3,
                    at: [7.0, 2.5],
                },
            ],
        )
        .unwrap();
    assert_eq!(tracks.len(), 2);
    // Pixel centres go to the model's grid and back unchanged.
    for (track, at) in tracks.iter().zip([[0.0, 5.0], [7.0, 2.5]]) {
        assert_eq!(track.at.len(), 4);
        for p in &track.at {
            assert!(
                (p[0] - at[0]).abs() < 1e-3 && (p[1] - at[1]).abs() < 1e-3,
                "{p:?}"
            );
        }
        assert_eq!(track.seen, [true, true, true, false]);
    }
    assert_eq!(tracks[0].frame, 4);
    // Three grids stacked over four frames, and the queries.
    assert_eq!(
        *read.lock().unwrap(),
        [
            vec![4, 2, 2, 1],
            vec![4, 2, 2, 1],
            vec![4, 2, 2, 1],
            vec![2, 3]
        ]
    );
    // Each frame's features once.
    assert_eq!(run.lock().unwrap().len(), 4);
}

#[test]
fn keeps_the_features_windows_share() {
    let (run, read) = (Arc::default(), Arc::default());
    let mut tracker = fake(Arc::clone(&run), read);
    let tracks = follow_all(&mut tracker, 0, &[[1.0, 1.0]]).unwrap();
    assert_eq!(tracks[0].at.len(), 60);
    // Two windows, 0..48 and 40..60: the eight shared frames are computed
    // once.
    assert_eq!(run.lock().unwrap().len(), 60);
}

#[test]
fn refuses_a_run_wider_than_its_window() {
    let mut tracker = fake(Arc::default(), Arc::default());
    let err = tracker
        .follow(
            0..49,
            &[Query {
                frame: 0,
                at: [0.0, 0.0],
            }],
        )
        .unwrap_err();
    assert!(err.to_string().contains("at most 48"), "{err}");
    let err = tracker
        .follow(
            0..4,
            &[Query {
                frame: 5,
                at: [0.0, 0.0],
            }],
        )
        .unwrap_err();
    assert!(err.to_string().contains("outside"), "{err}");
}
