//! Following points through a clip, in windows the point tracker can hold.

use std::ops::Range;

use nakkash_baked::PointTrack;
use nakkash_format::{Diagnostic, Kind, Result};

/// How many frames two windows share; the later one starts from where the
/// earlier one put its points on the first shared frame.
pub const OVERLAP: usize = 8;

/// A point to follow: where it is on one frame, in clip pixels (pixel
/// centres at whole numbers).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Query {
    /// The clip frame it is given on.
    pub frame: u32,
    /// Where it is on that frame.
    pub at: [f64; 2],
}

/// What follows points through a clip: the point tracker, run by the
/// inference runtime. It looks at a whole window of frames at once, so a
/// query may lie anywhere in the window and is followed both ways.
pub trait FollowPoints {
    /// The clip's frame count.
    fn frames(&self) -> usize;

    /// The most frames one call may span.
    fn window(&self) -> usize;

    /// Follows `queries`, each on a frame in `frames`, through `frames`:
    /// one [`PointTrack`] per query, in order, with one position and one
    /// visibility per frame of `frames`, in clip pixels.
    ///
    /// # Errors
    /// The runtime's diagnostic.
    fn follow(&mut self, frames: Range<usize>, queries: &[Query]) -> Result<Vec<PointTrack>>;
}

/// Follows `queries`, all given on frame `from`, through the whole clip.
///
/// A clip longer than the tracker's window is followed in windows that
/// share [`OVERLAP`] frames, outwards from the window holding `from`: the
/// next window starts from the positions the previous one gave on their
/// first shared frame, for the points it saw there. Chaining costs what
/// each window's error adds to the next, and a point unseen on the frame
/// the next window starts from is lost for the rest of the clip in that
/// direction; anchors ([D88]) are the remedy.
///
/// # Errors
/// The tracker's diagnostic, or a [`Kind::Invalid`] one when it returns
/// tracks of the wrong length or a window of fewer than [`OVERLAP`] + 2
/// frames.
///
/// [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots
pub fn follow_all(
    tracker: &mut dyn FollowPoints,
    from: u32,
    points: &[[f64; 2]],
) -> Result<Vec<PointTrack>> {
    let n = tracker.frames();
    let from_usize = from as usize;
    if from_usize >= n {
        return Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("frame {from} of a clip of {n} frames"),
        ));
    }
    let window = tracker.window().min(n);
    if window < n && window < OVERLAP + 2 {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "a point tracker window of {window} frames; it takes at least {}",
                OVERLAP + 2
            ),
        ));
    }
    let mut tracks: Vec<PointTrack> = points
        .iter()
        .map(|_| PointTrack {
            frame: from,
            at: vec![[0.0, 0.0]; n],
            seen: vec![false; n],
        })
        .collect();
    // The first window: centred on `from`, kept inside the clip.
    let start = from_usize.saturating_sub(window / 2).min(n - window);
    let first = start..start + window;
    let queries: Vec<Query> = points.iter().map(|&at| Query { frame: from, at }).collect();
    let all: Vec<usize> = (0..points.len()).collect();
    copy_into(
        &mut tracks,
        &all,
        &run(tracker, first.clone(), &queries)?,
        first.clone(),
        first.clone(),
    );
    // Onwards, then backwards.
    let mut done = first.clone();
    while done.end < n {
        let next = (done.end - OVERLAP)..(done.end - OVERLAP + window).min(n);
        let seed = next.start;
        let (ids, queries) = reseed(&tracks, seed);
        if ids.is_empty() {
            break;
        }
        let result = run(tracker, next.clone(), &queries)?;
        copy_into(&mut tracks, &ids, &result, next.clone(), seed + 1..next.end);
        done = done.start..next.end;
    }
    let mut done = first;
    while done.start > 0 {
        let end = done.start + OVERLAP;
        let next = end.saturating_sub(window)..end;
        let seed = end - 1;
        let (ids, queries) = reseed(&tracks, seed);
        if ids.is_empty() {
            break;
        }
        let result = run(tracker, next.clone(), &queries)?;
        copy_into(&mut tracks, &ids, &result, next.clone(), next.start..seed);
        done = next.start..done.end;
    }
    Ok(tracks)
}

/// The points seen on `frame`, as queries there, with their indices.
fn reseed(tracks: &[PointTrack], frame: usize) -> (Vec<usize>, Vec<Query>) {
    tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.seen[frame])
        .map(|(i, t)| {
            (
                i,
                Query {
                    frame: frame as u32,
                    at: t.at[frame],
                },
            )
        })
        .unzip()
}

/// Runs the tracker on one window and checks what it gave.
fn run(
    tracker: &mut dyn FollowPoints,
    frames: Range<usize>,
    queries: &[Query],
) -> Result<Vec<PointTrack>> {
    let result = tracker.follow(frames.clone(), queries)?;
    let len = frames.len();
    if result.len() != queries.len()
        || result
            .iter()
            .any(|t| t.at.len() != len || t.seen.len() != len)
    {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "the point tracker gave {} tracks for {} queries, not each of {len} frames",
                result.len(),
                queries.len()
            ),
        ));
    }
    Ok(result)
}

/// Copies the frames `keep` of a window's `result` (over `frames`) into
/// the tracks `ids`.
fn copy_into(
    tracks: &mut [PointTrack],
    ids: &[usize],
    result: &[PointTrack],
    frames: Range<usize>,
    keep: Range<usize>,
) {
    for (&id, got) in ids.iter().zip(result) {
        for f in keep.clone() {
            tracks[id].at[f] = got.at[f - frames.start];
            tracks[id].seen[f] = got.seen[f - frames.start];
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/track/follow.rs"]
mod tests;
