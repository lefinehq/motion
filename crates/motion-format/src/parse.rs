//! Reading a scene document: JSON text to a validated [`Scene`].
//!
//! Part of step 1 (Read). The text is parsed as JSON, deserialized into the
//! document types of `crate::doc` by `crate::de`, then checked and typed by
//! `crate::validate`. This module only joins the steps.

use crate::{Diagnostic, Result};
use serde_json::Value;
use tracing::{Span, debug, field, instrument};

use crate::doc::{Form, Object, SceneDoc};
use crate::{Document, Scene, de, validate};

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
    let checked = de::from_value(value).and_then(|Object::<SceneDoc>(doc)| validate::scene(&doc));
    // Validation stops at the first rule that fails, so this is that rule.
    checked.inspect_err(|d| {
        debug!(kind = ?d.kind, path = d.path.as_deref(), "rejected: {}", d.message);
    })
}

#[cfg(test)]
#[path = "../tests/unit/parse.rs"]
mod tests;
