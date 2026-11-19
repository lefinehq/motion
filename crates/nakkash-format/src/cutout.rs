//! What a video layer asks a bake for ([D85]): a [`Cutout`] of its object,
//! and [`TrackRequest`]s for the motion of objects in its clip. Each keeps
//! the digest of what was baked for it; [`Cutout::request`] and
//! [`TrackRequest::request`] are what a baked asset must answer.
//!
//! [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::address::Id;
use crate::doc::{Form, List, Object};
use crate::{Diagnostic, Kind, Result, Sha256};

/// A video layer's own object, cut out of its clip ([D87]): the prompts
/// that say which object, how its edges are made, and the digest of its
/// baked matte once there is one.
///
/// [D87]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d87-an-objects-matte-is-its-segmentation-refined-at-its-edge
#[derive(Debug, Clone, PartialEq)]
pub struct Cutout {
    /// Where the object is, on frames of the clip; at least one.
    pub prompts: Vec<Prompt>,
    /// How its edges are made.
    pub edges: Edges,
    /// How far either side of the segmentation's edge the edge matting
    /// reaches, in clip px.
    pub band: f64,
    /// How well the edge matting's mask must agree with the segmentation,
    /// as an IoU, for a frame to take its edge.
    pub agree: f64,
    /// Smoothing along the clip's motion, when asked for.
    pub smooth: Option<Smooth>,
    /// The digest of the baked matte, once baked.
    pub baked: Option<Sha256>,
}

/// How a cutout's edges are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Edges {
    /// The segmentation, its edge refined by the edge matting model where
    /// the two agree.
    #[default]
    Refine,
    /// The segmentation's mask, each pixel in or out.
    Hard,
    /// The segmentation's probability.
    Soft,
}

impl Edges {
    /// The names a scene writes, in the order of the variants.
    pub const NAMES: [&str; 3] = ["refine", "hard", "soft"];

    /// The edges `name` spells, one of [`NAMES`](Self::NAMES).
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "refine" => Some(Self::Refine),
            "hard" => Some(Self::Hard),
            "soft" => Some(Self::Soft),
            _ => None,
        }
    }

    /// Its name, as a scene writes it.
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }
}

/// Smoothing of a matte along the clip's motion: the previous frame's
/// matte, carried along the flow, blended into each frame's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smooth {
    /// How much of the carried matte a frame takes where the two agree,
    /// from 0 to 1.
    pub amount: f64,
    /// The difference in alpha over which that share falls off; above 0.
    pub tolerance: f64,
}

/// Where an object is on one frame of a clip, in clip pixels: a box around
/// it, points on it, points off it.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    /// The clip frame, counted from 0 in the order the frames show.
    pub frame: u32,
    /// A box around the object, `[x0, y0, x1, y1]`.
    pub bbox: Option<[f64; 4]>,
    /// Points on the object.
    pub include: Vec<[f64; 2]>,
    /// Points that are not the object.
    pub exclude: Vec<[f64; 2]>,
}

/// A track of one object in a video layer's clip ([D88]).
///
/// [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRequest {
    /// Its id, unique among the layer's tracks.
    pub id: String,
    /// Where the object is, on frames of the clip; at least one.
    pub prompts: Vec<Prompt>,
    /// The object's corners on the first prompted frame, in clip pixels:
    /// top left, top right, bottom right, bottom left. Found from its mask
    /// when not given.
    pub corners: Option<[[f64; 2]; 4]>,
    /// Corrections of its corners on later frames.
    pub anchors: Vec<Anchor>,
    /// The digest of the baked track, once baked.
    pub baked: Option<Sha256>,
}

/// The corners of a tracked object on one frame, where it is seen, that
/// correct the drift of its track.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    /// The clip frame.
    pub frame: u32,
    /// The corners there, in clip pixels: top left, top right, bottom
    /// right, bottom left.
    pub corners: [[f64; 2]; 4],
}

