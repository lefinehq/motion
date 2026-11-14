//! Shape items and effects: the parts a shape layer draws with and the
//! effects any layer applies, each one of a kind from a table ([`SHAPES`],
//! [`EFFECTS`]). A kind has its own property [`Registry`], so its values and
//! tracks are checked like a layer's, and its fixed settings, such as a
//! stroke's cap, are names from a list.
//!
//! A scene writes an item as an object whose kind's name is a key:
//! `{"rect":[300,100],"round":12}`. That key holds the kind's primary value,
//! here the rectangle's `size`; a group's holds its items.

use crate::anim::AnimTrack;
use crate::path::Path;
use crate::property::{GROUP, Prop, PropertySpec, Registry, Rest};
use crate::table::{self, Named};
use crate::value::{Value, ValueType};

/// What an item does with the items around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Holds items of its own, under its own transform and opacity.
    Group,
    /// Adds an outline to its group, which the paints after it in the group draw.
    Geometry,
    /// Draws the outlines before it in its group and in the groups inside it.
    Paint,
    /// Changes the pixels of the layer it is on, after its content is drawn.
    Effect,
}

/// A fixed setting of an item: one name out of a list.
#[derive(Debug)]
pub struct SettingSpec {
    /// The key scene files write.
    pub name: &'static str,
    /// The names it accepts.
    pub choices: &'static [&'static str],
    /// The choice when the item leaves it out; one of `choices`.
    pub default: &'static str,
    /// What it means, for people and agents.
    pub doc: &'static str,
}

impl SettingSpec {
    /// Where `default` is in `choices`.
    pub fn default_index(&self) -> usize {
        self.choices
            .iter()
            .position(|c| *c == self.default)
            .expect("the default is a choice")
    }
}

impl Named for SettingSpec {
    fn name(&self) -> &str {
        self.name
    }
}

/// One kind of item.
#[derive(Debug)]
pub struct ItemKind {
    /// The key that makes an object an item of this kind.
    pub name: &'static str,
    /// What it does with the items around it.
    pub role: Role,
    /// Its animatable properties.
    pub registry: &'static Registry,
    /// The property the kind's key holds; `None` for a group, whose key holds its items.
    pub primary: Option<&'static str>,
    /// Its fixed settings.
    pub settings: &'static [SettingSpec],
    /// What it draws or does, for people and agents.
    pub doc: &'static str,
}

impl ItemKind {
    /// The property its key holds, if it is not a group.
    pub fn primary(&'static self) -> Option<Prop> {
        self.primary
            .map(|name| self.registry.prop(name).expect("the primary is a property"))
    }

    /// The setting called `name`, if the kind has one.
    pub fn setting(&self, name: &str) -> Option<&'static SettingSpec> {
        table::lookup(self.settings, name)
    }
}

/// A kind is a row of its table, so two kinds are equal when they are the same row.
impl PartialEq for ItemKind {
    fn eq(&self, other: &ItemKind) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for ItemKind {}

impl Named for ItemKind {
    fn name(&self) -> &str {
        self.name
    }
}

/// One shape item or effect, as a scene sets it.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Its kind.
    pub kind: &'static ItemKind,
    /// A name to address it by, unique among the layer's items.
    pub id: Option<String>,
    /// Resting values it sets, by property, the primary first.
    pub values: Vec<(Prop, Value)>,
    /// The choice of each of the kind's settings, an index into its
    /// `choices`, in the kind's order.
    pub settings: Vec<usize>,
    /// A group's items, in order: the first draws on top.
    pub items: Vec<Item>,
    /// Its tracks, against the kind's registry.
    pub anim: Vec<AnimTrack>,
    /// Left out of the frame, yet kept in the scene.
    pub hidden: bool,
}

impl Item {
    /// The resting value it sets for `prop`, if any.
    pub fn value(&self, prop: Prop) -> Option<&Value> {
        self.values.iter().find(|(p, _)| *p == prop).map(|(_, v)| v)
    }

    /// Where `prop` rests on this item: the value it sets, else the registry's.
    ///
    /// # Panics
    /// If the row rests on a layer's box, which no item property does.
    pub fn rest(&self, prop: Prop) -> Value {
        match (self.value(prop), &prop.spec().rest) {
            (Some(value), _) => value.clone(),
            (None, Rest::Value(value)) => value.clone(),
            (None, rest) => unreachable!("an item property rests at a value, not {rest:?}"),
        }
    }

