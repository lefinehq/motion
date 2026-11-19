//! [`Track`]: the baked motion of one object in a clip.

use serde::{Deserialize, Serialize};

use crate::Asset;

/// A track ([D88]): per clip frame, what the object's mask and its points
/// say about where it is. `nakkash ml track` turns it into keys; a render
/// never reads it.
///
/// [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    /// The first prompted frame, where scale is 1 and rotation 0.
    pub prompt_frame: u32,
    /// The object's four corners on the prompted frame, in clip pixels:
    /// top left, top right, bottom right, bottom left.
    pub corners: [[f64; 2]; 4],
    /// The frames of the anchors that corrected it, in order.
    pub anchors: Vec<u32>,
    /// When each clip frame starts to show, in seconds into the clip, in
    /// the order they show: where its keys go.
    pub times: Vec<f64>,
    /// One entry per clip frame, in the order they show.
    pub frames: Vec<TrackFrame>,
    /// The points followed, each from the frame it was chosen on.
    pub points: Vec<PointTrack>,
}

impl Asset for Track {
    const KIND: &'static str = "track";
}

/// What one frame of a [`Track`] holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackFrame {
    /// The mask's measures, or `None` when the object is not seen.
    pub mask: Option<MaskStats>,
    /// The fit to the points, or `None` when too few are seen.
    pub fit: Option<Fit>,
}

/// Measures of the object's mask on one frame, in clip pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskStats {
    /// The centroid of the mask.
    pub centroid: [f64; 2],
    /// Its area in pixels.
    pub area: f64,
    /// The square root of its area over the prompted frame's.
    pub scale: f64,
    /// The angle of its principal axis in degrees, clockwise from the x
    /// axis, between −90 and 90.
    pub angle: f64,
}

/// The fit of one frame's visible points to the prompted frame's.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fit {
    /// Where the similarity takes the prompted frame's mask centroid.
    pub position: [f64; 2],
    /// The similarity's scale.
    pub scale: f64,
    /// The similarity's rotation in degrees, clockwise.
    pub rotate: f64,
    /// The corners, through the homography when its guard passed, else
    /// through the similarity.
    pub corners: [[f64; 2]; 4],
    /// Whether the guard refused the homography on this frame.
    pub guarded: bool,
    /// The homography's inliers.
    pub inliers: u32,
    /// The points seen on both frames.
    pub points: u32,
}

/// One followed point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointTrack {
    /// The frame it was chosen on.
    pub frame: u32,
    /// Where it is on each clip frame, in clip pixels.
    pub at: Vec<[f64; 2]>,
    /// Whether it is seen on each clip frame.
    pub seen: Vec<bool>,
}