impl Cutout {
    /// The request a baked matte answers: every setting, defaults filled
    /// in, without `baked`, in a fixed order. Bake and render both compare
    /// this, so it is built only here.
    pub fn request(&self) -> Value {
        json!({
            "prompts": self.prompts.iter().map(Prompt::request).collect::<Vec<_>>(),
            "edges": self.edges.name(),
            "band": self.band,
            "agree": self.agree,
            "smooth": self.smooth.map(|s| json!({"amount": s.amount, "tolerance": s.tolerance})),
        })
    }

    /// The first prompted frame, where propagation starts.
    pub fn first_frame(&self) -> u32 {
        first_frame(&self.prompts)
    }
}

impl TrackRequest {
    /// The request a baked track answers: as [`Cutout::request`].
    pub fn request(&self) -> Value {
        json!({
            "id": self.id,
            "prompts": self.prompts.iter().map(Prompt::request).collect::<Vec<_>>(),
            "corners": self.corners,
            "anchors": self
                .anchors
                .iter()
                .map(|a| json!({"frame": a.frame, "corners": a.corners}))
                .collect::<Vec<_>>(),
        })
    }

    /// The first prompted frame, where scale is 1 and rotation 0.
    pub fn first_frame(&self) -> u32 {
        first_frame(&self.prompts)
    }
}

impl Prompt {
    fn request(&self) -> Value {
        json!({
            "frame": self.frame,
            "box": self.bbox,
            "include": self.include,
            "exclude": self.exclude,
        })
    }
}

/// The smallest frame of `prompts`, which validation keeps non-empty.
fn first_frame(prompts: &[Prompt]) -> u32 {
    prompts.iter().map(|p| p.frame).min().unwrap_or(0)
}

/// The default `band`, in clip px.
const BAND: f64 = 12.0;
/// The default `agree`.
const AGREE: f64 = 0.9;
/// The default `smooth.tolerance`.
const TOLERANCE: f64 = 0.08;

/// A `cutout` as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CutoutDoc {
    prompts: Option<List<Vec<Object<PromptDoc>>>>,
    edges: Option<String>,
    band: Option<f64>,
    agree: Option<f64>,
    smooth: Option<Object<SmoothDoc>>,
    baked: Option<String>,
}

impl Form for CutoutDoc {
    const FORM: &str = r#"an object of prompts and settings, e.g. {"prompts":[{"frame":0,"box":[690,470,1210,735]}]}"#;
}

/// A `smooth` as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SmoothDoc {
    amount: Option<f64>,
    tolerance: Option<f64>,
}

impl Form for SmoothDoc {
    const FORM: &str = r#"an object, e.g. {"amount":0.6,"tolerance":0.08}"#;
}

/// A prompt as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptDoc {
    frame: Option<u32>,
    #[serde(rename = "box")]
    bbox: Option<[f64; 4]>,
    #[serde(default)]
    include: Vec<[f64; 2]>,
    #[serde(default)]
    exclude: Vec<[f64; 2]>,
}

impl Form for PromptDoc {
    const FORM: &str = r#"a prompt, e.g. {"frame":0,"box":[690,470,1210,735]} or {"frame":0,"include":[[900,600]]}"#;
}

impl Form for Vec<Object<PromptDoc>> {
    const FORM: &str = PROMPTS_EXPECTED;
}

/// A track as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackDoc {
    id: Option<String>,
    prompts: Option<List<Vec<Object<PromptDoc>>>>,
    corners: Option<[[f64; 2]; 4]>,
    #[serde(default)]
    anchors: Vec<Object<AnchorDoc>>,
    baked: Option<String>,
}

impl Form for TrackDoc {
    const FORM: &str =
        r#"a track, e.g. {"id":"screen","prompts":[{"frame":0,"box":[700,480,1200,720]}]}"#;
}

impl Form for Vec<Object<TrackDoc>> {
    const FORM: &str = r#"a list of tracks, e.g. "tracks":[{"id":"screen","prompts":[{"frame":0,"box":[700,480,1200,720]}]}]"#;
}

/// An anchor as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnchorDoc {
    frame: Option<u32>,
    corners: Option<[[f64; 2]; 4]>,
}

impl Form for AnchorDoc {
    const FORM: &str =
        r#"an anchor, e.g. {"frame":36,"corners":[[640,470],[1120,470],[1118,700],[642,702]]}"#;
}

