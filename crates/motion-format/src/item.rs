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
use crate::property::{GROUP, Prop, PropertySpec, Registry, Rest, TRANSFORM_EFFECT};
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
    /// Changes the outlines before it in its group and in the groups inside
    /// it, for every paint that draws them.
    Modifier,
    /// Changes the pixels of the layer it is on, after its content is drawn.
    Effect,
    /// Moves, scales, turns, fades or recolours the parts of a text its
    /// selectors choose, by as much as they choose them.
    Animator,
    /// Chooses how much of each character, word or line an animator applies to.
    Selector,
    /// Cuts what the layer it is on draws, before its effects.
    Mask,
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
    /// The key that names another layer it reads, and what it reads of
    /// it, if it reads one: the map a displacement follows, the sound a
    /// spectrum shows.
    pub input: Option<Input>,
    /// The key that names one of the layer's masks, by its id, whose
    /// outline it follows, if it may follow one: the path a spectrum's
    /// bands stand along.
    pub outline: Option<&'static str>,
    /// What it draws or does, for people and agents.
    pub doc: &'static str,
}

/// Another layer of the same list an effect reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    /// The key that names the layer.
    pub key: &'static str,
    /// What the effect reads of it.
    pub reads: Reads,
}

/// What an effect reads of the layer its input names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reads {
    /// Its drawing, with its effects but not its transform: a layer that
    /// draws.
    Drawing,
    /// The sound it plays, with its volume: an `audio` layer, or a `comp`
    /// layer for everything its composition plays.
    Sound,
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
    /// The id of the layer its kind's input key names, in the same list.
    pub input: Option<String>,
    /// The id of the mask its kind's outline key names, among its layer's.
    pub outline: Option<String>,
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

