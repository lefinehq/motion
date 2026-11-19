//! Writing a baked track into a scene as keys on pivots ([D88]).
//!
//! [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots

use nakkash_baked::{Manifest, Track};
use nakkash_format::key::{ANIM, COMPS, ID, LAYERS, OFFSET, PARENT, PIVOT, STRETCH};
use nakkash_format::{Address, Content, Diagnostic, Document, Id, Kind, Layer, Result, fmt_num};
use serde_json::{Map, Value, json};

/// The key a video layer's time remapping is written under.
const REMAP: &str = "remap";
/// The suffixes of the four corner pivots, in the order of a track's
/// corners.
const CORNERS: [&str; 4] = ["tl", "tr", "br", "bl"];

/// Where a track goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PivotRequest {
    /// The video layer the track was baked from, by its address.
    pub video: Address,
    /// The id of the pivot that takes the keys, made when the layer's list
    /// has none.
    pub pivot: String,
    /// Whether to write the four corner pivots too.
    pub corners: bool,
}

/// What [`apply`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The pivots written, each with how many keys it took.
    pub pivots: Vec<(String, usize)>,
    /// The ids of pivots made, of those written; the others were updated.
    pub made: Vec<String>,
}

/// Writes `track` into `scene`, the scene's JSON as read: on the pivot
/// `request` names, a sibling of the video layer and its child, with the
/// video's `offset` and `stretch` so their times agree, one linear key per
/// clip frame at the time the layer shows it. `position` and `scale` come
/// from the mask, `rotate` from the point fit, and with
/// [`PivotRequest::corners`] the four corner pivots `<pivot>-tl`, `-tr`,
/// `-br` and `-bl` take the guarded corners. Positions are in the video
/// layer's own coordinates: clip pixels placed in its `box` as its `fit`
/// places the frame. A frame where the mask or the fit is missing gets no
/// key, so the keys around it interpolate.
///
/// `scene` changes only when the result is a valid scene.
///
/// # Errors
/// A [`Kind::Missing`] diagnostic when the address names no layer, a
/// [`Kind::Conflict`] one when it is not a video layer with a clip, when
/// the video remaps its time, or when the pivot's id is a layer that is not
/// a pivot; a [`Kind::Invalid`] one when the track is not of the clip's
/// frames, the pivot's id is not an id, or the result does not validate.
pub fn apply(
    scene: &mut Value,
    request: &PivotRequest,
    track: &Manifest<Track>,
) -> Result<Applied> {
    request.pivot.parse::<Id>()?;
    let document = Document::from_value(scene.clone())?;
    let (video, list) = locate(&document, &request.video)?;
    let Content::Video(content) = &video.content else {
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!("`{}` is not a video layer", video.id),
        )
        .expected("the address of a video layer whose track was baked"));
    };
    if content.path.is_none() {
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!("`{}` has no clip", video.id),
        ));
    }
    let frames = &track.content.frames;
    if frames.len() != track.content.times.len() {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "a track of {} frames with {} times",
                frames.len(),
                track.content.times.len()
            ),
        ));
    }
    let [width, height] = track.size.map(f64::from);
    let fitted = content
        .fit
        .place(kurbo::Rect::new(0.0, 0.0, width, height), content.frame);
    // Clip pixel centres sit at whole numbers, half a pixel in from the
    // frame's edge.
    let place = |[x, y]: [f64; 2]| {
        [
            round(
                fitted.rect.x0
                    + (x + 0.5 - fitted.shown.x0) * fitted.rect.width() / fitted.shown.width(),
            ),
            round(
                fitted.rect.y0
                    + (y + 0.5 - fitted.shown.y0) * fitted.rect.height() / fitted.shown.height(),
            ),
        ]
    };
    let times = &track.content.times;
    let key = |t: f64, v: Value| json!({"t": seconds(t), "v": v});

    let mut written = scene.clone();
    let layers = layers_mut(&mut written, &list)?;
    let at = layers
        .iter()
        .position(|l| l.get(ID).and_then(Value::as_str) == Some(video.id.as_str()))
        .ok_or_else(|| missing(&request.video))?;
    let source = layers[at].as_object().cloned().unwrap_or_default();
    let remaps = source.contains_key(REMAP)
        || source
            .get(ANIM)
            .and_then(Value::as_object)
            .is_some_and(|a| a.contains_key(REMAP));
    if remaps {
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!(
                "`{}` remaps its time, so its frames do not each show at one time",
                video.id
            ),
        )
        .expected("a video layer without `remap`"));
    }

    let mut position = Vec::new();
    let mut scale = Vec::new();
    for (frame, &t) in frames.iter().zip(times) {
        if let Some(mask) = &frame.mask {
            position.push(key(t, json!(place(mask.centroid))));
            scale.push(key(t, json!([factor(mask.scale), factor(mask.scale)])));
        }
    }
    let mut rotate = Vec::new();
    let mut last: Option<f64> = None;
    for (frame, &t) in frames.iter().zip(times) {
        if let Some(fit) = &frame.fit {
            // Unwrapped, so a turn past ±180° keeps going instead of
            // spinning back.
            let angle = match last {
                Some(prev) => prev + (fit.rotate - prev + 180.0).rem_euclid(360.0) - 180.0,
                None => fit.rotate,
            };
            last = Some(angle);
            rotate.push(key(t, json!(round(angle))));
        }
    }
    let mut tracks = vec![(
        request.pivot.clone(),
        vec![("position", position), ("scale", scale), ("rotate", rotate)],
    )];
    if request.corners {
        for (i, suffix) in CORNERS.iter().enumerate() {
            let keys = frames
                .iter()
                .zip(times)
                .filter_map(|(f, &t)| {
                    f.fit
                        .as_ref()
                        .map(|fit| key(t, json!(place(fit.corners[i]))))
                })
                .collect();
            tracks.push((
                format!("{}-{suffix}", request.pivot),
                vec![("position", keys)],
            ));
        }
    }

    let mut applied = Applied {
        pivots: Vec::new(),
        made: Vec::new(),
    };
    let mut insert_at = at + 1;
    for (id, anim) in tracks {
        let count = anim.iter().map(|(_, k)| k.len()).max().unwrap_or(0);
        let existing = layers
            .iter()
            .position(|l| l.get(ID).and_then(Value::as_str) == Some(id.as_str()));
        let mut layer = match existing {
            Some(i) => match layers[i].get(PIVOT) == Some(&Value::Bool(true)) {
                true => layers[i].as_object().cloned().unwrap_or_default(),
                false => {
                    return Err(Diagnostic::new(
                        Kind::Conflict,
                        format!("`{id}` is a layer that is not a pivot"),
                    )
                    .expected("the id of a pivot, or of no layer yet"));
                }
            },
            None => {
                let mut fresh = Map::new();
                fresh.insert(ID.into(), json!(id));
                fresh.insert(PIVOT.into(), json!(true));
                fresh
            }
        };
        layer.insert(PARENT.into(), json!(video.id));
        for copied in [OFFSET, STRETCH] {
            match source.get(copied) {
                Some(v) => layer.insert(copied.into(), v.clone()),
                None => layer.remove(copied),
            };
        }
        let anims = layer.entry(ANIM).or_insert_with(|| json!({}));
        let Some(anims) = anims.as_object_mut() else {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("`{id}` has an `anim` that is not an object"),
            ));
        };
        for (prop, keys) in anim {
            match keys.is_empty() {
                true => anims.remove(prop),
                false => anims.insert(prop.into(), Value::Array(keys)),
            };
        }
        match existing {
            Some(i) => layers[i] = Value::Object(layer),
            None => {
                layers.insert(insert_at, Value::Object(layer));
                insert_at += 1;
                applied.made.push(id.clone());
            }
        }
        applied.pivots.push((id, count));
    }
    Document::from_value(written.clone())?;
    *scene = written;
    Ok(applied)
}

