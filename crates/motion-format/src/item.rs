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
    /// Moves, scales, turns, fades or recolours the parts of a text its
    /// selectors choose, by as much as they choose them.
    Animator,
    /// Chooses how much of each character, word or line an animator applies to.
    Selector,
}

/// What an item's kind key holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primary {
    /// Items of a table: a group's shapes, an animator's selectors.
    Items(&'static [ItemKind]),
    /// The resting value of the property with this name.
    Property(&'static str),
    /// The choice of the setting with this name.
    Setting(&'static str),
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
    /// What the kind's key holds.
    pub primary: Primary,
    /// Its fixed settings.
    pub settings: &'static [SettingSpec],
    /// What it draws or does, for people and agents.
    pub doc: &'static str,
}

impl ItemKind {
    /// The property its key holds, if it holds one.
    pub fn primary_prop(&'static self) -> Option<Prop> {
        match self.primary {
            Primary::Property(name) => {
                Some(self.registry.prop(name).expect("the primary is a property"))
            }
            Primary::Items(_) | Primary::Setting(_) => None,
        }
    }

    /// The index of the setting its key holds, if it holds one.
    pub fn primary_setting(&self) -> Option<usize> {
        match self.primary {
            Primary::Setting(name) => {
                Some(table::position(self.settings, name).expect("the primary is a setting"))
            }
            Primary::Items(_) | Primary::Property(_) => None,
        }
    }

    /// The table its key's items come from, if it holds items.
    pub fn items(&self) -> Option<&'static [ItemKind]> {
        match self.primary {
            Primary::Items(kinds) => Some(kinds),
            Primary::Property(_) | Primary::Setting(_) => None,
        }
    }

    /// The name of the property or setting its key holds, if any.
    pub fn primary_name(&self) -> Option<&'static str> {
        match self.primary {
            Primary::Property(name) | Primary::Setting(name) => Some(name),
            Primary::Items(_) => None,
        }
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

const fn color(name: &'static str, rest: [f64; 4], doc: &'static str) -> PropertySpec {
    PropertySpec {
        name,
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
        color("color", WHITE, "the colour inside the outlines"),
        opacity("how opaque the fill is, 0 to 1"),
    ],
};

/// The properties of a stroke.
pub static STROKE: Registry = Registry {
    kind: "stroke",
    rows: &[
        color("color", WHITE, "the colour along the outlines"),
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
        color(
            "color",
            WHITE,
            "the colour every drawn pixel of the layer takes",
        ),
        opacity("how much of the colour shows over the layer, 0 to 1"),
    ],
};

/// Every kind of shape item.
pub static SHAPES: [ItemKind; 6] = [
    ItemKind {
        name: "group",
        role: Role::Group,
        registry: &GROUP,
        primary: Primary::Items(&SHAPES),
        settings: &[],
        doc: "items under one transform and opacity; paints in it draw the outlines above them, \
              in it and in its groups",
    },
    ItemKind {
        name: "rect",
        role: Role::Geometry,
        registry: &RECT,
        primary: Primary::Property("size"),
        settings: &[],
        doc: "a rectangle centred on `position`, with rounded corners",
    },
    ItemKind {
        name: "ellipse",
        role: Role::Geometry,
        registry: &ELLIPSE,
        primary: Primary::Property("size"),
        settings: &[],
        doc: "an ellipse centred on `position`",
    },
    ItemKind {
        name: "path",
        role: Role::Geometry,
        registry: &PATH,
        primary: Primary::Property("path"),
        settings: &[],
        doc: "a bézier outline, open or closed",
    },
    ItemKind {
        name: "fill",
        role: Role::Paint,
        registry: &FILL,
        primary: Primary::Property("color"),
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
        primary: Primary::Property("color"),
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
    primary: Primary::Property("color"),
    settings: &[],
    doc: "turns every drawn pixel of the layer one colour, keeping its coverage",
}];

const fn point(
    name: &'static str,
    rest: [f64; 2],
    components: &'static [&'static str],
    broadcast: bool,
    doc: &'static str,
) -> PropertySpec {
    PropertySpec {
        name,
        ty: ValueType::Point,
        rest: Rest::Value(Value::Point(rest)),
        components,
        range: None,
        broadcast,
        spatial: false,
        doc,
    }
}

/// What a text animator applies to the parts its selectors choose: each
/// part takes the value scaled by how much it is chosen. An animator
/// applies only the properties it sets or animates.
pub static ANIMATOR: Registry = Registry {
    kind: "animator",
    rows: &[
        point(
            "position",
            [0.0, 0.0],
            &["x", "y"],
            false,
            "how far each part moves, in px",
        ),
        point(
            "anchor",
            [0.0, 0.0],
            &["anchor-x", "anchor-y"],
            false,
            "how far each part's own anchor moves from the middle of its baseline, in px",
        ),
        point(
            "scale",
            [1.0, 1.0],
            &["scale-x", "scale-y"],
            true,
            "the size factor each part grows to; one number scales both axes",
        ),
        number(
            "rotate",
            0.0,
            None,
            "how far each part turns about its anchor, in degrees clockwise",
        ),
        number(
            "opacity",
            1.0,
            Some((0.0, 1.0)),
            "the opacity each part fades to, 0 to 1",
        ),
        color("fill", WHITE, "the colour each part's fill turns to"),
        color("stroke", WHITE, "the colour each part's stroke turns to"),
        number(
            "stroke-width",
            0.0,
            Some((0.0, f64::INFINITY)),
            "the stroke width each part's stroke turns to, in px",
        ),
        number(
            "tracking",
            0.0,
            None,
            "extra space after each part, in thousandths of an em; the parts after it on its line move along",
        ),
    ],
};

/// The properties of a range selector. Start, end and offset are fractions
/// of the parts it counts: 0 is before the first, 1 after the last.
pub static RANGE: Registry = Registry {
    kind: "range",
    rows: &[
        number(
            "start",
            0.0,
            None,
            "where the range starts, as a fraction of the parts",
        ),
        number(
            "end",
            1.0,
            None,
            "where the range ends, as a fraction of the parts",
        ),
        number(
            "offset",
            0.0,
            Some((-1.0, 1.0)),
            "how far start and end both move, as a fraction of the parts",
        ),
        number(
            "amount",
            1.0,
            Some((-1.0, 1.0)),
            "how strongly the chosen parts are chosen; negative reverses the animator",
        ),
        number(
            "smooth",
            1.0,
            Some((0.0, 1.0)),
            "for the `square` shape: 1 chooses a part by how much of it the range covers, \
             0 all or nothing by its middle",
        ),
    ],
};

/// Every kind of text animator.
pub static ANIMATORS: [ItemKind; 1] = [ItemKind {
    name: "animator",
    role: Role::Animator,
    registry: &ANIMATOR,
    primary: Primary::Items(&SELECTORS),
    settings: &[],
    doc: "applies its properties to the parts of the text its selectors choose",
}];

/// Every kind of text selector.
pub static SELECTORS: [ItemKind; 1] = [ItemKind {
    name: "range",
    role: Role::Selector,
    registry: &RANGE,
    primary: Primary::Setting("by"),
    settings: &[
        SettingSpec {
            name: "by",
            choices: &["char", "char-no-space", "word", "line"],
            default: "char",
            doc: "what it counts: characters, characters other than spaces, words or lines",
        },
        SettingSpec {
            name: "shape",
            choices: &[
                "square",
                "ramp-up",
                "ramp-down",
                "triangle",
                "round",
                "smooth",
            ],
            default: "square",
            doc: "how the choice rises and falls across the range",
        },
        SettingSpec {
            name: "mode",
            choices: &["add", "subtract", "intersect", "min", "max", "difference"],
            default: "add",
            doc: "how it combines with the selectors before it",
        },
    ],
    doc: "chooses the parts between `start` and `end`, moved by `offset`",
}];

/// The shape item kind called `name`, if there is one.
pub fn shape_kind(name: &str) -> Option<&'static ItemKind> {
    table::lookup(&SHAPES, name)
}

