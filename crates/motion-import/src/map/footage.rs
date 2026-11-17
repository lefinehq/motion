//! Footage files: which ones a scene draws as image layers ([D41]) or
//! video layers, or plays as audio layers, and why it draws the others as
//! nothing. Finding a file on
//! disk and reading which codec a clip is in are the caller's job, since
//! the import reads no files; it hands over what it found in
//! [`Options::footage`](super::Options::footage),
//! [`Options::videos`](super::Options::videos),
//! [`Options::sounds`](super::Options::sounds) and
//! [`Options::vectors`](super::Options::vectors).
//!
//! [D41]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d41-image-layers-name-their-file-and-carry-their-box

use std::collections::BTreeMap;

use super::{Sound, Vector, Video};
use crate::model::Footage;

/// The file extensions of the still pictures an image layer draws.
pub const STILL_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// What one footage file becomes.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Media<'a> {
    /// A still picture, found at this path for the scene's `image`.
    Still(&'a str),
    /// Vector footage that was read, found at this path for the scene's
    /// `image` ([D70]).
    ///
    /// [D70]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines
    Vector(&'a str, &'a Vector),
    /// Vector footage found in a format it could not read, by the format's
    /// name.
    VectorFormat(&'a str),
    /// A clip in a codec with a decoder, found at this path for the
    /// scene's `video`.
    Clip(&'a str),
    /// A clip found in a codec without a decoder, by the codec's name.
    Codec(&'a str),
    /// A sound in a format with a decoder, found at this path for the
    /// scene's `audio`.
    Sound(&'a str),
    /// A sound found in a format without a decoder, by the format's name.
    SoundFormat(&'a str),
    /// Something the scene cannot draw yet, and why, as the report says it.
    Unsupported(&'static str),
}

/// The footage files found on disk, by the path the project stores, and
/// what the caller read of the clips and sounds among them.
#[derive(Debug, Clone, Copy)]
pub(super) struct Files<'a> {
    /// The path a layer names for each file found.
    pub found: &'a BTreeMap<String, String>,
    /// What each found clip is.
    pub videos: &'a BTreeMap<String, Video>,
    /// What each found sound is.
    pub sounds: &'a BTreeMap<String, Sound>,
    /// What each found vector footage file is.
    pub vectors: &'a BTreeMap<String, Vector>,
}

/// What the footage `footage`, stored at `stored`, becomes, given the
/// `files` found.
pub(super) fn media<'a>(footage: &Footage, stored: &str, files: Files<'a>) -> Media<'a> {
    let Files {
        found,
        videos,
        sounds,
        vectors,
    } = files;
    // Vector footage is known by what the caller read, whatever its name.
    if let (Some(path), Some(vector), false) = (
        found.get(stored),
        vectors.get(stored),
        footage.duration > 0.0,
    ) {
        return match vector.reads {
            true => Media::Vector(path, vector),
            false => Media::VectorFormat(&vector.format),
        };
    }
    let still = extension(stored).is_some_and(|extension| {
        STILL_EXTENSIONS
            .iter()
            .any(|still| extension.eq_ignore_ascii_case(still))
    });
    // Stills last no time; anything that does is a clip or a sequence.
    match (
        footage.width == 0 || footage.height == 0,
        footage.duration > 0.0,
        still,
    ) {
        // Footage without a picture is a sound.
        (true, _, _) => match (found.get(stored), sounds.get(stored)) {
            (None, _) => Media::Unsupported("audio footage it cannot find"),
            (Some(path), Some(sound)) if sound.decodes => Media::Sound(path),
            (Some(_), Some(sound)) => Media::SoundFormat(&sound.format),
            (Some(_), None) => Media::Unsupported("audio footage in a format it cannot read"),
        },
        (false, true, true) => Media::Unsupported("image sequences"),
        (false, true, false) => match (found.get(stored), videos.get(stored)) {
            (None, _) => Media::Unsupported("video footage it cannot find"),
            (Some(path), Some(video)) if video.decodes => Media::Clip(path),
            (Some(_), Some(video)) => Media::Codec(&video.codec),
            (Some(_), None) => Media::Unsupported("video footage"),
        },
        (false, false, false) => Media::Unsupported("footage in a format it cannot read"),
        (false, false, true) => match found.get(stored) {
            Some(path) => Media::Still(path),
            None => Media::Unsupported("image footage it cannot find"),
        },
    }
}

/// The part of vector footage's page an image layer shows over a box the
/// footage's size, `[x, y, width, height]`: the page, or its art bounds,
/// whichever, rounded out to whole units, is that size ([D70]). `None` when
/// it is the page exactly, which the scene shows by default; an error when
/// neither is, and the page is stretched over the box.
///
/// [D70]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines
pub(super) fn vector_region(footage: &Footage, vector: &Vector) -> Result<Option<[f64; 4]>, ()> {
    let size = [f64::from(footage.width), f64::from(footage.height)];
    let [width, height] = vector.page;
    let page = [0.0, 0.0, width, height];
    let rounded_out = |[x, y, w, h]: [f64; 4]| {
        let (x0, y0) = (x.floor(), y.floor());
        let (x1, y1) = ((x + w).ceil(), (y + h).ceil());
        [x0, y0, x1 - x0, y1 - y0]
    };
    let fits = |[_, _, w, h]: [f64; 4]| (w - size[0]).abs() < 0.5 && (h - size[1]).abs() < 0.5;
    if fits(page) && page.iter().all(|v| v.fract() == 0.0) {
        return Ok(None);
    }
    [Some(page), vector.art]
        .into_iter()
        .flatten()
        .map(rounded_out)
        .find(|&region| fits(region))
        .map(Some)
        .ok_or(())
}

/// The last part of a stored path, whichever separator the machine that
/// stored it used.
pub fn file_name(stored: &str) -> &str {
    stored.rsplit(['/', '\\']).next().unwrap_or(stored)
}

/// The extension of a stored path's file name, without its dot.
fn extension(stored: &str) -> Option<&str> {
    file_name(stored)
        .rsplit_once('.')
        .map(|(_, extension)| extension)
}

#[cfg(test)]
#[path = "../../tests/unit/map/footage.rs"]
mod tests;