    /// The choice of the setting called `name`.
    ///
    /// # Panics
    /// If its kind has no setting called `name`.
    pub fn setting(&self, name: &str) -> &'static str {
        let at = table::position(self.kind.settings, name).expect("a setting of the kind");
        self.kind.settings[at].choices[self.settings[at]]
    }
}

const fn number(
    name: &'static str,
    rest: f64,
    range: Option<(f64, f64)>,
    doc: &'static str,
) -> PropertySpec {
    PropertySpec {
        name,
        ty: ValueType::Number,
        rest: Rest::Value(Value::Number(rest)),
        components: &[],
        range,
        broadcast: false,
        spatial: false,
        doc,
    }
}

const fn opacity(doc: &'static str) -> PropertySpec {
    number("opacity", 1.0, Some((0.0, 1.0)), doc)
}

const fn color(rest: [f64; 4], doc: &'static str) -> PropertySpec {
    PropertySpec {
        name: "color",
        ty: ValueType::Color,
        rest: Rest::Value(Value::Color(rest)),
        components: &[],
        range: Some((0.0, 1.0)),
        broadcast: false,
        spatial: false,
        doc,
    }
}

const fn size(doc: &'static str) -> PropertySpec {
    PropertySpec {
        name: "size",
        ty: ValueType::Point,
        rest: Rest::Value(Value::Point([100.0, 100.0])),
        components: &["width", "height"],
        range: Some((0.0, f64::INFINITY)),
        broadcast: false,
        spatial: false,
        doc,
    }
}

const fn centre(doc: &'static str) -> PropertySpec {
    PropertySpec {
        name: "position",
        ty: ValueType::Point,
        rest: Rest::Value(Value::Point([0.0, 0.0])),
        components: &["x", "y"],
        range: None,
        broadcast: false,
        spatial: true,
        doc,
    }
}

const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];

/// The properties of a rectangle.
pub static RECT: Registry = Registry {
    kind: "rect",
    rows: &[
        size("width and height in px"),
        centre("the rectangle's centre, in px of its group"),
        number(
            "round",
            0.0,
            Some((0.0, f64::INFINITY)),
            "corner radius in px; at most half the shorter side takes effect",
        ),
    ],
};

/// The properties of an ellipse.
pub static ELLIPSE: Registry = Registry {
    kind: "ellipse",
    rows: &[
        size("width and height in px"),
        centre("the ellipse's centre, in px of its group"),
    ],
};

/// The properties of a free path.
pub static PATH: Registry = Registry {
    kind: "path",
    rows: &[PropertySpec {
        name: "path",
        ty: ValueType::Path,
        rest: Rest::Value(Value::Path(Path::EMPTY)),
        components: &[],
        range: None,
        broadcast: false,
        spatial: false,
        doc: "the outline, as SVG path data in px of its group; keys of one track \
              share their vertex count and closure",
    }],
};

/// The properties of a fill.
pub static FILL: Registry = Registry {
    kind: "fill",
    rows: &[
        color(WHITE, "the colour inside the outlines"),
        opacity("how opaque the fill is, 0 to 1"),
    ],
};

/// The properties of a stroke.
pub static STROKE: Registry = Registry {
    kind: "stroke",
    rows: &[
        color(WHITE, "the colour along the outlines"),
        opacity("how opaque the stroke is, 0 to 1"),
        number(
            "width",
            2.0,
            Some((0.0, f64::INFINITY)),
            "line width in px, centred on the outline",
        ),
        number(
            "miter",
            4.0,
            Some((1.0, f64::INFINITY)),
            "how far a `miter` join may reach, in line widths, before it is bevelled",
        ),
    ],
};

/// The properties of the fill effect.
pub static FILL_EFFECT: Registry = Registry {
    kind: "fill effect",
    rows: &[
        color(WHITE, "the colour every drawn pixel of the layer takes"),
        opacity("how much of the colour shows over the layer, 0 to 1"),
    ],
};