/// The example of a whole `prompts` list.
const PROMPTS_EXPECTED: &str =
    r#"at least one prompt, e.g. "prompts":[{"frame":0,"box":[690,470,1210,735]}]"#;

/// Reads a layer's `cutout`, with errors placed inside it.
pub(crate) fn cutout(value: &Value) -> Result<Cutout> {
    let Object(doc) = crate::de::from_value::<Object<CutoutDoc>>(value)?;
    let prompts = prompts(doc.prompts)?;
    let edges = match doc.edges.as_deref() {
        None => Edges::default(),
        Some(name) => Edges::named(name).ok_or_else(|| {
            Diagnostic::unknown("edges", name, Edges::NAMES.map(|n| format!("`{n}`"))).at("edges")
        })?,
    };
    let band = doc.band.unwrap_or(BAND);
    if band <= 0.0 {
        return Err(out_of_range("band", band, "a width above 0 px, e.g. 12"));
    }
    let agree = doc.agree.unwrap_or(AGREE);
    if !(0.0..=1.0).contains(&agree) {
        return Err(out_of_range("agree", agree, "an IoU from 0 to 1, e.g. 0.9"));
    }
    let smooth = doc
        .smooth
        .map(|Object(s)| {
            let amount = s.amount.unwrap_or(0.6);
            if !(0.0..=1.0).contains(&amount) {
                return Err(out_of_range(
                    "smooth.amount",
                    amount,
                    "a share from 0 to 1, e.g. 0.6",
                ));
            }
            let tolerance = s.tolerance.unwrap_or(TOLERANCE);
            if tolerance <= 0.0 {
                return Err(out_of_range(
                    "smooth.tolerance",
                    tolerance,
                    "an alpha difference above 0, e.g. 0.08",
                ));
            }
            Ok(Smooth { amount, tolerance })
        })
        .transpose()?;
    Ok(Cutout {
        prompts,
        edges,
        band,
        agree,
        smooth,
        baked: digest(doc.baked.as_deref())?,
    })
}

/// Reads a layer's `tracks`, with errors placed inside it.
pub(crate) fn tracks(value: &Value) -> Result<Vec<TrackRequest>> {
    let List(docs) = crate::de::from_value::<List<Vec<Object<TrackDoc>>>>(value)?;
    let mut ids = HashSet::new();
    docs.into_iter()
        .enumerate()
        .map(|(i, Object(doc))| {
            let at = format!("[{i}]");
            let track = track(doc).map_err(|e| e.under(at.as_str()))?;
            if !ids.insert(track.id.clone()) {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("two tracks are called `{}`", track.id),
                )
                .at(format!("{at}.id"))
                .expected("a different id for each track of the layer"));
            }
            Ok(track)
        })
        .collect()
}

/// One track.
fn track(doc: TrackDoc) -> Result<TrackRequest> {
    let id = doc.id.ok_or_else(|| {
        Diagnostic::new(Kind::Missing, "a track needs an `id`")
            .expected(r#"an id for the track, e.g. "id":"screen""#)
    })?;
    id.parse::<Id>().map_err(|e| e.at("id"))?;
    let prompts = prompts(doc.prompts)?;
    let first = first_frame(&prompts);
    let mut frames = HashSet::new();
    let anchors = doc
        .anchors
        .into_iter()
        .enumerate()
        .map(|(i, Object(anchor))| {
            let at = format!("anchors[{i}]");
            let frame = anchor.frame.ok_or_else(|| {
                Diagnostic::new(Kind::Missing, "an anchor needs a `frame`")
                    .at(at.as_str())
                    .expected(AnchorDoc::FORM)
            })?;
            let corners = anchor.corners.ok_or_else(|| {
                Diagnostic::new(Kind::Missing, "an anchor needs its `corners`")
                    .at(at.as_str())
                    .expected(AnchorDoc::FORM)
            })?;
            if frame == first {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("an anchor on frame {frame}, the track's first prompted frame"),
                )
                .at(format!("{at}.frame"))
                .expected("an anchor on a later or earlier frame; the first prompted frame takes `corners`"));
            }
            if !frames.insert(frame) {
                return Err(Diagnostic::new(Kind::Conflict, format!("two anchors on frame {frame}"))
                    .at(format!("{at}.frame"))
                    .expected("one anchor a frame"));
            }
            in_clip(corners.iter().flatten().copied(), &format!("{at}.corners"))?;
            Ok(Anchor { frame, corners })
        })
        .collect::<Result<Vec<_>>>()?;
    in_clip(doc.corners.iter().flatten().flatten().copied(), "corners")?;
    Ok(TrackRequest {
        id,
        prompts,
        corners: doc.corners,
        anchors,
        baked: digest(doc.baked.as_deref())?,
    })
}