/// The properties of a star or a polygon.
pub static STAR: Registry = Registry {
    kind: "star",
    rows: &[
        number(
            "points",
            5.0,
            Some((0.0, 100.0)),
            "how many points a star has, or corners a polygon; the whole part counts",
        ),
        centre("the shape's centre, in px of its group"),
        number(
            "rotate",
            0.0,
            None,
            "how far the shape is turned, clockwise in degrees; at 0 its first point is \
             straight up from `position`",
        ),
        number(
            "outer-radius",
            100.0,
            Some((0.0, f64::INFINITY)),
            "how far the points, or a polygon's corners, are from `position`, in px",
        ),
        number(
            "inner-radius",
            50.0,
            Some((0.0, f64::INFINITY)),
            "how far a star's inner corners, between its points, are from `position`, in px",
        ),
        number(
            "outer-roundness",
            0.0,
            None,
            "how round the points, or a polygon's corners, are: each one's handles run along \
             the circle through it, as long as this share of a quarter of the arc to the next \
             point; negative loops them inwards",
        ),
        number(
            "inner-roundness",
            0.0,
            None,
            "how round a star's inner corners are, as `outer-roundness` is for its points",
        ),
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

/// The properties of the tint effect.
pub static TINT: Registry = Registry {
    kind: "tint",
    rows: &[
        color("black", [0.0, 0.0, 0.0, 1.0], "the colour black maps to"),
        color("white", [1.0, 1.0, 1.0, 1.0], "the colour white maps to"),
        number(
            "amount",
            1.0,
            Some((0.0, 1.0)),
            "how much of the tint replaces the colours, 0 to 1",
        ),
    ],
};

/// The properties of particles.
pub static PARTICLES: Registry = Registry {
    kind: "particles",
    rows: &[
        number(
            "rate",
            40.0,
            Some((0.0, 100_000.0)),
            "how many particles are born a second",
        ),
        number(
            "longevity",
            2.0,
            Some((0.0, 3_000.0)),
            "how long each lives, in seconds",
        ),
        point(
            "position",
            [0.0, 0.0],
            &[],
            false,
            "the producer's centre, in px of the layer",
        ),
        number(
            "radius-x",
            3.0,
            Some((0.0, 10_000.0)),
            "how far across from `position` particles are born, in px",
        ),
        number(
            "radius-y",
            3.0,
            Some((0.0, 10_000.0)),
            "how far up or down from `position` particles are born, in px",
        ),
        number(
            "velocity",
            100.0,
            None,
            "how fast particles leave, in px a second",
        ),
        number(
            "gravity",
            100.0,
            None,
            "how fast they fall faster, in px a second each second; less than 0 rises",
        ),
        number(
            "resistance",
            0.0,
            Some((0.0, 1_000.0)),
            "how much of their speed they lose, a share each second",
        ),
        number(
            "direction",
            0.0,
            None,
            "where `direction` particles fly, in degrees clockwise from up",
        ),
        number(
            "extra",
            1.0,
            Some((0.0, 100.0)),
            "how far speeds and directions vary",
        ),
        number(
            "birth-size",
            4.0,
            Some((0.0, 10_000.0)),
            "how wide a particle is at birth, in px",
        ),
        number(
            "death-size",
            10.0,
            Some((0.0, 10_000.0)),
            "how wide it is at death, in px",
        ),
        number(
            "size-variation",
            0.5,
            Some((0.0, 1.0)),
            "how far sizes vary, as a share",
        ),
        number(
            "max-opacity",
            0.75,
            Some((0.0, 1.0)),
            "the most a particle shows",
        ),
        color(
            "birth-color",
            [0.98, 0.98, 0.125, 1.0],
            "its colour at birth",
        ),
        color(
            "death-color",
            [0.784, 0.063, 0.063, 1.0],
            "its colour at death",
        ),
        number(
            "seed",
            0.0,
            Some((0.0, 100_000.0)),
            "which particles they are",
        ),
    ],
};

/// The properties of the displacement map.
pub static DISPLACEMENT_MAP: Registry = Registry {
    kind: "displacement-map",
    rows: &[
        number(
            "horizontal",
            5.0,
            Some((-32_000.0, 32_000.0)),
            "the farthest a pixel moves across, in px",
        ),
        number(
            "vertical",
            5.0,
            Some((-32_000.0, 32_000.0)),
            "the farthest a pixel moves up or down, in px",
        ),
    ],
};

/// The properties every sound-reading effect shares, in order: where it
/// stands, how tall and thick it draws, the window of sound it reads, and
/// its colours.
macro_rules! sound_rows {
    ($($extra:expr),* $(,)?) => {
        &[
            point("start", [0.0, 0.0], &["start-x", "start-y"], false, "where the line starts, \
                   or the circle's centre, in px of the layer"),
            point("end", [200.0, 0.0], &["end-x", "end-y"], false, "where the line ends, or a \
                   point of the circle, in px of the layer"),
            number("height", 480.0, Some((0.0, 32_000.0)), "how far the loudest it draws \
                    reaches from the line, in px"),
            number("duration", 90.0, Some((1.0, 30_000.0)), "how much sound it reads about the \
                    layer's time, in ms"),
            number("offset", 0.0, Some((-30_000.0, 30_000.0)), "how far after the layer's time the \
                    window is centred, in ms"),
            number("thickness", 3.0, Some((0.0, 4_000.0)), "how thick a line, bar or dot is, in px"),
            number("softness", 0.5, Some((0.0, 1.0)), "how much of a line's thickness fades out \
                    at its edges, 0 to 1"),
            color("inside-color", [1.0, 1.0, 1.0, 1.0], "the colour at the line"),
            color("outside-color", [1.0, 1.0, 1.0, 1.0], "the colour at the far ends"),
            $($extra,)*
        ]
    };
}

/// The properties of `audio-spectrum`.
pub static AUDIO_SPECTRUM: Registry = Registry {
    kind: "audio-spectrum",
    rows: sound_rows!(
        number(
            "from",
            20.0,
            Some((1.0, 48_000.0)),
            "the lowest frequency, in Hz"
        ),
        number(
            "to",
            2_000.0,
            Some((1.0, 48_000.0)),
            "the highest frequency, in Hz"
        ),
        number(
            "bands",
            64.0,
            Some((1.0, 4_096.0)),
            "how many frequencies it shows"
        ),
        number(
            "hue",
            0.0,
            None,
            "how far the hue turns from the first band to the last, in \
                degrees"
        ),
    ),
};

/// The properties of `audio-waveform`.
pub static AUDIO_WAVEFORM: Registry = Registry {
    kind: "audio-waveform",
    rows: sound_rows!(number(
        "samples",
        32.0,
        Some((2.0, 4_096.0)),
        "how many points of the wave it draws"
    )),
};

/// The properties of `motion-tile`.
pub static MOTION_TILE: Registry = Registry {
    kind: "motion-tile",
    rows: &[
        point(
            "center",
            [0.0, 0.0],
            &[],
            false,
            "where a tile is centred, in px of the layer",
        ),
        number(
            "tile-width",
            1.0,
            Some((0.0, 1.0)),
            "a tile's width, as a share of the layer's box",
        ),
        number(
            "tile-height",
            1.0,
            Some((0.0, 1.0)),
            "a tile's height, as a share of the layer's box",
        ),
        number(
            "output-width",
            1.0,
            Some((0.0, 300.0)),
            "how wide the tiles reach, as a share of the box, about its middle",
        ),
        number(
            "output-height",
            1.0,
            Some((0.0, 300.0)),
            "how high the tiles reach, as a share of the box, about its middle",
        ),
        number(
            "phase",
            0.0,
            None,
            "how far every other column or row shifts, in degrees; a turn is a tile",
        ),
    ],
};

/// The properties of `wrap`.
pub static WRAP: Registry = Registry {
    kind: "wrap",
    rows: &[
        point(
            "to",
            [0.0, 0.0],
            &[],
            false,
            "where the middle of the layer's box moves to, in px of the layer",
        ),
        number(
            "blend",
            0.0,
            Some((0.0, 1.0)),
            "how much of the layer stays as it was",
        ),
    ],
};

/// The properties of `turbulent-displace`.
pub static TURBULENT_DISPLACE: Registry = Registry {
    kind: "turbulent-displace",
    rows: &[
        number(
            "amount",
            50.0,
            Some((-10_000.0, 10_000.0)),
            "how far pixels move, in percent of `size`",
        ),
        number(
            "size",
            100.0,
            Some((2.0, 1_000.0)),
            "how wide the noise's cells are, in px",
        ),
        point(
            "offset",
            [0.0, 0.0],
            &[],
            false,
            "where the noise is centred, in px of the layer",
        ),
        number(
            "complexity",
            1.0,
            Some((1.0, 10.0)),
            "how many octaves the noise sums",
        ),
        number(
            "evolution",
            0.0,
            None,
            "how far the noise has evolved, in degrees; a turn is a cell",
        ),
        number("seed", 0.0, Some((0.0, 100_000.0)), "which noise it is"),
    ],
};

/// The properties of `mirror`.
pub static MIRROR: Registry = Registry {
    kind: "mirror",
    rows: &[
        point(
            "center",
            [0.0, 0.0],
            &[],
            false,
            "a point of the line it reflects across, in px of the layer",
        ),
        number(
            "angle",
            0.0,
            None,
            "which way the reflected side faces, in degrees clockwise: at 0 the \
                line is upright and the right side shows the left",
        ),
    ],
};

/// The properties of `polar-coordinates`.
pub static POLAR_COORDINATES: Registry = Registry {
    kind: "polar-coordinates",
    rows: &[number(
        "interpolation",
        0.0,
        Some((0.0, 1.0)),
        "how far each pixel goes towards where the conversion takes it",
    )],
};

/// The properties of `repeat-tile`.
pub static REPEAT_TILE: Registry = Registry {
    kind: "repeat-tile",
    rows: &[
        number(
            "right",
            0.0,
            Some((0.0, 100_000.0)),
            "how far the tiles reach right of the layer's box, in px of the layer",
        ),
        number(
            "left",
            0.0,
            Some((0.0, 100_000.0)),
            "how far the tiles reach left of the layer's box, in px of the layer",
        ),
        number(
            "down",
            0.0,
            Some((0.0, 100_000.0)),
            "how far the tiles reach below the layer's box, in px of the layer",
        ),
        number(
            "up",
            0.0,
            Some((0.0, 100_000.0)),
            "how far the tiles reach above the layer's box, in px of the layer",
        ),
    ],
};

/// The properties of the gradient ramp.
pub static RAMP: Registry = Registry {
    kind: "ramp",
    rows: &[
        point(
            "start",
            [0.0, 0.0],
            &[],
            false,
            "where the ramp starts, in px of the layer",
        ),
        color("start-color", [0.0, 0.0, 0.0, 1.0], "the colour at `start`"),
        point(
            "end",
            [0.0, 100.0],
            &[],
            false,
            "where the ramp ends, in px of the layer",
        ),
        color("end-color", [1.0, 1.0, 1.0, 1.0], "the colour at `end`"),
        number(
            "blend",
            0.0,
            Some((0.0, 1.0)),
            "how much of the layer's own colour stays, 0 to 1",
        ),
    ],
};

/// The properties of the four-colour gradient.
pub static FOUR_COLOR: Registry = Registry {
    kind: "four-color",
    rows: &[
        point(
            "point-1",
            [0.0, 0.0],
            &[],
            false,
            "where `color-1` is, in px of the layer",
        ),
        color("color-1", [1.0, 1.0, 0.0, 1.0], "the first colour"),
        point("point-2", [100.0, 0.0], &[], false, "where `color-2` is"),
        color("color-2", [0.0, 1.0, 0.0, 1.0], "the second colour"),
        point("point-3", [0.0, 100.0], &[], false, "where `color-3` is"),
        color("color-3", [1.0, 0.0, 1.0, 1.0], "the third colour"),
        point("point-4", [100.0, 100.0], &[], false, "where `color-4` is"),
        color("color-4", [0.0, 0.0, 1.0, 1.0], "the fourth colour"),
        number(
            "blend",
            100.0,
            Some((1.0, 10_000.0)),
            "how far the colours mix; the weights fall with distance to the power 2 × 100 / `blend`",
        ),
        opacity("how much of the gradient shows, 0 to 1"),
    ],
};

/// The properties of the grid.
pub static GRID: Registry = Registry {
    kind: "grid",
    rows: &[
        point(
            "anchor",
            [0.0, 0.0],
            &[],
            false,
            "where a corner of the cells is, in px of the layer",
        ),
        point(
            "corner",
            [32.0, 24.0],
            &[],
            false,
            "the opposite corner of the first cell, when `size` is `corner`",
        ),
        number("width", 32.0, Some((1.0, 4_000.0)), "a cell's width, in px"),
        number(
            "height",
            24.0,
            Some((1.0, 4_000.0)),
            "a cell's height, in px, when `size` is `width-height`",
        ),
        number(
            "border",
            5.0,
            Some((0.0, 2_000.0)),
            "how wide the lines are, in px",
        ),
        color("color", [1.0, 1.0, 1.0, 1.0], "the lines' colour"),
        opacity("how much of the grid shows, 0 to 1"),
    ],
};

/// The properties of the venetian blinds.
pub static VENETIAN_BLINDS: Registry = Registry {
    kind: "venetian-blinds",
    rows: &[
        number(
            "completion",
            0.0,
            Some((0.0, 1.0)),
            "how much of each stripe is hidden, 0 to 1",
        ),
        number(
            "direction",
            0.0,
            None,
            "which way the stripes run, in degrees; 0 is horizontal",
        ),
        number(
            "width",
            20.0,
            Some((1.0, 32_000.0)),
            "how wide each stripe is, in px",
        ),
        number(
            "feather",
            0.0,
            Some((0.0, 32_000.0)),
            "how soft a stripe's edge is, in px",
        ),
    ],
};

/// The properties of fractal noise.
pub static FRACTAL_NOISE: Registry = Registry {
    kind: "fractal-noise",
    rows: &[
        number(
            "contrast",
            1.0,
            Some((0.0, 100.0)),
            "how far levels spread from mid grey, 1 as they are",
        ),
        number(
            "brightness",
            0.0,
            Some((-100.0, 100.0)),
            "what is added to every level",
        ),
        number("rotation", 0.0, None, "how far the noise turns, in degrees"),
        number(
            "scale",
            1.0,
            Some((0.01, 100.0)),
            "the noise's size; at 1 a cell is 100 px",
        ),
        number(
            "scale-width",
            1.0,
            Some((0.01, 100.0)),
            "the noise's width, times `scale`",
        ),
        number(
            "scale-height",
            1.0,
            Some((0.01, 100.0)),
            "the noise's height, times `scale`",
        ),
        point(
            "offset",
            [0.0, 0.0],
            &[],
            false,
            "where the noise is centred, in px of the layer",
        ),
        number(
            "complexity",
            6.0,
            Some((1.0, 20.0)),
            "how many octaves it sums",
        ),
        number(
            "sub-influence",
            0.7,
            Some((0.0, 100.0)),
            "each octave's weight against the one before",
        ),
        number(
            "sub-scaling",
            0.56,
            Some((0.1, 100.0)),
            "each octave's size against the one before",
        ),
        number(
            "sub-rotation",
            0.0,
            None,
            "how far each octave turns against the one before, in degrees",
        ),
        point(
            "sub-offset",
            [0.0, 0.0],
            &[],
            false,
            "how far each octave moves against the one before, in px",
        ),
        number(
            "evolution",
            0.0,
            None,
            "how far the noise has evolved, in degrees; a turn is a cell",
        ),
        number("seed", 0.0, Some((0.0, 100_000.0)), "which noise it is"),
        opacity("how much of the noise shows, 0 to 1"),
    ],
};

/// The properties of the Gaussian blur.
pub static GAUSSIAN_BLUR: Registry = Registry {
    kind: "gaussian-blur",
    rows: &[number(
        "blurriness",
        0.0,
        Some((0.0, 30_000.0)),
        "how far it blurs, in px; the Gaussian's standard deviation is 0.3 of it",
    )],
};

/// The properties of the box blur.
pub static BOX_BLUR: Registry = Registry {
    kind: "box-blur",
    rows: &[
        number(
            "radius",
            0.0,
            Some((0.0, 30_000.0)),
            "the radius of each box, in px",
        ),
        number(
            "iterations",
            3.0,
            Some((1.0, 50.0)),
            "how many boxes it runs, one after another; three come close to a Gaussian",
        ),
    ],
};

/// The properties of the drop shadow.
pub static DROP_SHADOW: Registry = Registry {
    kind: "drop-shadow",
    rows: &[
        color("color", [0.0, 0.0, 0.0, 1.0], "the shadow's colour"),
        opacity("the shadow's opacity, 0 to 1"),
        number(
            "direction",
            135.0,
            None,
            "where the shadow falls, in degrees clockwise from up",
        ),
        number(
            "distance",
            5.0,
            Some((0.0, 4_000.0)),
            "how far the shadow falls, in px",
        ),
        number(
            "softness",
            0.0,
            Some((0.0, 30_000.0)),
            "how far the shadow blurs, in px, as `blurriness` does",
        ),
    ],
};

/// The properties of the glow.
pub static GLOW: Registry = Registry {
    kind: "glow",
    rows: &[
        number(
            "threshold",
            0.6,
            Some((0.0, 1.0)),
            "the luma below which nothing glows, 0 to 1",
        ),
        number(
            "radius",
            10.0,
            Some((0.0, 1_000.0)),
            "how far the glow spreads, in px, as `blurriness` does",
        ),
        number(
            "intensity",
            1.0,
            Some((0.0, 255.0)),
            "what the glow's light is multiplied by",
        ),
    ],
};

/// The properties of the sharpen effect.
pub static SHARPEN: Registry = Registry {
    kind: "sharpen",
    rows: &[number(
        "amount",
        0.0,
        Some((0.0, 4_000.0)),
        "how far each pixel moves away from its neighbours' mean, in percent of the difference",
    )],
};

/// The properties of the black and white effect: how light each of six
/// hues turns, and a tint.
pub static BLACK_WHITE: Registry = Registry {
    kind: "black-white",
    rows: &[
        number(
            "reds",
            0.4,
            Some((-2.0, 3.0)),
            "the grey reds turn into, as a share of their level",
        ),
        number(
            "yellows",
            0.6,
            Some((-2.0, 3.0)),
            "the grey yellows turn into, as a share of their level",
        ),
        number(
            "greens",
            0.4,
            Some((-2.0, 3.0)),
            "the grey greens turn into, as a share of their level",
        ),
        number(
            "cyans",
            0.6,
            Some((-2.0, 3.0)),
            "the grey cyans turn into, as a share of their level",
        ),
        number(
            "blues",
            0.2,
            Some((-2.0, 3.0)),
            "the grey blues turn into, as a share of their level",
        ),
        number(
            "magentas",
            0.8,
            Some((-2.0, 3.0)),
            "the grey magentas turn into, as a share of their level",
        ),
        number(
            "tint",
            0.0,
            Some((0.0, 1.0)),
            "how much of `tint-color` colours the grey, 0 to 1",
        ),
        color(
            "tint-color",
            [0.882_353, 0.827_451, 0.701_961, 1.0],
            "the colour the grey takes",
        ),
    ],
};

/// The properties of the hue and saturation effect.
pub static HUE_SATURATION: Registry = Registry {
    kind: "hue-saturation",
    rows: &[
        number("hue", 0.0, None, "how far hues turn, in degrees"),
        number(
            "saturation",
            0.0,
            Some((-1.0, 1.0)),
            "-1 takes all colour out, 1 saturates fully",
        ),
        number(
            "lightness",
            0.0,
            Some((-1.0, 1.0)),
            "-1 turns every colour black, 1 white",
        ),
        number(
            "colorize-hue",
            0.0,
            None,
            "the hue every colour takes when colourising, in degrees",
        ),
        number(
            "colorize-saturation",
            0.25,
            Some((0.0, 1.0)),
            "the saturation every colour takes when colourising",
        ),
        number(
            "colorize-lightness",
            0.0,
            Some((-1.0, 1.0)),
            "how far colourising moves lightness towards white (1) or black (-1)",
        ),
    ],
};

/// The properties of the exposure effect.
pub static EXPOSURE: Registry = Registry {
    kind: "exposure",
    rows: &[
        number(
            "exposure",
            0.0,
            Some((-100.0, 100.0)),
            "stops of light: each doubles or halves it",
        ),
        number(
            "offset",
            0.0,
            Some((-2.0, 2.0)),
            "light added after the exposure; negative darkens the shadows",
        ),
        number(
            "gamma",
            1.0,
            Some((0.1, 10.0)),
            "a power of 1/gamma on the result: above 1 brightens the mid-tones",
        ),
    ],
};

/// The properties of the levels effect.
pub static LEVELS: Registry = Registry {
    kind: "levels",
    rows: &[
        number(
            "input-black",
            0.0,
            None,
            "the level that becomes `output-black`, 0 to 1",
        ),
        number(
            "input-white",
            1.0,
            None,
            "the level that becomes `output-white`, 0 to 1",
        ),
        number(
            "gamma",
            1.0,
            Some((0.0, 5.0)),
            "the power between: above 1 brightens the mid-tones",
        ),
        number("output-black", 0.0, None, "the darkest level out, 0 to 1"),
        number("output-white", 1.0, None, "the brightest level out, 0 to 1"),
    ],
};

/// The properties of the invert effect.
pub static INVERT: Registry = Registry {
    kind: "invert",
    rows: &[number(
        "blend",
        0.0,
        Some((0.0, 1.0)),
        "how much of the original stays, 0 to 1",
    )],
};

/// The properties of the curves effect: one curve per channel, each the
/// points `M x,y L x,y …` from input to output level, 0 to 1, in rising `x`,
/// at most 16, joined by a natural cubic spline. An empty path is the
/// straight line, which changes nothing.
pub static CURVES: Registry = Registry {
    kind: "curves",
    rows: &[
        PropertySpec {
            name: "rgb",
            ty: ValueType::Path,
            rest: Rest::Value(Value::Path(Path::EMPTY)),
            components: &[],
            range: None,
            broadcast: false,
            spatial: false,
            doc: "the curve of red, green and blue alike, after their own",
        },
        PropertySpec {
            name: "red",
            ty: ValueType::Path,
            rest: Rest::Value(Value::Path(Path::EMPTY)),
            components: &[],
            range: None,
            broadcast: false,
            spatial: false,
            doc: "the curve of red",
        },
        PropertySpec {
            name: "green",
            ty: ValueType::Path,
            rest: Rest::Value(Value::Path(Path::EMPTY)),
            components: &[],
            range: None,
            broadcast: false,
            spatial: false,
            doc: "the curve of green",
        },
        PropertySpec {
            name: "blue",
            ty: ValueType::Path,
            rest: Rest::Value(Value::Path(Path::EMPTY)),
            components: &[],
            range: None,
            broadcast: false,
            spatial: false,
            doc: "the curve of blue",
        },
        PropertySpec {
            name: "alpha",
            ty: ValueType::Path,
            rest: Rest::Value(Value::Path(Path::EMPTY)),
            components: &[],
            range: None,
            broadcast: false,
            spatial: false,
            doc: "the curve of alpha",
        },
    ],
};

/// The channels an effect can work on.
const CHANNELS: &[&str] = &["rgb", "red", "green", "blue", "alpha"];

/// The properties of a trim.
pub static TRIM: Registry = Registry {
    kind: "trim",
    rows: &[
        number(
            "end",
            1.0,
            Some((0.0, 1.0)),
            "where the drawn stretch ends, as a share of the outline's length",
        ),
        number(
            "start",
            0.0,
            Some((0.0, 1.0)),
            "where the drawn stretch starts, as a share of the outline's length",
        ),
        number(
            "offset",
            0.0,
            None,
            "how far the stretch is shifted along the outline, in degrees; 360 is once around",
        ),
    ],
};

/// Every kind of shape item.
pub static SHAPES: [ItemKind; 8] = [
    ItemKind {
        name: "group",
        role: Role::Group,
        registry: &GROUP,
        primary: Primary::Items(&SHAPES),
        settings: &[],
        input: None,
        outline: None,
        doc: "items under one transform and opacity; paints in it draw the outlines above them, \
              in it and in its groups",
    },
    ItemKind {
        name: "rect",
        role: Role::Geometry,
        registry: &RECT,
        primary: Primary::Property("size"),
        settings: &[],
        input: None,
        outline: None,
        doc: "a rectangle centred on `position`, with rounded corners",
    },
    ItemKind {
        name: "ellipse",
        role: Role::Geometry,
        registry: &ELLIPSE,
        primary: Primary::Property("size"),
        settings: &[],
        input: None,
        outline: None,
        doc: "an ellipse centred on `position`",
    },
    ItemKind {
        name: "star",
        role: Role::Geometry,
        registry: &STAR,
        primary: Primary::Property("points"),
        settings: &[
            SettingSpec {
                name: "type",
                choices: &["star", "polygon"],
                default: "star",
                doc: "`star` alternates points and inner corners; `polygon` has only corners, \
                      and leaves `inner-radius` and `inner-roundness` unused",
            },
            SettingSpec {
                name: "direction",
                choices: &["clockwise", "counterclockwise"],
                default: "clockwise",
                doc: "which way the outline runs from its first point, which a trim follows",
            },
        ],
        input: None,
        outline: None,
        doc: "a star or a regular polygon centred on `position`, its first point straight up",
    },
    ItemKind {
        name: "path",
        role: Role::Geometry,
        registry: &PATH,
        primary: Primary::Property("path"),
        settings: &[],
        input: None,
        outline: None,
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
        input: None,
        outline: None,
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
        input: None,
        outline: None,
        doc: "paints a line along the outlines above it",
    },
    ItemKind {
        name: "trim",
        role: Role::Modifier,
        registry: &TRIM,
        primary: Primary::Property("end"),
        settings: &[SettingSpec {
            name: "mode",
            choices: &["each", "joined"],
            default: "each",
            doc: "`each` cuts every outline the same way on its own; `joined` cuts them as one \
                  line, in order",
        }],
        input: None,
        outline: None,
        doc: "cuts the outlines above it, and those in the groups above it, down to a stretch \
              of their length, for every paint that draws them",
    },
];

/// Every kind of effect.
pub static EFFECTS: [ItemKind; 29] = [
    ItemKind {
        name: "fill",
        role: Role::Effect,
        registry: &FILL_EFFECT,
        primary: Primary::Property("color"),
        settings: &[],
        input: None,
        outline: None,
        doc: "turns every drawn pixel of the layer one colour, keeping its coverage",
    },
    ItemKind {
        name: "transform",
        role: Role::Effect,
        registry: &TRANSFORM_EFFECT,
        primary: Primary::Property("scale"),
        settings: &[],
        input: None,
        outline: None,
        doc: "moves, scales, turns and fades everything the layer draws, inside the layer: \
              `anchor` sits at `position`, both at the origin by default, so a scale or turn \
              on its own is about the origin",
    },
    ItemKind {
        name: "tint",
        role: Role::Effect,
        registry: &TINT,
        primary: Primary::Property("amount"),
        settings: &[],
        input: None,
        outline: None,
        doc: "maps each colour's luminance from `black` to `white`",
    },
    ItemKind {
        name: "hue-saturation",
        role: Role::Effect,
        registry: &HUE_SATURATION,
        primary: Primary::Property("hue"),
        settings: &[SettingSpec {
            name: "mode",
            choices: &["adjust", "colorize"],
            default: "adjust",
            doc: "`adjust` turns hues and changes saturation and lightness; `colorize` gives \
                  every colour the `colorize-` hue and saturation, keeping its lightness",
        }],
        input: None,
        outline: None,
        doc: "turns hues and changes saturation and lightness",
    },
    ItemKind {
        name: "exposure",
        role: Role::Effect,
        registry: &EXPOSURE,
        primary: Primary::Property("exposure"),
        settings: &[SettingSpec {
            name: "space",
            choices: &["linear", "gamma"],
            default: "linear",
            doc: "`linear` works on light, decoding sRGB first and encoding it again after; \
                  `gamma` works on the values as they are",
        }],
        input: None,
        outline: None,
        doc: "changes exposure in stops, as a camera does",
    },
    ItemKind {
        name: "levels",
        role: Role::Effect,
        registry: &LEVELS,
        primary: Primary::Property("gamma"),
        settings: &[SettingSpec {
            name: "channel",
            choices: CHANNELS,
            default: "rgb",
            doc: "the channel it works on; `rgb` is each of the three alike",
        }],
        input: None,
        outline: None,
        doc: "maps the levels from `input-black`–`input-white` to `output-black`–`output-white` \
              through `gamma`, clipped to 0 to 1",
    },
    ItemKind {
        name: "particles",
        role: Role::Effect,
        registry: &PARTICLES,
        primary: Primary::Property("rate"),
        settings: &[
            SettingSpec {
                name: "animation",
                choices: &["explosive", "direction"],
                default: "explosive",
                doc: "how particles leave: every way, or towards `direction`",
            },
            SettingSpec {
                name: "shape",
                choices: &["faded", "disc", "bubble"],
                default: "faded",
                doc: "what a particle looks like: a soft dot, a solid one, or a ring",
            },
            SettingSpec {
                name: "opacity-map",
                choices: &[
                    "fade-out",
                    "constant",
                    "fade-out-sharp",
                    "fade-in",
                    "fade-in-out",
                ],
                default: "fade-out",
                doc: "how a particle's opacity runs over its life",
            },
            SettingSpec {
                name: "transfer",
                choices: &["over", "screen", "add"],
                default: "over",
                doc: "how particles lay on each other",
            },
        ],
        input: None,
        outline: None,
        doc: "draws particles a producer sends out over time, in place of the layer",
    },
    ItemKind {
        name: "displacement-map",
        role: Role::Effect,
        registry: &DISPLACEMENT_MAP,
        primary: Primary::Property("horizontal"),
        settings: &[
            SettingSpec {
                name: "horizontal-from",
                choices: &[
                    "red",
                    "green",
                    "blue",
                    "alpha",
                    "luminance",
                    "hue",
                    "lightness",
                    "saturation",
                    "full",
                    "half",
                    "off",
                ],
                default: "red",
                doc: "which level of the map moves pixels across; `full`, `half` and `off` are fixed levels",
            },
            SettingSpec {
                name: "vertical-from",
                choices: &[
                    "green",
                    "red",
                    "blue",
                    "alpha",
                    "luminance",
                    "hue",
                    "lightness",
                    "saturation",
                    "full",
                    "half",
                    "off",
                ],
                default: "green",
                doc: "which level of the map moves pixels up or down",
            },
            SettingSpec {
                name: "behaviour",
                choices: &["center", "stretch", "tile"],
                default: "center",
                doc: "how the map's box lies on the layer's: centred, stretched over it, or tiled",
            },
            SettingSpec {
                name: "edges",
                choices: &["clear", "wrap"],
                default: "clear",
                doc: "whether what moves past the layer's box comes back on the other side",
            },
        ],
        input: Some(Input {
            key: "map",
            reads: Reads::Drawing,
        }),
        outline: None,
        doc: "moves each pixel by the levels of another layer, the map",
    },
    ItemKind {
        name: "audio-spectrum",
        role: Role::Effect,
        registry: &AUDIO_SPECTRUM,
        primary: Primary::Property("height"),
        settings: &[
            SettingSpec {
                name: "display",
                choices: &["digital", "lines", "dots"],
                default: "digital",
                doc: "how a band shows: a column of squares, a solid bar, or a dot at its top",
            },
            SettingSpec {
                name: "side",
                choices: &["both", "a", "b"],
                default: "both",
                doc: "which side of the line bands rise on: `a` is to the left going from \
                      `start` to `end`, `b` the other, `both` mirrors them",
            },
            SettingSpec {
                name: "path",
                choices: &["line", "circle"],
                default: "line",
                doc: "where bands stand when `along` names no mask: along the line from \
                      `start` to `end`, or round a circle about `start` through `end`",
            },
            SettingSpec {
                name: "average",
                choices: &["off", "on"],
                default: "off",
                doc: "whether each band is averaged over `duration` in short windows, which \
                      steadies it",
            },
            SettingSpec {
                name: "symmetry",
                choices: &["on", "off"],
                default: "on",
                doc: "whether the hue turn of `hue` runs to the middle band and back",
            },
            SettingSpec {
                name: "composite",
                choices: &["off", "on"],
                default: "off",
                doc: "whether the layer's own drawing stays under the bands",
            },
        ],
        input: Some(Input {
            key: "audio",
            reads: Reads::Sound,
        }),
        outline: Some("along"),
        doc: "draws the spectrum of another layer's sound at the layer's time: one band a \
              frequency from `from` to `to`, as tall as that frequency is loud; `along` names \
              one of the layer's masks by its id for the bands to stand along its outline",
    },
    ItemKind {
        name: "audio-waveform",
        role: Role::Effect,
        registry: &AUDIO_WAVEFORM,
        primary: Primary::Property("height"),
        settings: &[
            SettingSpec {
                name: "display",
                choices: &["digital", "lines", "dots"],
                default: "digital",
                doc: "how the wave shows: a bar a sample, a line through the samples, or a \
                      dot a sample",
            },
            SettingSpec {
                name: "channel",
                choices: &["mono", "left", "right"],
                default: "mono",
                doc: "which channel the wave follows; `mono` is both, averaged",
            },
            SettingSpec {
                name: "path",
                choices: &["line", "circle"],
                default: "line",
                doc: "where the wave runs when `along` names no mask: along the line from \
                      `start` to `end`, or round a circle about `start` through `end`",
            },
            SettingSpec {
                name: "composite",
                choices: &["off", "on"],
                default: "off",
                doc: "whether the layer's own drawing stays under the wave",
            },
        ],
        input: Some(Input {
            key: "audio",
            reads: Reads::Sound,
        }),
        outline: Some("along"),
        doc: "draws the wave of another layer's sound at the layer's time, `samples` points \
              along the line from `start` to `end`, or along the outline of the layer's mask \
              whose id `along` names",
    },
    ItemKind {
        name: "motion-tile",
        role: Role::Effect,
        registry: &MOTION_TILE,
        primary: Primary::Property("tile-width"),
        settings: &[
            SettingSpec {
                name: "edges",
                choices: &["repeat", "mirror"],
                default: "repeat",
                doc: "whether every other tile flips",
            },
            SettingSpec {
                name: "shift",
                choices: &["vertical", "horizontal"],
                default: "vertical",
                doc: "whether the phase shifts columns up and down, or rows across",
            },
        ],
        input: None,
        outline: None,
        doc: "repeats the layer in tiles around a point",
    },
    ItemKind {
        name: "wrap",
        role: Role::Effect,
        registry: &WRAP,
        primary: Primary::Property("to"),
        settings: &[],
        input: None,
        outline: None,
        doc: "moves the layer's content within its box, wrapping around",
    },
    ItemKind {
        name: "turbulent-displace",
        role: Role::Effect,
        registry: &TURBULENT_DISPLACE,
        primary: Primary::Property("amount"),
        settings: &[
            SettingSpec {
                name: "displacement",
                choices: &[
                    "turbulent",
                    "turbulent-smoother",
                    "bulge",
                    "bulge-smoother",
                    "twist",
                    "twist-smoother",
                    "horizontal",
                    "vertical",
                    "cross",
                ],
                default: "turbulent",
                doc: "how rough the movement is",
            },
            SettingSpec {
                name: "pinning",
                choices: &["all", "left-right", "top-bottom", "none"],
                default: "all",
                doc: "which edges of the layer's box stay put",
            },
        ],
        input: None,
        outline: None,
        doc: "moves each pixel by a smooth noise",
    },
    ItemKind {
        name: "mirror",
        role: Role::Effect,
        registry: &MIRROR,
        primary: Primary::Property("center"),
        settings: &[],
        input: None,
        outline: None,
        doc: "reflects one side of a line through the layer onto the other",
    },
    ItemKind {
        name: "polar-coordinates",
        role: Role::Effect,
        registry: &POLAR_COORDINATES,
        primary: Primary::Property("interpolation"),
        settings: &[SettingSpec {
            name: "conversion",
            choices: &["rect-to-polar", "polar-to-rect"],
            default: "rect-to-polar",
            doc: "`rect-to-polar` wraps the layer's rows into rings about its middle, its \
                  columns round them; `polar-to-rect` unrolls rings into rows",
        }],
        input: None,
        outline: None,
        doc: "turns the layer between rows and columns and rings about its middle",
    },
    ItemKind {
        name: "repeat-tile",
        role: Role::Effect,
        registry: &REPEAT_TILE,
        primary: Primary::Setting("tiling"),
        settings: &[SettingSpec {
            name: "tiling",
            choices: &["repeat", "unfold", "brick", "none"],
            default: "repeat",
            doc: "how the tiles lie: all alike, every other column and row flipped so \
                  neighbours meet at their edges, every other row shifted by half a tile, \
                  or none, the box grown with nothing in it for the effects after",
        }],
        input: None,
        outline: None,
        doc: "repeats the layer's box around it, growing the box",
    },
    ItemKind {
        name: "ramp",
        role: Role::Effect,
        registry: &RAMP,
        primary: Primary::Property("blend"),
        settings: &[SettingSpec {
            name: "shape",
            choices: &["linear", "radial"],
            default: "linear",
            doc: "`linear` along the line from `start` to `end`; `radial` around `start`",
        }],
        input: None,
        outline: None,
        doc: "fills the layer with a gradient between two colours",
    },
    ItemKind {
        name: "four-color",
        role: Role::Effect,
        registry: &FOUR_COLOR,
        primary: Primary::Property("opacity"),
        settings: &[SettingSpec {
            name: "mode",
            choices: &["replace", "over"],
            default: "replace",
            doc: "`replace` draws it alone where the layer is; `over` lays it over the layer",
        }],
        input: None,
        outline: None,
        doc: "fills the layer with four colours at four points, blended",
    },
    ItemKind {
        name: "grid",
        role: Role::Effect,
        registry: &GRID,
        primary: Primary::Property("border"),
        settings: &[
            SettingSpec {
                name: "size",
                choices: &["corner", "width", "width-height"],
                default: "corner",
                doc: "what sets a cell's size: `corner`, `width` alone, or `width` and `height`",
            },
            SettingSpec {
                name: "draw",
                choices: &["lines", "cells"],
                default: "lines",
                doc: "whether the lines or the cells between them are drawn",
            },
            SettingSpec {
                name: "mode",
                choices: &["replace", "over"],
                default: "replace",
                doc: "`replace` draws it alone where the layer is; `over` lays it over the layer",
            },
        ],
        input: None,
        outline: None,
        doc: "draws a grid of lines",
    },
    ItemKind {
        name: "venetian-blinds",
        role: Role::Effect,
        registry: &VENETIAN_BLINDS,
        primary: Primary::Property("completion"),
        settings: &[],
        input: None,
        outline: None,
        doc: "hides the layer in stripes, from one side of each",
    },
    ItemKind {
        name: "fractal-noise",
        role: Role::Effect,
        registry: &FRACTAL_NOISE,
        primary: Primary::Property("evolution"),
        settings: &[
            SettingSpec {
                name: "fractal",
                choices: &[
                    "basic",
                    "turbulent-smooth",
                    "turbulent-basic",
                    "turbulent-sharp",
                ],
                default: "basic",
                doc: "how octaves sum: as they are, or folded about mid grey",
            },
            SettingSpec {
                name: "noise",
                choices: &["spline", "soft-linear", "linear", "block"],
                default: "spline",
                doc: "how values between lattice points blend",
            },
            SettingSpec {
                name: "overflow",
                choices: &["clip", "soft-clamp", "wrap-back"],
                default: "clip",
                doc: "how levels outside 0 to 1 come back",
            },
            SettingSpec {
                name: "polarity",
                choices: &["normal", "inverted"],
                default: "normal",
                doc: "whether the levels are inverted",
            },
            SettingSpec {
                name: "mode",
                choices: &["over", "replace"],
                default: "over",
                doc: "`over` lays it over the layer; `replace` draws it alone where the layer is",
            },
        ],
        input: None,
        outline: None,
        doc: "fills the layer with grey noise in octaves",
    },
    ItemKind {
        name: "gaussian-blur",
        role: Role::Effect,
        registry: &GAUSSIAN_BLUR,
        primary: Primary::Property("blurriness"),
        settings: &[
            SettingSpec {
                name: "dimensions",
                choices: &["both", "horizontal", "vertical"],
                default: "both",
                doc: "which way it blurs: both ways, along rows or along columns",
            },
            SettingSpec {
                name: "edges",
                choices: &["transparent", "repeat"],
                default: "transparent",
                doc: "what lies beyond the frame's edges while blurring: nothing, or the edge pixels repeated",
            },
        ],
        input: None,
        outline: None,
        doc: "blurs the layer with a Gaussian",
    },
    ItemKind {
        name: "box-blur",
        role: Role::Effect,
        registry: &BOX_BLUR,
        primary: Primary::Property("radius"),
        settings: &[
            SettingSpec {
                name: "dimensions",
                choices: &["both", "horizontal", "vertical"],
                default: "both",
                doc: "which way it blurs: both ways, along rows or along columns",
            },
            SettingSpec {
                name: "edges",
                choices: &["transparent", "repeat"],
                default: "transparent",
                doc: "what lies beyond the frame's edges while blurring: nothing, or the edge pixels repeated",
            },
        ],
        input: None,
        outline: None,
        doc: "blurs the layer with boxes run one after another",
    },
    ItemKind {
        name: "drop-shadow",
        role: Role::Effect,
        registry: &DROP_SHADOW,
        primary: Primary::Property("distance"),
        settings: &[SettingSpec {
            name: "layer",
            choices: &["shown", "hidden"],
            default: "shown",
            doc: "whether the layer is drawn over its shadow, or only the shadow",
        }],
        input: None,
        outline: None,
        doc: "draws the layer's shape as a soft shadow behind it",
    },
    ItemKind {
        name: "glow",
        role: Role::Effect,
        registry: &GLOW,
        primary: Primary::Property("radius"),
        settings: &[
            SettingSpec {
                name: "dimensions",
                choices: &["both", "horizontal", "vertical"],
                default: "both",
                doc: "which way it blurs: both ways, along rows or along columns",
            },
            SettingSpec {
                name: "composite",
                choices: &["with-original", "glow-only"],
                default: "with-original",
                doc: "whether the glow is added to the layer, or drawn alone",
            },
        ],
        input: None,
        outline: None,
        doc: "adds the blurred light of the layer's bright colours to it",
    },
    ItemKind {
        name: "sharpen",
        role: Role::Effect,
        registry: &SHARPEN,
        primary: Primary::Property("amount"),
        settings: &[],
        input: None,
        outline: None,
        doc: "sharpens edges by moving pixels away from their neighbours",
    },
    ItemKind {
        name: "black-white",
        role: Role::Effect,
        registry: &BLACK_WHITE,
        primary: Primary::Property("tint"),
        settings: &[],
        input: None,
        outline: None,
        doc: "turns colours to grey, each hue as light as its share says",
    },
    ItemKind {
        name: "curves",
        role: Role::Effect,
        registry: &CURVES,
        primary: Primary::Property("rgb"),
        settings: &[],
        input: None,
        outline: None,
        doc: "maps each channel's levels through a curve: first the channel's own, then `rgb`",
    },
    ItemKind {
        name: "invert",
        role: Role::Effect,
        registry: &INVERT,
        primary: Primary::Setting("channel"),
        settings: &[SettingSpec {
            name: "channel",
            choices: CHANNELS,
            default: "rgb",
            doc: "the channel it inverts; `rgb` is each of the three",
        }],
        input: None,
        outline: None,
        doc: "turns each level of a channel into its opposite",
    },
];

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
        number(
            "stagger",
            0.0,
            Some((0.0, f64::INFINITY)),
            "how many frames each part waits after the one before it in the order, seeing the \
             animator as it was that much earlier",
        ),
        number("seed", 0.0, None, "which shuffle the `random` order takes"),
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
        number(
            "ease-high",
            0.0,
            Some((-1.0, 1.0)),
            "how the choice eases as it nears fully chosen: above 0 it arrives slower, below 0 \
             faster",
        ),
        number(
            "ease-low",
            0.0,
            Some((-1.0, 1.0)),
            "how the choice eases as it leaves not chosen: above 0 it starts slower, below 0 \
             faster",
        ),
    ],
};

