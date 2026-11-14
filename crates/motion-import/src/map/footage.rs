//! Footage files: which ones a scene draws as image layers ([D41]), and
//! why it draws the others as nothing. Finding a file on disk is the
//! caller's job, since the import reads no files; it hands over what it
//! found in [`Options::footage`](super::Options::footage).
//!
//! [D41]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d41-image-layers-name-their-file-and-carry-their-box

use std::collections::BTreeMap;

use crate::model::Footage;

/// The file extensions of the still pictures an image layer draws.
pub const STILL_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// What one footage file becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Media<'a> {
    /// A still picture, found at this path for the scene's `image`.
    Still(&'a str),
    /// Something the scene cannot draw yet, and why, as the report says it.
    Unsupported(&'static str),
}

/// What the footage `footage`, stored at `stored`, becomes, given the files
/// `found` for stored paths.
pub(super) fn media<'a>(
    footage: &Footage,
    stored: &str,
    found: &'a BTreeMap<String, String>,
) -> Media<'a> {
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
        (true, _, _) => Media::Unsupported("audio footage"),
        (false, true, true) => Media::Unsupported("image sequences"),
        (false, true, false) => Media::Unsupported("video footage"),
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
