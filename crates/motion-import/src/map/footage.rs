//! Footage files: which ones a scene draws as image layers ([D41]) or
//! video layers, or plays as audio layers, and why it draws the others as
//! nothing. Finding a file on
//! disk and reading which codec a clip is in are the caller's job, since
//! the import reads no files; it hands over what it found in
//! [`Options::footage`](super::Options::footage),
//! [`Options::videos`](super::Options::videos) and
//! [`Options::sounds`](super::Options::sounds).
//!
//! [D41]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d41-image-layers-name-their-file-and-carry-their-box

use std::collections::BTreeMap;

use super::{Sound, Video};
use crate::model::Footage;

/// The file extensions of the still pictures an image layer draws.
pub const STILL_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// What one footage file becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Media<'a> {
    /// A still picture, found at this path for the scene's `image`.
    Still(&'a str),
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
}

/// What the footage `footage`, stored at `stored`, becomes, given the
/// `files` found.
pub(super) fn media<'a>(footage: &Footage, stored: &str, files: Files<'a>) -> Media<'a> {
    let Files {
        found,
        videos,
        sounds,
    } = files;
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
