//! The typed scene model.
//!
//! Part of step 1 (Read). These are the values the compiler consumes; they
//! are produced by [`parse_document`](crate::parse_document).

use serde_json::Value;

use crate::shorthand::{Behaviour, Font, Position, RectShape};
use crate::units::{Color, Size, Time};

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
    /// Layers in paint order: the first is at the back.
    pub layers: Vec<Layer>,
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
    /// Where the layer's centre sits.
    pub at: Position,
    /// Entrance behaviour; JSON key `in`.
    pub enter: Option<Behaviour>,
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

/// A parsed scene file: the typed scene and the JSON it came from.
///
/// The raw value is kept so that writing the document back preserves the
/// author's keys and values exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The JSON as read.
    pub raw: Value,
    /// The validated scene.
    pub scene: Scene,
}