/// Every kind of text animator.
pub static ANIMATORS: [ItemKind; 1] = [ItemKind {
    name: "animator",
    role: Role::Animator,
    registry: &ANIMATOR,
    primary: Primary::Items(&SELECTORS),
    settings: &[
        SettingSpec {
            name: "by",
            choices: &["char", "char-no-space", "word", "line"],
            default: "char",
            doc: "which parts take turns when it staggers",
        },
        SettingSpec {
            name: "order",
            choices: &["first", "last", "center", "edges", "random"],
            default: "first",
            doc: "which part goes first when it staggers: in reading order, the reverse, \
                  outward from the middle, inward from both ends, or shuffled by `seed`",
        },
    ],
    input: None,
    outline: None,
    doc: "applies its properties to the parts of the text its selectors choose, each part in \
          its turn when it staggers",
}];

/// The properties of a `parts` selector: none of its own; each of its parts
/// holds its amount.
pub static PARTS: Registry = Registry {
    kind: "parts",
    rows: &[],
};

/// The properties of one part of a `parts` selector.
pub static PART: Registry = Registry {
    kind: "part",
    rows: &[point(
        "amount",
        [0.0, 0.0],
        &["amount-x", "amount-y"],
        true,
        "how strongly this part is chosen, from -1 to 1, for what the animator moves and \
         scales across and down; one number chooses both, and values of one number take \
         the first; negative reverses the animator",
    )],
};

