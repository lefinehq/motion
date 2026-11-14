//! Reading a scene document: JSON text to a validated [`Scene`].
//!
//! Part of step 1 (Read). The text is parsed as JSON, deserialized into the
//! document types of `crate::doc` (serde, with the path of every error
//! tracked by `serde_path_to_error`), then checked and typed by
//! `crate::validate`. This module only joins the steps and maps serde's
//! errors to a [`Diagnostic`].

use crate::{Diagnostic, Kind, Result};
use serde_json::Value;
use tracing::{Span, debug, field, instrument};

use crate::doc::{Form, Object, SceneDoc};
use crate::{Document, Scene, validate};

/// The spelling of every scene and layer key, the one place code writes it.
/// [`SCENE_KEYS`] and [`LAYER_KEYS`] list these, the rules in
/// `crate::validate` place their errors with them, and the writer and the
/// edit crate find keys with them, so no code can name a key the format does
/// not have.
pub mod key {
    /// The format version.
    pub const MOTION: &str = "motion";
    /// The canvas size.
    pub const SIZE: &str = "size";
    /// Frames per second.
    pub const FPS: &str = "fps";
    /// The scene's length.
    pub const DUR: &str = "dur";
    /// The background colour.
    pub const BG: &str = "bg";
    /// The scene's own presets.
    pub const PRESETS: &str = "presets";
    /// The layers.
    pub const LAYERS: &str = "layers";
    /// A layer's id.
    pub const ID: &str = "id";
    /// The words of a text layer.
    pub const TEXT: &str = "text";
    /// The shape of a rect layer.
    pub const RECT: &str = "rect";
    /// The font of a text layer.
    pub const FONT: &str = "font";
    /// A layer's fill colour.
    pub const FILL: &str = "fill";
    /// A layer's position.
    pub const AT: &str = "at";
    /// A layer's entrance.
    pub const IN: &str = "in";
    /// A layer's tracks.
    pub const ANIM: &str = "anim";

    /// The path of the layer at `index`, as diagnostics write it:
    /// `layers[2]`. Validation places a layer's errors under it, and the
    /// edit crate compares error paths with it, so the two cannot drift
    /// apart. It matches the paths serde reports for the same layer.
    pub fn layer_path(index: usize) -> String {
        format!("{LAYERS}[{index}]")
    }
}

/// The keys allowed at the top level of a scene, in the order of
/// `SceneDoc`'s fields; a unit test keeps the two in step.
pub const SCENE_KEYS: [&str; 7] = [
    key::MOTION,
    key::SIZE,
    key::FPS,
    key::DUR,
    key::BG,
    key::PRESETS,
    key::LAYERS,
];
/// The keys allowed on a layer, in the order of `LayerDoc`'s fields; a unit
/// test keeps the two in step.
pub const LAYER_KEYS: [&str; 8] = [
    key::ID,
    key::TEXT,
    key::RECT,
    key::FONT,
    key::FILL,
    key::AT,
    key::IN,
    key::ANIM,
];

/// Parses scene JSON text into a [`Document`]: the original value (kept so
/// edits and writes preserve what the author wrote) plus the typed scene.
#[instrument(level = "debug", skip_all, fields(bytes = json.len(), layers = field::Empty))]
pub fn parse_document(json: &str) -> Result<Document> {
    let raw: Value = serde_json::from_str(json)
        .map_err(|e| Diagnostic::syntax(format!("invalid JSON: {e}")).expected(SceneDoc::FORM))?;
    let doc = Document::from_value(raw)?;
    Span::current().record("layers", doc.scene().layers.len());
    Ok(doc)
}

/// Validates a JSON value and converts it into a typed [`Scene`], applying
/// defaults for the keys that may be left out. Outside this crate a scene
/// is only ever read as part of a [`Document`], through
/// [`Document::from_value`].
pub(crate) fn scene_from_value(value: &Value) -> Result<Scene> {
    let checked = serde_path_to_error::deserialize(value)
        .map_err(structural)
        .and_then(|Object::<SceneDoc>(doc)| validate::scene(&doc));
    // Validation stops at the first rule that fails, so this is that rule.
    checked.inspect_err(|d| {
        debug!(kind = ?d.kind, path = d.path.as_deref(), "rejected: {}", d.message);
    })
}

/// Serde's error messages, one row per shape: how the message starts, and
/// the kind it is. Anything else (a custom message) is [`Kind::Invalid`].
const SHAPES: [(&str, Kind); 5] = [
    ("invalid type: ", Kind::WrongType),
    ("invalid value: ", Kind::Invalid),
    ("invalid length ", Kind::Invalid),
    ("unknown field ", Kind::UnknownName),
    // The document types require no key in serde's terms (`validate`
    // reports a missing key, with an example), so this row only keeps the
    // kind right should one ever be required.
    ("missing field ", Kind::Missing),
];

/// What serde writes between its message and the accepted form.
const EXPECTED_SEPARATOR: &str = ", expected ";

/// Maps a structural error from deserializing a [`SceneDoc`] to a
/// [`Diagnostic`] at the path where it happened (none at the top level).
///
/// serde_json exposes no structured kind for these errors (`classify()` says
/// `Data` for all of them), so this is the one place that reads serde's
/// text. The kind comes from the message's shape ([`SHAPES`]), and the part
/// after the last `, expected ` is the accepted form. The last, because the
/// author's value is quoted before the form and may itself contain the
/// separator, while no form of ours does. The form runs to the end of the
/// text because an error from deserializing a `Value`, unlike one from
/// text, has no ` at line … column …` suffix.
fn structural(error: serde_path_to_error::Error<serde_json::Error>) -> Diagnostic {
    let path = error.path().to_string();
    let at_root = error.path().iter().len() == 0;
    let text = error.into_inner().to_string();
    let kind = SHAPES
        .iter()
        .find(|(start, _)| text.starts_with(start))
        .map_or(Kind::Invalid, |(_, kind)| *kind);
    let diagnostic = match text.rsplit_once(EXPECTED_SEPARATOR) {
        // Serde lists names in backticks (`one of `a`, `b``);
        // `Diagnostic::unknown` lists them plain, so every list reads alike.
        Some((message, expected)) if kind == Kind::UnknownName => {
            Diagnostic::new(kind, message).expected(expected.replace('`', ""))
        }
        Some((message, expected)) => Diagnostic::new(kind, message).expected(expected.to_owned()),
        None => Diagnostic::new(kind, text),
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