/// The video layer `address` names, and the list it is written in: the
/// scene's layers, or a composition's, by the composition's id.
fn locate<'d>(document: &'d Document, address: &Address) -> Result<(&'d Layer, Option<String>)> {
    let scene = document.scene();
    let mut layers = &scene.root.layers;
    let mut list = None;
    let Some((last, through)) = address.layers.split_last() else {
        return Err(missing(address));
    };
    for id in through {
        let layer = layers
            .iter()
            .find(|l| l.id == id.as_ref())
            .ok_or_else(|| missing(address))?;
        let Content::Comp(comp) = &layer.content else {
            return Err(
                Diagnostic::new(Kind::Conflict, format!("`{id}` shows no composition"))
                    .expected("an address through `comp` layers to a video layer"),
            );
        };
        let inner = scene
            .comps
            .iter()
            .find(|c| c.id == comp.id)
            .ok_or_else(|| missing(address))?;
        layers = &inner.layers;
        list = Some(comp.id.clone());
    }
    let layer = layers
        .iter()
        .find(|l| l.id == last.as_ref())
        .ok_or_else(|| missing(address))?;
    Ok((layer, list))
}

/// The layer list `list` names in the JSON: the scene's, or a
/// composition's.
fn layers_mut<'v>(scene: &'v mut Value, list: &Option<String>) -> Result<&'v mut Vec<Value>> {
    let holder = match list {
        None => Some(scene),
        Some(comp) => scene.get_mut(COMPS).and_then(|c| c.get_mut(comp)),
    };
    holder
        .and_then(|h| h.get_mut(LAYERS))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Diagnostic::new(Kind::Missing, "the scene has no such layer list"))
}

/// The error for an address that names no layer.
fn missing(address: &Address) -> Diagnostic {
    let path: Vec<&str> = address.layers.iter().map(|id| id.as_ref()).collect();
    Diagnostic::new(Kind::Missing, format!("no layer `{}`", path.join("/")))
        .expected("the address of a video layer, such as `phone` or `card/phone`")
}

/// `value` rounded as scenes write numbers: at most two decimals.
fn round(value: f64) -> f64 {
    fmt_num(value).parse().unwrap_or(value)
}

/// A scale factor to four decimals: a ten-thousandth of a 500 px object
/// is 0.05 px, the track tolerance of D86, where the two decimals of
/// [`round`] would be a step of 2.5 px.
fn factor(value: f64) -> f64 {
    (value * 1e4).round() / 1e4
}

/// `t` seconds as a key's time: to the microsecond, so a key lands on its
/// frame at any frame rate.
fn seconds(t: f64) -> String {
    let text = format!("{t:.6}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    match trimmed {
        "-0" | "" => "0s".into(),
        t => format!("{t}s"),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/track/apply.rs"]
mod tests;