/// The one kind of item a `parts` selector holds.
pub static PART_KINDS: [ItemKind; 1] = [ItemKind {
    name: "part",
    role: Role::Selector,
    registry: &PART,
    primary: Primary::Property("amount"),
    settings: &[],
    input: None,
    outline: None,
    doc: "how strongly one part is chosen, in the order the selector counts them",
}];

/// Every kind of text selector.
pub static SELECTORS: [ItemKind; 2] = [
    ItemKind {
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
        input: None,
        outline: None,
        doc: "chooses the parts between `start` and `end`, moved by `offset`",
    },
    ItemKind {
        name: "parts",
        role: Role::Selector,
        registry: &PARTS,
        primary: Primary::Items(&PART_KINDS),
        settings: &[
            SettingSpec {
                name: "by",
                choices: &["char", "char-no-space", "word", "line"],
                default: "char",
                doc: "what it counts: characters, characters other than spaces, words or lines",
            },
            SettingSpec {
                name: "mode",
                choices: &["add", "subtract", "intersect", "min", "max", "difference"],
                default: "add",
                doc: "how it combines with the selectors before it",
            },
        ],
        input: None,
        outline: None,
        doc: "chooses each part by its own `part` item, the first part by the first item; a part \
          past the last item is not chosen",
    },
];

