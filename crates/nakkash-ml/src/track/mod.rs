//! Tracks ([D88]): the motion of one object through a clip, baked from the
//! segmenter's masks and the point tracker's points, and written as keys on
//! pivots by [`apply`].
//!
//! - **The mask** of each frame gives the object's centroid, its area's
//!   scale against the prompted frame and its principal angle
//!   ([`Mask::stats`]). It holds under occlusion, so it gives position and
//!   scale.
//! - **Points** are chosen inside the mask on the prompted frame
//!   ([`points::choose`]), followed through the clip ([`follow_all`]) and
//!   fitted on each frame to where they started ([`fit::fit_frame`]): a
//!   similarity for rotation, and a guarded homography for the corners.
//! - **Anchors** correct drift: each gives the corners on a later frame,
//!   points are chosen and followed again from there, and between two
//!   anchors a frame blends the fits from both, each weighed by its
//!   inliers over the square of its distance, so the track meets every
//!   anchor exactly.
//!
//! [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots

mod apply;
pub mod fit;
mod follow;
pub mod linalg;
mod mask;
pub mod points;
mod tracker;

pub use apply::{Applied, PivotRequest, apply};
pub use follow::{FollowPoints, OVERLAP, Query, follow_all};
pub use mask::{Mask, in_polygon};
pub use tracker::{MODEL, PointTracker, SIZE, WINDOW};

use nakkash_baked::{Fit, PointTrack, Track, TrackFrame};
use nakkash_format::{Diagnostic, Kind, Result};

use crate::image::{Plane, Rgb8};
use fit::{FrameFit, Similarity};

/// What a track asks for, as the scene's track request says it.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackSpec {
    /// The first prompted frame, where scale is 1 and rotation 0.
    pub prompt_frame: u32,
    /// The object's corners on the prompted frame, in clip pixels (top
    /// left, top right, bottom right, bottom left); found from the mask
    /// when not given.
    pub corners: Option<[[f64; 2]; 4]>,
    /// Corrections on later (or earlier) frames.
    pub anchors: Vec<Anchor>,
}

/// A correction: where the object's corners are on one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    /// The clip frame.
    pub frame: u32,
    /// The corners there, in clip pixels, in the order of
    /// [`TrackSpec::corners`].
    pub corners: [[f64; 2]; 4],
}

/// The clip a track is baked from, as the segmentation left it.
pub struct TrackClip<'a> {
    /// Its frame size in pixels.
    pub size: (usize, usize),
    /// When each frame starts to show, in seconds into the clip.
    pub times: &'a [f64],
    /// The segmenter's logits for each frame (256² as it gives them).
    pub logits: &'a [Plane],
    /// Each frame's picture, at the clip's size, asked for only on the
    /// prompted frame and the anchors' frames.
    pub frame: &'a mut dyn FnMut(usize) -> Result<Rgb8>,
}

/// One anchor's view of the clip: the similarity from the prompted frame's
/// corners to its corners, and every frame's fit against it.
struct Segment {
    frame: usize,
    onto: Similarity,
    fits: Vec<Option<FrameFit>>,
}

/// Bakes the track `spec` asks for over `clip`, following points with
/// `tracker`.
///
/// # Errors
/// A [`Kind::Invalid`] diagnostic when the clip's parts disagree in length
/// or the object is not seen on its prompted frame, a [`Kind::OutOfRange`]
/// or [`Kind::Conflict`] one for an anchor outside the clip or on a frame
/// already anchored, and the tracker's or the frames' diagnostics.
pub fn bake(
    spec: &TrackSpec,
    clip: TrackClip<'_>,
    tracker: &mut dyn FollowPoints,
) -> Result<Track> {
    let n = clip.logits.len();
    let (w, h) = clip.size;
    if clip.times.len() != n || tracker.frames() != n || n == 0 {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "a track over {n} masks, {} frame times and {} tracked frames",
                clip.times.len(),
                tracker.frames()
            ),
        ));
    }
    let prompt = spec.prompt_frame as usize;
    if prompt >= n {
        return Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("prompted frame {prompt} of a clip of {n} frames"),
        ));
    }
    let first = Mask::from_logits(&clip.logits[prompt], w, h);
    let first_stats = first.stats(0.0).ok_or_else(|| {
        Diagnostic::new(
            Kind::Invalid,
            format!("the object is not seen on its prompted frame, {prompt}"),
        )
        .expected("a prompt that the segmenter finds the object from")
    })?;
    let corners = match spec.corners {
        Some(c) => c,
        None => first.corners().unwrap_or([first_stats.centroid; 4]),
    };
    let mut anchors = vec![Anchor {
        frame: spec.prompt_frame,
        corners,
    }];
    for anchor in &spec.anchors {
        if anchor.frame as usize >= n {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!(
                    "an anchor on frame {} of a clip of {n} frames",
                    anchor.frame
                ),
            ));
        }
        if anchors.iter().any(|a| a.frame == anchor.frame) {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!("two anchors on frame {}", anchor.frame),
            ));
        }
        anchors.push(*anchor);
    }
    anchors.sort_by_key(|a| a.frame);

    let mut points: Vec<PointTrack> = Vec::new();
    let mut segments = Vec::new();
    for anchor in &anchors {
        let k = anchor.frame as usize;
        let mask = match k == prompt {
            true => first.clone(),
            false => {
                let cut = Mask::from_logits(&clip.logits[k], w, h).within(&anchor.corners);
                match cut.inside.contains(&true) {
                    true => cut,
                    false => Mask {
                        width: w,
                        height: h,
                        inside: vec![true; w * h],
                    }
                    .within(&anchor.corners),
                }
            }
        };
        let picture = (clip.frame)(k)?;
        let chosen = points::choose(&picture, &mask);
        let tracks = follow_all(tracker, anchor.frame, &chosen)?;
        let fits = (0..n)
            .map(|t| segment_fit(&chosen, &tracks, t, k, &anchor.corners))
            .collect();
        let onto = match k == prompt {
            true => Similarity::IDENTITY,
            false => Similarity::fit(&corners, &anchor.corners).unwrap_or(Similarity::IDENTITY),
        };
        segments.push(Segment {
            frame: k,
            onto,
            fits,
        });
        points.extend(tracks);
    }

    let mut frames = Vec::with_capacity(n);
    for t in 0..n {
        let mask = match t == prompt {
            true => Some(first_stats),
            false => Mask::from_logits(&clip.logits[t], w, h).stats(first_stats.area),
        };
        let mask = mask.map(|mut m| {
            if t == prompt {
                m.scale = 1.0;
            }
            m
        });
        frames.push(TrackFrame {
            mask,
            fit: blended(&segments, t, first_stats.centroid),
        });
    }
    Ok(Track {
        prompt_frame: spec.prompt_frame,
        corners,
        anchors: spec
            .anchors
            .iter()
            .map(|a| a.frame)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        times: clip.times.to_vec(),
        frames,
        points,
    })
}

