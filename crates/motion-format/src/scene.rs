//! The typed scene model the compiler consumes, and [`Document`], which keeps
//! it with the JSON it was read from.

use crate::Result;
use serde_json::Value as Json;

use crate::anim::AnimTrack;
use crate::parse::scene_from_value;
use crate::preset::{PresetSpec, lookup, names};
use crate::property::Prop;
use crate::shorthand::{Behaviour, Font, Position, RectShape};
use crate::units::{Color, Size, Time, TimeSpan};
use crate::value::Value;

/// A whole scene: a canvas, a length and the layers drawn on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// Format version; always 1 for now.
    pub version: u32,
    /// Canvas size in pixels.
    pub size: Size,
    /// Frames per second, above zero.
    pub fps: f64,
    /// Total length, as the author wrote it.
    pub duration: Time,
    /// Canvas colour behind every layer.
    pub background: Color,
    /// The scene's own presets, which hide built-ins of the same name.
    pub presets: Vec<(String, PresetSpec)>,
    /// Layers in paint order: the first is at the back.
    pub layers: Vec<Layer>,
}

impl Scene {
    /// Finds a preset by name: the scene's own first, then the built-ins.
    pub fn preset(&self, name: &str) -> Option<&PresetSpec> {
        lookup(&self.presets, name)
    }

    /// Every usable preset name, the scene's own first, without duplicates.
    pub fn preset_names(&self) -> Vec<&str> {
        names(&self.presets)
    }
}

/// One thing on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Unique name, used to address the layer in edits.
    pub id: String,
    /// What the layer draws.
    pub content: Content,
    /// Fill colour (the text colour for text layers).
    pub fill: Color,
    /// Where the layer's anchor sits, in its parent's box or on the canvas:
    /// its centre, unless `anchor` moves it.
    pub at: Position,
    /// Resting values set on the layer, by property, in the order written.
    pub values: Vec<(Prop, Value)>,
    /// Entrance behaviour; JSON key `in`.
    pub enter: Option<Behaviour>,
    /// Free-form animation tracks.
    pub anim: Vec<AnimTrack>,
    /// The layer it moves with: its position, rotation and scale apply on top
    /// of the parent's.
    pub parent: Option<String>,
    /// When it shows, on its composition's timeline; always when `None`.
    pub span: Option<TimeSpan>,
    /// Where its own time 0 falls on its composition's timeline. Its keys and
    /// its entrance are in its own time, so moving it is one edit.
    pub offset: Time,
    /// How slowly its own time runs: 2 plays at half speed; a negative value
    /// runs backwards from `offset`, so its keys play before `offset`.
    pub stretch: f64,
}

impl Layer {
    /// The resting value it sets for `prop`, if any.
    pub fn value(&self, prop: Prop) -> Option<Value> {
        self.values
            .iter()
            .find(|(p, _)| *p == prop)
            .map(|(_, v)| *v)
    }

    /// Where `prop` rests on this layer: the value it sets, else the
    /// registry's rest, on a canvas of `canvas` pixels for a box of `size`.
    pub fn rest(&self, prop: Prop, canvas: (f64, f64), size: (f64, f64)) -> Value {
        self.value(prop)
            .unwrap_or_else(|| prop.spec().rest(&self.at, canvas, size))
    }
}

/// What a layer draws.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// A line of text.
    Text {
        /// The words to draw.
        text: String,
        /// Family, size and weight.
        font: Font,
    },
    /// A rectangle, possibly with rounded corners.
    Rect(RectShape),
}

/// A parsed scene file: the typed scene and the JSON it came from, kept so
/// that writing it back preserves what the author wrote.
///
/// [`Document::from_value`] is the only constructor, so the scene is always
/// the validated reading of the raw value: layer `i` of one is layer `i` of the other.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    raw: Json,
    scene: Scene,
}

impl Document {
    /// Validates `raw` and keeps it with the [`Scene`] it describes.
    ///
    /// # Errors
    /// A [`Diagnostic`](crate::Diagnostic) for the first part of `raw` that is
    /// not a valid scene, at its path.
    pub fn from_value(raw: Json) -> Result<Document> {
        let scene = scene_from_value(&raw)?;
        Ok(Document { raw, scene })
    }

    /// The JSON as read.
    pub fn raw(&self) -> &Json {
        &self.raw
    }

    /// The validated scene the JSON describes.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The JSON as read, dropping the scene.
    pub fn into_raw(self) -> Json {
        self.raw
    }
}