/// The prompts of a cutout or a track: at least one, each with a box or a
/// point on the object, no two on one frame.
fn prompts(docs: Option<List<Vec<Object<PromptDoc>>>>) -> Result<Vec<Prompt>> {
    let docs = match docs {
        Some(List(docs)) if !docs.is_empty() => docs,
        _ => {
            return Err(
                Diagnostic::new(Kind::Missing, "no prompt says where the object is")
                    .at("prompts")
                    .expected(PROMPTS_EXPECTED),
            );
        }
    };
    let mut frames = HashSet::new();
    docs.into_iter()
        .enumerate()
        .map(|(i, Object(doc))| {
            let at = format!("prompts[{i}]");
            let frame = doc.frame.ok_or_else(|| {
                Diagnostic::new(Kind::Missing, "a prompt needs the `frame` it is on")
                    .at(at.as_str())
                    .expected(PromptDoc::FORM)
            })?;
            if doc.bbox.is_none() && doc.include.is_empty() {
                return Err(Diagnostic::new(
                    Kind::Missing,
                    "a prompt needs a `box` or a point to `include`",
                )
                .at(at.as_str())
                .expected(PromptDoc::FORM));
            }
            if let Some([x0, y0, x1, y1]) = doc.bbox
                && !(x0 < x1 && y0 < y1)
            {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    "a box whose first corner is not above and left of its second",
                )
                .at(format!("{at}.box"))
                .expected("`[x0, y0, x1, y1]` with x0 < x1 and y0 < y1, e.g. [690,470,1210,735]"));
            }
            in_clip(doc.bbox.iter().flatten().copied(), &format!("{at}.box"))?;
            in_clip(
                doc.include.iter().flatten().copied(),
                &format!("{at}.include"),
            )?;
            in_clip(
                doc.exclude.iter().flatten().copied(),
                &format!("{at}.exclude"),
            )?;
            if !frames.insert(frame) {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("two prompts on frame {frame}"),
                )
                .at(format!("{at}.frame"))
                .expected("one prompt a frame, with every box and point of that frame"));
            }
            Ok(Prompt {
                frame,
                bbox: doc.bbox,
                include: doc.include,
                exclude: doc.exclude,
            })
        })
        .collect()
}

/// Checks that every coordinate of `values`, at `at`, lies in the clip's
/// pixels, which start at 0. JSON numbers are always finite.
fn in_clip(mut values: impl Iterator<Item = f64>, at: &str) -> Result<()> {
    match values.find(|v| *v < 0.0) {
        None => Ok(()),
        Some(v) => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("{} is left of or above the clip", crate::fmt_num(v)),
        )
        .at(at)
        .expected("coordinates in the clip's pixels, from 0 at its top-left corner")),
    }
}

/// A `baked` digest, when written.
fn digest(text: Option<&str>) -> Result<Option<Sha256>> {
    text.map(|t| {
        t.parse::<Sha256>().map_err(|e| {
            e.at("baked")
                .expected("the 64 hexadecimal digits a bake writes; leave it out to bake again")
        })
    })
    .transpose()
}

/// A number outside the range `key` takes.
fn out_of_range(key: &str, value: f64, expected: &'static str) -> Diagnostic {
    Diagnostic::new(
        Kind::OutOfRange,
        format!("`{}` is out of range", crate::fmt_num(value)),
    )
    .at(key)
    .expected(expected)
}

#[cfg(test)]
#[path = "../tests/unit/cutout.rs"]
mod tests;
