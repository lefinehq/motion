//! Reading a scene document: JSON text to a validated [`Scene`].
//!
//! Part of step 1 (Read). The text is parsed as JSON, deserialized into the
//! document types of `crate::doc` (serde, with the path of every error
//! tracked by `serde_path_to_error`), then checked and typed by
//! `crate::validate`. This module only joins the steps and maps serde's
//! errors to a [`Diagnostic`].

use motion_diagnostic::{Diagnostic, Kind, Result};
use serde_json::Value;

use crate::doc::{Object, ObjectForm, SceneDoc};
use crate::{Document, Scene, validate};

/// The keys allowed at the top level of a scene, in the order of
/// `SceneDoc`'s fields; a unit test keeps the two in step.
pub const SCENE_KEYS: [&str; 7] = ["motion", "size", "fps", "dur", "bg", "presets", "layers"];
/// The keys allowed on a layer, in the order of `LayerDoc`'s fields; a unit
/// test keeps the two in step.
pub const LAYER_KEYS: [&str; 8] = ["id", "text", "rect", "font", "fill", "at", "in", "anim"];

/// Parses scene JSON text into a [`Document`]: the original value (kept so
/// edits and writes preserve what the author wrote) plus the typed scene.
pub fn parse_document(json: &str) -> Result<Document> {
    let raw: Value = serde_json::from_str(json)
        .map_err(|e| Diagnostic::syntax(format!("invalid JSON: {e}")).expected(SceneDoc::FORM))?;
    let scene = scene_from_value(&raw)?;
    Ok(Document { raw, scene })
}

/// Validates a JSON value and converts it into a typed [`Scene`], applying
/// defaults for the keys that may be left out.
pub fn scene_from_value(value: &Value) -> Result<Scene> {
    let Object::<SceneDoc>(doc) = serde_path_to_error::deserialize(value).map_err(structural)?;
    validate::scene(&doc, value)
}

/// The kinds of serde's structural errors, by how their message starts.
const KINDS: [(&str, Kind); 3] = [
    ("invalid type:", Kind::WrongType),
    ("unknown field", Kind::UnknownName),
    ("missing field", Kind::Missing),
];

/// Maps a structural error from deserializing a [`SceneDoc`] to a
/// [`Diagnostic`] at the path where it happened (none at the top level).
///
/// serde_json exposes no structured kind for these errors (`classify()` says
/// `Data` for all of them), so this is the one place that reads serde's
/// text. Serde's messages have fixed shapes: `invalid type: <found>,
/// expected <form>`, `unknown field `<key>`, expected <keys>`, `missing
/// field `<key>``, `invalid length …` and `invalid value …`. The kind comes
/// from how the message starts, and the part after the last `, expected ` is
/// the accepted form.
fn structural(error: serde_path_to_error::Error<serde_json::Error>) -> Diagnostic {
    let path = error.path().to_string();
    let at_root = error.path().iter().len() == 0;
    let text = error.into_inner().to_string();
    let kind = KINDS
        .iter()
        .find(|(start, _)| text.starts_with(start))
        .map_or(Kind::Invalid, |(_, kind)| *kind);
    let diagnostic = match text.rsplit_once(", expected ") {
        Some((message, expected)) => Diagnostic::new(kind, message).expected(expected.to_owned()),
        // A missing field has no expected part; the form is the key itself.
        None => match text.strip_prefix("missing field ") {
            Some(key) => {
                let expected = format!("the key {key}");
                Diagnostic::new(kind, text).expected(expected)
            }
            None => Diagnostic::new(kind, text),
        },
    };
    if at_root {
        diagnostic
    } else {
        diagnostic.at(path)
    }
}

#[cfg(test)]
#[path = "../tests/unit/parse.rs"]
mod tests;