/// The fit of frame `t` against anchor frame `k`, from the points chosen
/// there and their tracks; exact on `k` itself.
fn segment_fit(
    chosen: &[[f64; 2]],
    tracks: &[PointTrack],
    t: usize,
    k: usize,
    corners: &[[f64; 2]; 4],
) -> Option<FrameFit> {
    if t == k {
        let seen = tracks.iter().filter(|track| track.seen[t]).count();
        return Some(FrameFit {
            similarity: Similarity::IDENTITY,
            corners: *corners,
            guarded: false,
            inliers: seen,
            points: seen,
        });
    }
    let (from, to): (Vec<[f64; 2]>, Vec<[f64; 2]>) = chosen
        .iter()
        .zip(tracks)
        .filter(|(_, track)| track.seen[t])
        .map(|(start, track)| (*start, track.at[t]))
        .unzip();
    fit::fit_frame(&from, &to, corners)
}

/// The fit of frame `t` from the anchors around it, blended by how far each
/// can be trusted there; `centroid` is the prompted frame's mask centroid,
/// which positions follow.
///
/// A point's drift grows about in step with its distance from the frame it
/// was chosen on, so a fit's error does too, and a least-squares fit's
/// variance falls as one over the points it rests on. Each anchor's fit is
/// weighed by its inliers over the square of its distance: the nearer and
/// the better-supported dominates, and each anchor is still met exactly.
fn blended(segments: &[Segment], t: usize, centroid: [f64; 2]) -> Option<Fit> {
    fn as_fit(segment: &Segment, t: usize) -> Option<(Similarity, &FrameFit)> {
        segment.fits[t]
            .as_ref()
            .map(|f| (f.similarity.after(&segment.onto), f))
    }
    let one = |(total, f): (Similarity, &FrameFit)| Fit {
        position: total.apply(centroid),
        scale: total.scale,
        rotate: total.angle.to_degrees(),
        corners: f.corners,
        guarded: f.guarded,
        inliers: f.inliers as u32,
        points: f.points as u32,
    };
    let (a, b) = match segments.iter().position(|s| s.frame > t) {
        Some(0) => (&segments[0], None),
        None => (&segments[segments.len() - 1], None),
        Some(i) => (&segments[i - 1], Some(&segments[i])),
    };
    let fa = as_fit(a, t).map(one);
    let Some(b) = b else { return fa };
    let fb = as_fit(b, t).map(one);
    match (fa, fb) {
        (Some(x), Some(y)) => {
            let (da, db) = ((t - a.frame) as f64, (b.frame - t) as f64);
            let trust = |fit: &Fit, d: f64| f64::from(fit.inliers.max(1)) / (d * d);
            let w = match da == 0.0 {
                true => 0.0,
                false => trust(&y, db) / (trust(&x, da) + trust(&y, db)),
            };
            Some(lerp_fit(&x, &y, w))
        }
        (x, y) => x.or(y),
    }
}

/// `a` moved `w` of the way to `b`; counts and the guard come from the
/// nearer one.
fn lerp_fit(a: &Fit, b: &Fit, w: f64) -> Fit {
    let mix = |x: f64, y: f64| x + (y - x) * w;
    let point = |x: [f64; 2], y: [f64; 2]| [mix(x[0], y[0]), mix(x[1], y[1])];
    let near = if w < 0.5 { a } else { b };
    // Turn the short way round.
    let turn = (b.rotate - a.rotate + 180.0).rem_euclid(360.0) - 180.0;
    Fit {
        position: point(a.position, b.position),
        scale: mix(a.scale, b.scale),
        rotate: a.rotate + turn * w,
        corners: std::array::from_fn(|i| point(a.corners[i], b.corners[i])),
        guarded: near.guarded,
        inliers: near.inliers,
        points: near.points,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/track/mod.rs"]
mod tests;