/// The text animator kind called `name`, if there is one.
pub fn animator_kind(name: &str) -> Option<&'static ItemKind> {
    table::lookup(&ANIMATORS, name)
}

/// The effect kind called `name`, if there is one.
pub fn effect_kind(name: &str) -> Option<&'static ItemKind> {
    table::lookup(&EFFECTS, name)
}

/// The properties code depends on, for the drawing and measuring of items.
pub mod props {
    use super::{ANIMATOR, ELLIPSE, FILL, FILL_EFFECT, PATH, Prop, RANGE, RECT, STROKE};

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
    /// How far an animator moves each part.
    pub const ANIMATOR_POSITION: Prop = Prop::of(&ANIMATOR, 0);
    /// How far an animator moves each part's anchor.
    pub const ANIMATOR_ANCHOR: Prop = Prop::of(&ANIMATOR, 1);
    /// The size factor an animator grows each part to.
    pub const ANIMATOR_SCALE: Prop = Prop::of(&ANIMATOR, 2);
    /// How far an animator turns each part.
    pub const ANIMATOR_ROTATE: Prop = Prop::of(&ANIMATOR, 3);
    /// The opacity an animator fades each part to.
    pub const ANIMATOR_OPACITY: Prop = Prop::of(&ANIMATOR, 4);
    /// The fill colour an animator turns each part to.
    pub const ANIMATOR_FILL: Prop = Prop::of(&ANIMATOR, 5);
    /// The stroke colour an animator turns each part to.
    pub const ANIMATOR_STROKE: Prop = Prop::of(&ANIMATOR, 6);
    /// The stroke width an animator turns each part to.
    pub const ANIMATOR_STROKE_WIDTH: Prop = Prop::of(&ANIMATOR, 7);
    /// The tracking an animator adds after each part.
    pub const ANIMATOR_TRACKING: Prop = Prop::of(&ANIMATOR, 8);
    /// Where a range selector starts.
    pub const RANGE_START: Prop = Prop::of(&RANGE, 0);
    /// Where a range selector ends.
    pub const RANGE_END: Prop = Prop::of(&RANGE, 1);
    /// How far a range selector's range moves.
    pub const RANGE_OFFSET: Prop = Prop::of(&RANGE, 2);
    /// How strongly a range selector chooses.
    pub const RANGE_AMOUNT: Prop = Prop::of(&RANGE, 3);
    /// How smooth a square range selector's edges are.
    pub const RANGE_SMOOTH: Prop = Prop::of(&RANGE, 4);
}

#[cfg(test)]
#[path = "../tests/unit/item.rs"]
mod tests;