/// The properties of a mask.
pub static MASK: Registry = Registry {
    kind: "mask",
    rows: &[
        PropertySpec {
            name: "path",
            ty: ValueType::Path,
            rest: Rest::Value(Value::Path(Path::EMPTY)),
            components: &[],
            range: None,
            broadcast: false,
            spatial: false,
            doc: "the outline it cuts along, as SVG path data in px of the layer; \
                  an open outline closes; keys of one track share their vertex count",
        },
        PropertySpec {
            name: "feather",
            ty: ValueType::Point,
            rest: Rest::Value(Value::Point([0.0, 0.0])),
            components: &["feather-x", "feather-y"],
            range: Some((0.0, f64::INFINITY)),
            broadcast: true,
            spatial: false,
            doc: "how far its edge fades, in px, on each axis; one number fades both",
        },
        opacity("how much it covers, 0 to 1"),
        number(
            "expand",
            0.0,
            None,
            "how far its edge moves out, in px; a negative value moves it in",
        ),
    ],
};

/// The one kind of mask.
pub static MASKS: [ItemKind; 1] = [ItemKind {
    name: "mask",
    role: Role::Mask,
    registry: &MASK,
    primary: Primary::Property("path"),
    settings: &[
        SettingSpec {
            name: "mode",
            choices: &[
                "add",
                "subtract",
                "intersect",
                "lighten",
                "darken",
                "difference",
                "none",
            ],
            default: "add",
            doc: "how it combines with the masks before it",
        },
        SettingSpec {
            name: "keep",
            choices: &["inside", "outside"],
            default: "inside",
            doc: "whether it covers what is inside its outline, or what is outside",
        },
    ],
    input: None,
    outline: None,
    doc: "cuts what the layer draws to an outline, before its effects",
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
    use super::{
        ANIMATOR, ELLIPSE, FILL, FILL_EFFECT, MASK, PART, PATH, Prop, RANGE, RECT, STAR, STROKE,
        TRIM,
    };

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
    /// How many points a star has, or corners a polygon.
    pub const STAR_POINTS: Prop = Prop::of(&STAR, 0);
    /// A star's centre.
    pub const STAR_POSITION: Prop = Prop::of(&STAR, 1);
    /// How far a star is turned, in degrees.
    pub const STAR_ROTATE: Prop = Prop::of(&STAR, 2);
    /// How far a star's points are from its centre.
    pub const STAR_OUTER_RADIUS: Prop = Prop::of(&STAR, 3);
    /// How far a star's inner corners are from its centre.
    pub const STAR_INNER_RADIUS: Prop = Prop::of(&STAR, 4);
    /// How round a star's points are.
    pub const STAR_OUTER_ROUNDNESS: Prop = Prop::of(&STAR, 5);
    /// How round a star's inner corners are.
    pub const STAR_INNER_ROUNDNESS: Prop = Prop::of(&STAR, 6);
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
    /// Where a trim's stretch ends.
    pub const TRIM_END: Prop = Prop::of(&TRIM, 0);
    /// Where a trim's stretch starts.
    pub const TRIM_START: Prop = Prop::of(&TRIM, 1);
    /// How far a trim's stretch is shifted, in degrees.
    pub const TRIM_OFFSET: Prop = Prop::of(&TRIM, 2);
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
    /// How many frames each part of a staggered animator waits.
    pub const ANIMATOR_STAGGER: Prop = Prop::of(&ANIMATOR, 9);
    /// Which shuffle a staggered animator's `random` order takes.
    pub const ANIMATOR_SEED: Prop = Prop::of(&ANIMATOR, 10);
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
    /// How a range selector's choice eases near fully chosen.
    pub const RANGE_EASE_HIGH: Prop = Prop::of(&RANGE, 5);
    /// How a range selector's choice eases near not chosen.
    pub const RANGE_EASE_LOW: Prop = Prop::of(&RANGE, 6);
    /// How strongly one part of a `parts` selector is chosen.
    pub const PART_AMOUNT: Prop = Prop::of(&PART, 0);
    /// A mask's outline.
    pub const MASK_PATH: Prop = Prop::of(&MASK, 0);
    /// How far a mask's edge fades.
    pub const MASK_FEATHER: Prop = Prop::of(&MASK, 1);
    /// How much a mask covers.
    pub const MASK_OPACITY: Prop = Prop::of(&MASK, 2);
    /// How far a mask's edge moves out.
    pub const MASK_EXPAND: Prop = Prop::of(&MASK, 3);
}

#[cfg(test)]
#[path = "../tests/unit/item.rs"]
mod tests;
