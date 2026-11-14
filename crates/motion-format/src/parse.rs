//! Reading a scene: JSON text, then the document types of `crate::doc`, then
//! the validated [`Scene`]; and the spelling of every key.

use crate::{Diagnostic, Result};
use serde_json::Value;
use tracing::{Span, debug, field, instrument};

use crate::doc::{Form, Object, SceneDoc};
use crate::{Document, Scene, de, validate};

/// The spelling of every scene and layer key, so no code can name a key the
/// format does not have.
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
    /// The scene's compositions.
    pub const COMPS: &str = "comps";
    /// The font files and folders a scene uses.
    pub const FONTS: &str = "fonts";
    /// The layers.
    pub const LAYERS: &str = "layers";
    /// A layer's id.
    pub const ID: &str = "id";
    /// The words of a text layer.
    pub const TEXT: &str = "text";
    /// The shape of a rect layer.
    pub const RECT: &str = "rect";
    /// The items of a shape layer.
    pub const SHAPES: &str = "shapes";
    /// The composition a layer draws.
    pub const COMP: &str = "comp";
    /// A layer that draws nothing, for others to move with.
    pub const PIVOT: &str = "pivot";
    /// The font of a text layer.
    pub const FONT: &str = "font";
    /// A layer's fill colour.
    pub const FILL: &str = "fill";
    /// A text layer's style.
    pub const STYLE: &str = "style";
    /// A text layer's styled character ranges.
    pub const RUNS: &str = "runs";
    /// A text layer's paragraph styles.
    pub const PARAGRAPHS: &str = "paragraphs";
    /// A text layer's box, for box text.
    pub const BOX: &str = "box";
    /// A text layer's animators.
    pub const ANIMATE: &str = "animate";
    /// A layer's position.
    pub const AT: &str = "at";
    /// A layer's entrance.
    pub const IN: &str = "in";
    /// A layer's tracks.
    pub const ANIM: &str = "anim";
    /// The layer a layer moves with.
    pub const PARENT: &str = "parent";
    /// When a layer shows.
    pub const SPAN: &str = "span";
    /// Where a layer's own time starts.
    pub const OFFSET: &str = "offset";
    /// How slowly a layer's own time runs.
    pub const STRETCH: &str = "stretch";
    /// A layer's effects.
    pub const FX: &str = "fx";
    /// Left out of the frame, yet kept in the scene.
    pub const HIDDEN: &str = "hidden";

    /// The path of composition `id`, `comps.card`.
    pub fn comp_path(id: &str) -> String {
        format!("{COMPS}.{id}")
    }

    /// The path of the layer at `index`, `layers[2]`, as serde and validation write it.
    pub fn layer_path(index: usize) -> String {
        format!("{LAYERS}[{index}]")
    }
}

/// The keys allowed at the top level, in the order of `SceneDoc`'s fields.
pub const SCENE_KEYS: [&str; 9] = [
    key::MOTION,
    key::SIZE,
    key::FPS,
    key::DUR,
    key::BG,
    key::PRESETS,
    key::FONTS,
    key::COMPS,
    key::LAYERS,
];
/// The keys allowed on a composition, in the order of `CompDoc`'s fields.
pub const COMP_KEYS: [&str; 5] = [key::SIZE, key::FPS, key::DUR, key::BG, key::LAYERS];
/// The keys allowed on a layer besides property names, in the order of
/// `LayerDoc`'s fields. A property in the registry is also a layer key,
/// holding its resting value; [`layer_keys`] lists both.
pub const LAYER_KEYS: [&str; 22] = [
    key::ID,
    key::TEXT,
    key::RECT,
    key::SHAPES,
    key::COMP,
    key::PIVOT,
    key::FONT,
    key::FILL,
    key::STYLE,
    key::RUNS,
    key::PARAGRAPHS,
    key::BOX,
    key::ANIMATE,
    key::AT,
    key::IN,
    key::ANIM,
    key::PARENT,
    key::SPAN,
    key::OFFSET,
    key::STRETCH,
    key::FX,
    key::HIDDEN,
];

/// Every key a layer may hold: [`LAYER_KEYS`], then the names of the
/// properties a layer sets by name (not `position`, which `at` sets).
pub fn layer_keys() -> impl Iterator<Item = &'static str> + Clone {
    use crate::property::{PROPERTIES, Rest};
    let by_name = PROPERTIES
        .iter()
        .filter(|spec| spec.rest != Rest::At)
        .map(|spec| spec.name);
    LAYER_KEYS.into_iter().chain(by_name)
}

/// Parses scene JSON text into a [`Document`].
///
/// # Errors
/// A [`Diagnostic`] for text that is not JSON or not a valid scene.
#[instrument(level = "debug", skip_all, fields(bytes = json.len(), layers = field::Empty))]
pub fn parse_document(json: &str) -> Result<Document> {
    let raw: Value = serde_json::from_str(json)
        .map_err(|e| Diagnostic::syntax(format!("invalid JSON: {e}")).expected(SceneDoc::FORM))?;
    let doc = Document::from_value(raw)?;
    Span::current().record("layers", doc.scene().layers.len());
    Ok(doc)
}

/// Validates a JSON value into a typed [`Scene`], with the defaults of the keys left out.
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