/// Every kind of shape item.
pub static SHAPES: [ItemKind; 6] = [
    ItemKind {
        name: "group",
        role: Role::Group,
        registry: &GROUP,
        primary: None,
        settings: &[],
        doc: "items under one transform and opacity; paints in it draw the outlines above them, \
              in it and in its groups",
    },
    ItemKind {
        name: "rect",
        role: Role::Geometry,
        registry: &RECT,
        primary: Some("size"),
        settings: &[],
        doc: "a rectangle centred on `position`, with rounded corners",
    },
    ItemKind {
        name: "ellipse",
        role: Role::Geometry,
        registry: &ELLIPSE,
        primary: Some("size"),
        settings: &[],
        doc: "an ellipse centred on `position`",
    },
    ItemKind {
        name: "path",
        role: Role::Geometry,
        registry: &PATH,
        primary: Some("path"),
        settings: &[],
        doc: "a bézier outline, open or closed",
    },
    ItemKind {
        name: "fill",
        role: Role::Paint,
        registry: &FILL,
        primary: Some("color"),
        settings: &[SettingSpec {
            name: "rule",
            choices: &["nonzero", "evenodd"],
            default: "nonzero",
            doc: "which areas count as inside where outlines cross",
        }],
        doc: "paints the inside of the outlines above it",
    },
    ItemKind {
        name: "stroke",
        role: Role::Paint,
        registry: &STROKE,
        primary: Some("color"),
        settings: &[
            SettingSpec {
                name: "cap",
                choices: &["butt", "round", "square"],
                default: "butt",
                doc: "how open ends finish",
            },
            SettingSpec {
                name: "join",
                choices: &["miter", "round", "bevel"],
                default: "miter",
                doc: "how corners finish",
            },
        ],
        doc: "paints a line along the outlines above it",
    },
];

/// Every kind of effect.
pub static EFFECTS: [ItemKind; 1] = [ItemKind {
    name: "fill",
    role: Role::Effect,
    registry: &FILL_EFFECT,
    primary: Some("color"),
    settings: &[],
    doc: "turns every drawn pixel of the layer one colour, keeping its coverage",
}];

/// The shape item kind called `name`, if there is one.
pub fn shape_kind(name: &str) -> Option<&'static ItemKind> {
    table::lookup(&SHAPES, name)
}

/// The effect kind called `name`, if there is one.
pub fn effect_kind(name: &str) -> Option<&'static ItemKind> {
    table::lookup(&EFFECTS, name)
}

/// The properties code depends on, for the drawing and measuring of items.
pub mod props {
    use super::{ELLIPSE, FILL, FILL_EFFECT, PATH, Prop, RECT, STROKE};

    /// A rectangle's width and height.
    pub const RECT_SIZE: Prop = Prop::of(&RECT, 0);
    /// A rectangle's centre.
    pub const RECT_POSITION: Prop = Prop::of(&RECT, 1);
    /// A rectangle's corner radius.
    pub const RECT_ROUND: Prop = Prop::of(&RECT, 2);
    /// An ellipse's width and height.
    pub const ELLIPSE_SIZE: Prop = Prop::of(&ELLIPSE, 0);
    /// An ellipse's centre.
    pub const ELLIPSE_POSITION: Prop = Prop::of(&ELLIPSE, 1);
    /// A free path's outline.
    pub const PATH_PATH: Prop = Prop::of(&PATH, 0);
    /// A fill's colour.
    pub const FILL_COLOR: Prop = Prop::of(&FILL, 0);
    /// A fill's opacity.
    pub const FILL_OPACITY: Prop = Prop::of(&FILL, 1);
    /// A stroke's colour.
    pub const STROKE_COLOR: Prop = Prop::of(&STROKE, 0);
    /// A stroke's opacity.
    pub const STROKE_OPACITY: Prop = Prop::of(&STROKE, 1);
    /// A stroke's width.
    pub const STROKE_WIDTH: Prop = Prop::of(&STROKE, 2);
    /// How far a stroke's miter joins reach.
    pub const STROKE_MITER: Prop = Prop::of(&STROKE, 3);
    /// The fill effect's colour.
    pub const FILL_EFFECT_COLOR: Prop = Prop::of(&FILL_EFFECT, 0);
    /// The fill effect's opacity.
    pub const FILL_EFFECT_OPACITY: Prop = Prop::of(&FILL_EFFECT, 1);
}

#[cfg(test)]
#[path = "../tests/unit/item.rs"]
mod tests;
