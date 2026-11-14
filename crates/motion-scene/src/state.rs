//! The result of evaluation: plain values, so checks and renderers never see animation.

use kurbo::{Affine, Rect};
use motion_format::item::ItemKind;
use motion_format::property::Prop;
use std::sync::Arc;

use motion_format::shorthand::RectShape;
use motion_format::{Path, Value};

use crate::animate::GlyphState;
use crate::shape::ShapeDraw;
use crate::text::TextNode;

/// One node's resolved values at a single frame.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeState {
    /// The layer id this state belongs to.
    pub id: String,
    /// One value per property, parallel to `motion_format::property::PROPERTIES`.
    pub values: Vec<Value>,
    /// From the node's own coordinates to the canvas: its transform, then its parents'.
    pub transform: Affine,
    /// Whether the frame falls in the node's span and, for a composition,
    /// in the composition's length.
    pub shown: bool,
    /// Left out of the frame; its children still move with it.
    pub hidden: bool,
    /// What it draws at this frame.
    pub content: ContentState,
    /// Its box in its own coordinates, which `anchor` rests at the centre of.
    pub own_box: Rect,
    /// The box its content covers, in its own coordinates: its box, or for
    /// shapes what they draw.
    pub local_bounds: Rect,
    /// The colour of a rect node, straight RGBA from 0 to 1.
    pub fill: [f64; 4],
    /// Its effects at this frame, in order.
    pub fx: Vec<ItemState>,
}

/// What a node draws at one frame.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentState {
    /// Text: its layout, which does not change from frame to frame, and
    /// each glyph's state in the layout's run order, empty when no animator
    /// moves it.
    Text {
        /// The words, their style and their layout.
        text: Arc<TextNode>,
        /// One per glyph under the animators, or none without them.
        glyphs: Vec<GlyphState>,
    },
    /// A rectangle the size of the node's box.
    Rect(RectShape),
    /// Paints and the outlines they cover, back to front.
    Shapes(Vec<ShapeDraw>),
    /// The composition at this index of the graph's comps, evaluated at the
    /// node's time; `None` while the node is not shown.
    Comp(usize, Option<Box<FrameState>>),
    /// Nothing.
    Pivot,
}

/// One shape item or effect at one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemState {
    /// Its kind.
    pub kind: &'static ItemKind,
    /// Its id, if the scene gave one.
    pub id: Option<String>,
    /// One value per row of the kind's registry.
    pub values: Vec<Value>,
    /// The choice of each of the kind's settings.
    pub settings: Vec<usize>,
    /// A group's items; the first draws on top.
    pub items: Vec<ItemState>,
}

impl ItemState {
    /// The value of `prop`, a row of the item's kind.
    pub fn get(&self, prop: Prop) -> &Value {
        debug_assert!(std::ptr::eq(prop.registry(), self.kind.registry));
        &self.values[prop.index()]
    }

    /// The value of a number property.
    ///
    /// # Panics
    /// If `prop` does not hold a number.
    pub fn number(&self, prop: Prop) -> f64 {
        self.get(prop).as_number().expect("a number property")
    }

    /// The value of a point property.
    ///
    /// # Panics
    /// If `prop` does not hold a point.
    pub fn point(&self, prop: Prop) -> [f64; 2] {
        self.get(prop).as_point().expect("a point property")
    }

    /// The value of a colour property, straight RGBA from 0 to 1.
    ///
    /// # Panics
    /// If `prop` does not hold a colour.
    pub fn color(&self, prop: Prop) -> [f64; 4] {
        match self.get(prop) {
            Value::Color(c) => *c,
            other => panic!("a colour property, not {other:?}"),
        }
    }

    /// The value of a path property.
    ///
    /// # Panics
    /// If `prop` does not hold a path.
    pub fn path(&self, prop: Prop) -> &Path {
        self.get(prop).as_path().expect("a path property")
    }

    /// The choice of the setting called `name`.
    ///
    /// # Panics
    /// If the kind has no setting called `name`.
    pub fn setting(&self, name: &str) -> &'static str {
        let at = self
            .kind
            .settings
            .iter()
            .position(|s| s.name == name)
            .expect("a setting of the kind");
        self.kind.settings[at].choices[self.settings[at]]
    }
}

impl NodeState {
    /// A shown node of a `width` by `height` box at `values`, which draws
    /// nothing: for states built in code, such as tests of rules.
    pub fn of_box(id: impl Into<String>, width: f64, height: f64, values: Vec<Value>) -> NodeState {
        NodeState {
            id: id.into(),
            transform: crate::local_transform(&values),
            values,
            shown: true,
            hidden: false,
            content: ContentState::Pivot,
            own_box: Rect::new(0.0, 0.0, width, height),
            local_bounds: Rect::new(0.0, 0.0, width, height),
            fill: [1.0; 4],
            fx: Vec::new(),
        }
    }

    /// Whether it draws at this frame: shown and not hidden.
    pub fn visible(&self) -> bool {
        self.shown && !self.hidden
    }

    /// The value of `prop`.
    pub fn get(&self, prop: Prop) -> &Value {
        &self.values[prop.index()]
    }

    /// The value of a number property.
    ///
    /// # Panics
    /// If `prop` does not hold a number.
    pub fn number(&self, prop: Prop) -> f64 {
        self.get(prop).as_number().expect("a number property")
    }

    /// The value of a point property.
    ///
    /// # Panics
    /// If `prop` does not hold a point.
    pub fn point(&self, prop: Prop) -> [f64; 2] {
        self.get(prop).as_point().expect("a point property")
    }

    /// The axis-aligned box the node covers on the canvas: what its content
    /// covers through its transform.
    pub fn bounds(&self) -> Rect {
        let drawn = self.transform.transform_rect_bbox(self.local_bounds);
        // Snap trig dust (cos 90° is 6e-17) so a quarter turn gives exact edges.
        let snap = |v: f64| {
            let near = v.round();
            if (v - near).abs() < 1e-9 { near } else { v }
        };
        Rect::new(
            snap(drawn.x0),
            snap(drawn.y0),
            snap(drawn.x1),
            snap(drawn.y1),
        )
    }

    /// The left edge of [`NodeState::bounds`].
    pub fn left(&self) -> f64 {
        self.bounds().x0
    }

    /// The right edge of [`NodeState::bounds`].
    pub fn right(&self) -> f64 {
        self.bounds().x1
    }

    /// The top edge of [`NodeState::bounds`].
    pub fn top(&self) -> f64 {
        self.bounds().y0
    }

    /// The bottom edge of [`NodeState::bounds`].
    pub fn bottom(&self) -> f64 {
        self.bounds().y1
    }
}

/// The state of the whole scene at one point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameState {
    /// The time evaluated, in frames.
    pub frame: f64,
    /// One state per node, in the graph's paint order.
    pub nodes: Vec<NodeState>,
}

#[cfg(test)]
#[path = "../tests/unit/state.rs"]
mod tests;
