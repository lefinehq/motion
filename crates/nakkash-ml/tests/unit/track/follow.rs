use super::*;

/// A tracker that knows where every point goes: each moves 2 px right a
/// frame, from wherever it is asked for, and is unseen on `hidden` frames.
/// It records the windows it was asked for.
pub(crate) struct Drifting {
    pub(crate) frames: usize,
    pub(crate) window: usize,
    pub(crate) hidden: Range<usize>,
    pub(crate) asked: Vec<Range<usize>>,
}

impl FollowPoints for Drifting {
    fn frames(&self) -> usize {
        self.frames
    }

    fn window(&self) -> usize {
        self.window
    }

    fn follow(&mut self, frames: Range<usize>, queries: &[Query]) -> Result<Vec<PointTrack>> {
        assert!(frames.len() <= self.window && frames.end <= self.frames);
        self.asked.push(frames.clone());
        Ok(queries
            .iter()
            .map(|q| {
                assert!(
                    frames.contains(&(q.frame as usize)),
                    "a query inside its window"
                );
                PointTrack {
                    frame: q.frame,
                    at: frames
                        .clone()
                        .map(|f| [q.at[0] + 2.0 * (f as f64 - f64::from(q.frame)), q.at[1]])
                        .collect(),
                    seen: frames.clone().map(|f| !self.hidden.contains(&f)).collect(),
                }
            })
            .collect())
    }
}

#[test]
fn follows_in_one_window_when_the_clip_fits() {
    let mut tracker = Drifting {
        frames: 10,
        window: 64,
        hidden: 0..0,
        asked: vec![],
    };
    let tracks = follow_all(&mut tracker, 4, &[[10.0, 5.0]]).unwrap();
    assert_eq!(tracker.asked, vec![0..10]);
    assert_eq!(tracks[0].frame, 4);
    assert_eq!(tracks[0].at[0], [2.0, 5.0]);
    assert_eq!(tracks[0].at[9], [20.0, 5.0]);
    assert!(tracks[0].seen.iter().all(|&s| s));
}

#[test]
fn chains_windows_both_ways_through_a_long_clip() {
    let mut tracker = Drifting {
        frames: 100,
        window: 20,
        hidden: 0..0,
        asked: vec![],
    };
    let tracks = follow_all(&mut tracker, 50, &[[0.0, 0.0]]).unwrap();
    // Each window shares OVERLAP frames with the one before it.
    assert_eq!(tracker.asked[0], 40..60);
    assert!(tracker.asked.windows(2).skip(1).all(|w| w[0] != w[1]));
    // The motion is exact, so the chained track is too, on every frame.
    for f in 0..100 {
        assert!(
            (tracks[0].at[f][0] - 2.0 * (f as f64 - 50.0)).abs() < 1e-9,
            "frame {f}"
        );
        assert!(tracks[0].seen[f], "frame {f}");
    }
}

#[test]
fn a_point_unseen_where_a_window_starts_is_lost_beyond_it() {
    // From frame 45 the first window is 35..55, and the next starts on
    // frame 47, where the point is hidden: past the first window it is lost.
    let mut tracker = Drifting {
        frames: 100,
        window: 20,
        hidden: 46..50,
        asked: vec![],
    };
    let tracks = follow_all(&mut tracker, 45, &[[0.0, 0.0]]).unwrap();
    assert!(tracks[0].seen[45] && tracks[0].seen[52]);
    assert!((55..100).all(|f| !tracks[0].seen[f]));
    assert_eq!(tracker.asked.first(), Some(&(35..55)));
}

#[test]
fn refuses_a_window_too_small_to_overlap_or_a_wrong_answer() {
    let mut tracker = Drifting {
        frames: 100,
        window: OVERLAP + 1,
        hidden: 0..0,
        asked: vec![],
    };
    assert_eq!(
        follow_all(&mut tracker, 0, &[[0.0, 0.0]]).unwrap_err().kind,
        Kind::Invalid
    );
    let mut tracker = Drifting {
        frames: 10,
        window: 64,
        hidden: 0..0,
        asked: vec![],
    };
    assert_eq!(
        follow_all(&mut tracker, 10, &[[0.0, 0.0]])
            .unwrap_err()
            .kind,
        Kind::OutOfRange
    );

    struct Short;
    impl FollowPoints for Short {
        fn frames(&self) -> usize {
            5
        }
        fn window(&self) -> usize {
            5
        }
        fn follow(&mut self, _: Range<usize>, _: &[Query]) -> Result<Vec<PointTrack>> {
            Ok(vec![])
        }
    }
    assert_eq!(
        follow_all(&mut Short, 0, &[[0.0, 0.0]]).unwrap_err().kind,
        Kind::Invalid
    );
}
