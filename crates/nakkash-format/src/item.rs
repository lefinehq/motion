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
    /// Draws everything before it in its group again, outlines and paints
    /// alike, as copies under transforms and opacities of their own.
    Repeater,
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
    /// The key that names the layers its corners rest on, if it pins
    /// corners to layers: a corner pin's `pivots` ([`Pivots`]).
    pub pivots: Option<&'static str>,
    /// Whether what it draws changes with its layer's time on its own,
    /// with no property animated: particles fly on as time passes. A layer
    /// with such an effect is drawn afresh at each moment of its motion
    /// blur, rather than drawn once and moved ([D57]).
    ///
    /// [D57]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter
    pub timed: bool,
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
    /// Its drawing, as [`Reads::Drawing`], laid on the reading layer's
    /// effect box as the effect's `behaviour` setting says: `stretch`ed
    /// over it or `center`ed on it. A layer's effect box is what its
    /// content fills (a rect, an image's or video's frame, a composition's
    /// canvas), or for content without one (shapes, text) a box the size
    /// of the composition it sits in, centred on its origin. With no layer
    /// named, the reading layer's own drawing before any of its effects
    /// run: what it drew, cut by its masks.
    Laid,
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
    /// The layers its kind's pivots key names, in the same list.
    pub pivots: Option<Pivots>,
}

/// The suffixes of the four corner pivots `nakkash ml track … --corners`
/// writes after a pivot's id, in corner order: top left, then clockwise
/// ([D88]).
///
/// [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots
pub const CORNER_SUFFIXES: [&str; 4] = ["tl", "tr", "br", "bl"];

/// The four layers a corner pin's corners rest on, as a scene names them
/// ([D89]): each corner rests where its layer's anchor lands.
///
/// [D89]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pivots {
    /// The corner pivots of the pivot with this id: `<id>-tl`, `<id>-tr`,
    /// `<id>-br` and `<id>-bl` ([`CORNER_SUFFIXES`]).
    Corners(String),
    /// Four layers by id: top left, top right, bottom right, bottom left.
    Each([String; 4]),
}

impl Pivots {
    /// The ids of the four layers, top left first and clockwise.
    pub fn ids(&self) -> [String; 4] {
        match self {
            Pivots::Corners(id) => CORNER_SUFFIXES.map(|suffix| format!("{id}-{suffix}")),
            Pivots::Each(ids) => ids.clone(),
        }
    }
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
        integral: false,
        doc,
    }
}

/// `spec` with its running total read by effects as well as its value
/// ([`PropertySpec::integral`]).
const fn integrated(mut spec: PropertySpec) -> PropertySpec {
    spec.integral = true;
    spec
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
        integral: false,
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
        integral: false,
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
        integral: false,
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
        integral: false,
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

/// The properties of the solid composite.
pub static SOLID_COMPOSITE: Registry = Registry {
    kind: "solid-composite",
    rows: &[
        number(
            "source-opacity",
            1.0,
            Some((0.0, 1.0)),
            "how opaque the layer is over the solid, 0 to 1",
        ),
        color("color", WHITE, "the solid's colour"),
        opacity("how opaque the solid is, 0 to 1"),
    ],
};

/// How the solid composite lays the layer over its solid, by the names of
/// the rasteriser's operations; the first is the default.
const SOLID_COMPOSITE_MODES: &[&str] = &[
    "normal",
    "add",
    "multiply",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "color-dodge",
    "color-burn",
    "darken",
    "lighten",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
];

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
        integrated(number(
            "rate",
            40.0,
            Some((0.0, 100_000.0)),
            "how many particles are born a second; it acts on births only, so \
             a rate keyed to 0 stops them and those alive live out their `longevity`",
        )),
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

/// The properties of `set-matte`: none, its choices are settings.
pub static SET_MATTE: Registry = Registry {
    kind: "set-matte",
    rows: &[],
};

/// The properties of `shift-channels`: none, its choices are settings.
pub static SHIFT_CHANNELS: Registry = Registry {
    kind: "shift-channels",
    rows: &[],
};

/// The levels a channel may be taken from, the channel's own first.
macro_rules! shift_levels {
    ($own:literal, $($other:literal),*) => {
        &[$own, $($other,)* "luminance", "hue", "lightness", "saturation", "full", "off"]
    };
}

/// The levels a matte may be taken from: alpha, the default, then the
/// colours, luminance, hue, lightness and saturation, then fixed full and
/// off.
const MATTE_LEVELS: &[&str] = &[
    "alpha",
    "red",
    "green",
    "blue",
    "luminance",
    "hue",
    "lightness",
    "saturation",
    "full",
    "off",
];

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

/// The properties of `lens-distortion`.
pub static LENS_DISTORTION: Registry = Registry {
    kind: "lens-distortion",
    rows: &[
        number(
            "field-of-view",
            0.0,
            Some((0.0, 180.0)),
            "how wide the lens sees across the span `field` names, in degrees; 0 bends \
             nothing",
        ),
        point(
            "center",
            [0.0, 0.0],
            &[],
            false,
            "where the lens's axis meets the layer, in px of the layer",
        ),
    ],
};

/// The properties of `bezier-warp`: its twelve points, each in px it
/// moves from where it rests on the layer's box: the corners, and the
/// handles a third of the way along each side.
pub static BEZIER_WARP: Registry = Registry {
    kind: "bezier-warp",
    rows: &[
        point(
            "top-left",
            [0.0, 0.0],
            &[],
            false,
            "how far the top-left corner moves from where it rests, in px of the layer",
        ),
        point(
            "top-handle-left",
            [0.0, 0.0],
            &[],
            false,
            "how far the top side's handle nearer its left end moves from where it rests, in px of the layer",
        ),
        point(
            "top-handle-right",
            [0.0, 0.0],
            &[],
            false,
            "how far the top side's handle nearer its right end moves from where it rests, in px of the layer",
        ),
        point(
            "top-right",
            [0.0, 0.0],
            &[],
            false,
            "how far the top-right corner moves from where it rests, in px of the layer",
        ),
        point(
            "right-handle-top",
            [0.0, 0.0],
            &[],
            false,
            "how far the right side's handle nearer its top moves from where it rests, in px of the layer",
        ),
        point(
            "right-handle-bottom",
            [0.0, 0.0],
            &[],
            false,
            "how far the right side's handle nearer its bottom moves from where it rests, in px of the layer",
        ),
        point(
            "bottom-right",
            [0.0, 0.0],
            &[],
            false,
            "how far the bottom-right corner moves from where it rests, in px of the layer",
        ),
        point(
            "bottom-handle-right",
            [0.0, 0.0],
            &[],
            false,
            "how far the bottom side's handle nearer its right end moves from where it rests, in px of the layer",
        ),
        point(
            "bottom-handle-left",
            [0.0, 0.0],
            &[],
            false,
            "how far the bottom side's handle nearer its left end moves from where it rests, in px of the layer",
        ),
        point(
            "bottom-left",
            [0.0, 0.0],
            &[],
            false,
            "how far the bottom-left corner moves from where it rests, in px of the layer",
        ),
        point(
            "left-handle-bottom",
            [0.0, 0.0],
            &[],
            false,
            "how far the left side's handle nearer its bottom moves from where it rests, in px of the layer",
        ),
        point(
            "left-handle-top",
            [0.0, 0.0],
            &[],
            false,
            "how far the left side's handle nearer its top moves from where it rests, in px of the layer",
        ),
    ],
};

/// The properties of `corner-pin` ([D89]): its four corners, each in px
/// it moves from where it rests, on the layer's box or on its pivot.
///
/// [D89]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography
pub static CORNER_PIN: Registry = Registry {
    kind: "corner-pin",
    rows: &[
        point(
            "top-left",
            [0.0, 0.0],
            &[],
            false,
            "how far the top-left corner moves from where it rests, in px of the layer",
        ),
        point(
            "top-right",
            [0.0, 0.0],
            &[],
            false,
            "how far the top-right corner moves from where it rests, in px of the layer",
        ),
        point(
            "bottom-right",
            [0.0, 0.0],
            &[],
            false,
            "how far the bottom-right corner moves from where it rests, in px of the layer",
        ),
        point(
            "bottom-left",
            [0.0, 0.0],
            &[],
            false,
            "how far the bottom-left corner moves from where it rests, in px of the layer",
        ),
    ],
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

/// The properties of `beam`.
pub static BEAM: Registry = Registry {
    kind: "beam",
    rows: &[
        point(
            "start",
            [0.0, 0.0],
            &[],
            false,
            "where the line starts, in px of the layer",
        ),
        point(
            "end",
            [100.0, 0.0],
            &[],
            false,
            "where the line ends, in px of the layer",
        ),
        number(
            "length",
            0.25,
            Some((0.0, 1.0)),
            "how long the beam is, as a share of the line",
        ),
        number(
            "time",
            0.5,
            Some((0.0, 1.0)),
            "how far the beam has travelled: its front is `time × (1 + length)` of the way \
             along the line, so with `length` 1 it spans the line at 0.5",
        ),
        number(
            "start-thickness",
            8.0,
            Some((0.0, 4_000.0)),
            "how thick the beam is at `start`, in px",
        ),
        number(
            "end-thickness",
            8.0,
            Some((0.0, 4_000.0)),
            "how thick the beam is at `end`, in px",
        ),
        number(
            "softness",
            0.5,
            Some((0.0, 1.0)),
            "how much of the half thickness fades out at the edges, 0 to 1",
        ),
        color(
            "inside-color",
            [1.0, 0.75, 0.98, 1.0],
            "the colour along the beam's middle",
        ),
        color(
            "outside-color",
            [1.0, 0.1, 0.1, 1.0],
            "the colour at the beam's edges",
        ),
    ],
};

/// The properties of `contour-strokes`.
pub static CONTOUR_STROKES: Registry = Registry {
    kind: "contour-strokes",
    rows: &[
        number(
            "threshold",
            0.498039,
            Some((0.0, 1.0)),
            "the level the contours follow, 0 to 1",
        ),
        number(
            "pre-blur",
            0.0,
            Some((0.0, 50.0)),
            "how much the level is blurred before it is traced, as a blurriness",
        ),
        number(
            "tolerance",
            0.5,
            Some((0.0, 5.0)),
            "how far, in px, a traced contour may stray when simplified",
        ),
        number(
            "contour",
            1.0,
            Some((1.0, 1000.0)),
            "which contour `render` `selected` strokes, counting from 1",
        ),
        number(
            "segments",
            32.0,
            Some((1.0, 250.0)),
            "how many strokes each contour takes",
        ),
        number(
            "length",
            1.0,
            Some((0.0, 1.0)),
            "how much of its share of the contour each stroke covers, 0 to 1",
        ),
        number(
            "rotation",
            0.0,
            None,
            "how far round each contour the strokes start, in degrees of a turn",
        ),
        number(
            "seed",
            1.0,
            Some((1.0, 1000.0)),
            "what the random phases are drawn from",
        ),
        color("color", [1.0, 1.0, 0.0, 1.0], "the strokes' colour"),
        number(
            "width",
            2.0,
            Some((0.5, 100.0)),
            "how wide the strokes are, in px",
        ),
        number(
            "hardness",
            0.0,
            Some((0.0, 1.0)),
            "how sharp the strokes' edges are, 0 soft to 1 hard",
        ),
        number(
            "start-opacity",
            1.0,
            Some((0.0, 1.0)),
            "how opaque each stroke is where it starts",
        ),
        number(
            "mid-opacity",
            0.0,
            Some((-1.0, 1.0)),
            "how much more opaque each stroke is at `mid-position` than the straight line from its start to its end",
        ),
        number(
            "mid-position",
            0.5,
            Some((0.0, 1.0)),
            "where along each stroke `mid-opacity` applies, 0 to 1",
        ),
        number(
            "end-opacity",
            0.0,
            Some((0.0, 1.0)),
            "how opaque each stroke is where it ends",
        ),
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

/// The properties of `linear-wipe`.
pub static LINEAR_WIPE: Registry = Registry {
    kind: "linear-wipe",
    rows: &[
        number(
            "completion",
            0.0,
            Some((0.0, 1.0)),
            "how far the edge has crossed the layer's box, 0 to 1",
        ),
        number(
            "angle",
            90.0,
            None,
            "which way the edge travels, in degrees clockwise from up; 90 runs left to right",
        ),
        number(
            "feather",
            0.0,
            Some((0.0, 32_000.0)),
            "how wide the edge's fade is, in px",
        ),
    ],
};

/// The properties of `iris-wipe`.
pub static IRIS_WIPE: Registry = Registry {
    kind: "iris-wipe",
    rows: &[
        point(
            "center",
            [0.0, 0.0],
            &[],
            false,
            "the middle of the hole, in px of the layer",
        ),
        number(
            "points",
            6.0,
            Some((3.0, 32.0)),
            "how many corners the hole's outline has",
        ),
        number(
            "outer-radius",
            0.0,
            Some((0.0, 32_000.0)),
            "how far its corners are from `center`, in px",
        ),
        number(
            "inner-radius",
            0.0,
            Some((0.0, 32_000.0)),
            "how far every other corner is from `center` with `inner` on, in px",
        ),
        number(
            "rotation",
            0.0,
            None,
            "how far the outline is turned clockwise from a corner straight up, in degrees",
        ),
        number(
            "feather",
            0.0,
            Some((0.0, 32_000.0)),
            "how wide the edge's fade is, inwards, in px",
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

/// The properties of the radial blur.
pub static RADIAL_BLUR: Registry = Registry {
    kind: "radial-blur",
    rows: &[
        number(
            "amount",
            0.0,
            Some((-3_600.0, 3_600.0)),
            "how long the streaks are: a percentage of each pixel's distance from `center` for \
             a zoom, degrees round it for a spin; negative runs them the other way",
        ),
        point(
            "center",
            [0.0, 0.0],
            &[],
            false,
            "the point the streaks run out from or round, in px of the layer",
        ),
    ],
};

/// The properties of the vector blur.
pub static VECTOR_BLUR: Registry = Registry {
    kind: "vector-blur",
    rows: &[
        number(
            "amount",
            0.0,
            Some((-4_000.0, 4_000.0)),
            "how long the streaks are, in px: at the map's steepest slope, or everywhere with \
             `constant-length`",
        ),
        number(
            "angle",
            0.0,
            None,
            "how far every streak turns from the map's slope, in degrees clockwise",
        ),
        number(
            "ridge-smoothness",
            1.0,
            Some((0.0, 1_000.0)),
            "how much the slope is softened, as a blurriness, so streaks shorten where it turns \
             sharply instead of flipping",
        ),
        number(
            "softness",
            15.0,
            Some((0.0, 1_000.0)),
            "how much the map's level is softened before its slope is taken, as a blurriness",
        ),
    ],
};

/// The properties of the lens blur.
pub static LENS_BLUR: Registry = Registry {
    kind: "lens-blur",
    rows: &[
        number(
            "radius",
            5.0,
            Some((0.0, 4_000.0)),
            "how far each pixel spreads, in px: the radius of the circle the iris's corners lie on",
        ),
        number(
            "roundness",
            0.0,
            Some((0.0, 1.0)),
            "how far the iris's sides bow out towards a circle, 0 to 1",
        ),
        number(
            "aspect",
            1.0,
            Some((0.01, 100.0)),
            "how many times wider than high the iris is",
        ),
        number(
            "rotation",
            0.0,
            None,
            "how far the iris turns, in degrees clockwise",
        ),
        number(
            "focal-distance",
            0.0,
            Some((0.0, 1.0)),
            "the level of `map` that stays sharp; a pixel spreads by `radius` times how far its \
             level is from it",
        ),
    ],
};

/// The properties of `bevel`.
pub static BEVEL: Registry = Registry {
    kind: "bevel",
    rows: &[
        number(
            "thickness",
            2.0,
            Some((0.0, 200.0)),
            "how far in from the layer's outline its surface rises, in px",
        ),
        number(
            "angle",
            -60.0,
            None,
            "where the light comes from, in degrees clockwise from straight up",
        ),
        color("color", WHITE, "the light's colour"),
        number(
            "intensity",
            0.4,
            Some((0.0, 1.0)),
            "how far a slope facing the light goes towards its colour, and one facing away \
             towards black, 0 to 1",
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

/// How the glow combines with its layer, by the names of the rasteriser's
/// operations; the first is the default.
const GLOW_OPERATIONS: &[&str] = &[
    "add",
    "normal",
    "multiply",
    "dissolve",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "darken",
    "lighten",
    "difference",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "color-dodge",
    "color-burn",
    "exclusion",
    "stencil-alpha",
    "stencil-luma",
    "silhouette-alpha",
    "silhouette-luma",
    "premultiplied-over",
    "alpha-add",
];

/// How a generator draws over its layer, by the names of the rasteriser's
/// operations; the first is the default.
const GENERATOR_MODES: &[&str] = &[
    "replace",
    "over",
    "add",
    "multiply",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "color-dodge",
    "color-burn",
    "darken",
    "lighten",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "stencil-alpha",
];

/// How fractal noise draws over its layer, as [`GENERATOR_MODES`] with
/// `over` first, its default.
const NOISE_MODES: &[&str] = &[
    "over",
    "replace",
    "add",
    "multiply",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "color-dodge",
    "color-burn",
    "darken",
    "lighten",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "stencil-alpha",
];

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

/// The properties of `find-edges`.
pub static FIND_EDGES: Registry = Registry {
    kind: "find-edges",
    rows: &[number(
        "blend",
        0.0,
        Some((0.0, 1.0)),
        "how much of the layer as it was shows through the lines, 0 to 1",
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

/// A path property resting at the empty path.
const fn path(name: &'static str, doc: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        ty: ValueType::Path,
        rest: Rest::Value(Value::Path(Path::EMPTY)),
        components: &[],
        range: None,
        broadcast: false,
        spatial: false,
        integral: false,
        doc,
    }
}

/// The properties of `color-cycle`: the cycle as one curve of the phase
/// per channel, `M x,y L x,y …` from phase to level, both 0 to 1, a point
/// per stop in rising phase, at most 16; then how the phase is moved and
/// how much of the original stays.
pub static COLOR_CYCLE: Registry = Registry {
    kind: "color-cycle",
    rows: &[
        path(
            "red",
            "the cycle's red at each phase; empty, the phase itself",
        ),
        path(
            "green",
            "the cycle's green at each phase; empty, the phase itself",
        ),
        path(
            "blue",
            "the cycle's blue at each phase; empty, the phase itself",
        ),
        path("alpha", "the cycle's alpha at each phase; empty, 1"),
        number(
            "shift",
            0.0,
            None,
            "how far round the cycle the phase is moved, in degrees",
        ),
        number(
            "repetitions",
            1.0,
            Some((0.0, 64.0)),
            "how many times the cycle runs over the phase",
        ),
        number(
            "blend",
            0.0,
            Some((0.0, 1.0)),
            "how much of the original stays, 0 to 1",
        ),
    ],
};

/// The levels a colour cycle's phase is read from.
const PHASES: &[&str] = &[
    "intensity",
    "red",
    "green",
    "blue",
    "hue",
    "lightness",
    "saturation",
    "value",
    "alpha",
    "zero",
];

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
            integral: false,
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
            integral: false,
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
            integral: false,
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
            integral: false,
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
            integral: false,
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

/// The properties of a merge: none, its mode is a setting.
pub static MERGE: Registry = Registry {
    kind: "merge",
    rows: &[],
};

/// The properties of round corners.
pub static ROUND_CORNERS: Registry = Registry {
    kind: "round-corners",
    rows: &[number(
        "radius",
        10.0,
        Some((0.0, f64::INFINITY)),
        "how far back along each side a corner is cut, in px, at most half the side",
    )],
};

/// The properties of a repeater. Copy `k` of `n` takes the transform
/// `k + offset` times over: moved by that many `position`s, turned by that
/// many `rotate`s and scaled by `scale` to that power, both about `anchor`.
pub static REPEATER: Registry = Registry {
    kind: "repeater",
    rows: &[
        number(
            "copies",
            3.0,
            Some((0.0, f64::INFINITY)),
            "how many copies are drawn, the first in place; a fractional part draws one more, \
             faded by that part",
        ),
        number(
            "offset",
            0.0,
            None,
            "how many steps of the transform every copy takes before its own, fractional or \
             negative",
        ),
        PropertySpec {
            name: "position",
            ty: ValueType::Point,
            rest: Rest::Value(Value::Point([100.0, 0.0])),
            components: &["x", "y"],
            range: None,
            broadcast: false,
            spatial: true,
            integral: false,
            doc: "how far each copy moves from the one before, in px of the group",
        },
        PropertySpec {
            name: "anchor",
            ty: ValueType::Point,
            rest: Rest::Value(Value::Point([0.0, 0.0])),
            components: &["anchor-x", "anchor-y"],
            range: None,
            broadcast: false,
            spatial: true,
            integral: false,
            doc: "the point each copy turns and scales about, in px of the group",
        },
        point(
            "scale",
            [1.0, 1.0],
            &["scale-x", "scale-y"],
            true,
            "the size factor of each copy over the one before, on each axis; one number scales \
             both",
        ),
        number(
            "rotate",
            0.0,
            None,
            "the turn of each copy over the one before, in degrees, clockwise",
        ),
        number(
            "start-opacity",
            1.0,
            Some((0.0, 1.0)),
            "the opacity of the first copy, 0 (invisible) to 1 (solid)",
        ),
        number(
            "end-opacity",
            1.0,
            Some((0.0, 1.0)),
            "the opacity the copies fade towards, one step past the last",
        ),
    ],
};

/// The properties of an expansion.
pub static EXPAND: Registry = Registry {
    kind: "expand",
    rows: &[
        number(
            "amount",
            10.0,
            None,
            "how far the outlines move out, in px; a negative amount moves them in",
        ),
        number(
            "miter",
            4.0,
            Some((1.0, f64::INFINITY)),
            "how far a `miter` join may reach from its corner, in amounts, before it is \
             bevelled",
        ),
        number(
            "copies",
            1.0,
            Some((0.0, f64::INFINITY)),
            "how many expanded outlines each outline becomes, each one amount further out; \
             counts by its whole part",
        ),
        number(
            "copy-offset",
            0.0,
            None,
            "how many amounts every copy moves further out",
        ),
    ],
};

/// Every kind of shape item.
pub static SHAPES: [ItemKind; 12] = [
    ItemKind {
        name: "group",
        role: Role::Group,
        registry: &GROUP,
        primary: Primary::Items(&SHAPES),
        settings: &[],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
        doc: "cuts the outlines above it, and those in the groups above it, down to a stretch \
              of their length, for every paint that draws them",
    },
    ItemKind {
        name: "merge",
        role: Role::Modifier,
        registry: &MERGE,
        primary: Primary::Setting("mode"),
        settings: &[SettingSpec {
            name: "mode",
            choices: &["merge", "add", "subtract", "intersect", "exclude"],
            default: "merge",
            doc: "`merge` keeps the outlines as they are, one after another; the others combine \
                  the areas inside them, the first outline with each next one in turn: `add` \
                  takes either, `subtract` the first without the next, `intersect` both, \
                  `exclude` one but not both",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "replaces the outlines above it, and those in the groups above it, by one outline \
              in its group; the paints above it no longer draw them",
    },
    ItemKind {
        name: "round-corners",
        role: Role::Modifier,
        registry: &ROUND_CORNERS,
        primary: Primary::Property("radius"),
        settings: &[],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "rounds the sharp corners of the outlines above it, and of those in the groups \
              above it, for every paint that draws them",
    },
    ItemKind {
        name: "repeater",
        role: Role::Repeater,
        registry: &REPEATER,
        primary: Primary::Property("copies"),
        settings: &[SettingSpec {
            name: "composite",
            choices: &["below", "above"],
            default: "below",
            doc: "`below` draws each copy under the one before, so the first is on top; `above` \
                  draws each over it",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "draws everything above it in its group again as copies, outlines and paints \
              alike, each moved, turned, scaled and faded one step further",
    },
    ItemKind {
        name: "expand",
        role: Role::Modifier,
        registry: &EXPAND,
        primary: Primary::Property("amount"),
        settings: &[SettingSpec {
            name: "join",
            choices: &["miter", "round", "bevel"],
            default: "miter",
            doc: "how the moved sides meet where the outline turns away from them",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "moves the outlines above it, and those in the groups above it, out by an amount, \
              for every paint that draws them",
    },
];

/// Every kind of effect.
pub static EFFECTS: [ItemKind; 45] = [
    ItemKind {
        name: "fill",
        role: Role::Effect,
        registry: &FILL_EFFECT,
        primary: Primary::Property("color"),
        settings: &[SettingSpec {
            name: "inverted",
            choices: &["off", "on"],
            default: "off",
            doc: "`on` fills where the layer does not draw instead, inside its box: the colour \
                  takes the layer's transparency as its coverage, and what the layer drew is \
                  cleared",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "turns every drawn pixel of the layer one colour, keeping its coverage; inverted, \
              fills the rest of the layer's box",
    },
    ItemKind {
        name: "solid-composite",
        role: Role::Effect,
        registry: &SOLID_COMPOSITE,
        primary: Primary::Property("color"),
        settings: &[SettingSpec {
            name: "mode",
            choices: SOLID_COMPOSITE_MODES,
            default: "normal",
            doc: "how the layer is laid over the solid: `normal`, `add` or the blend mode of \
                  that name",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "lays the layer, at `source-opacity`, over a solid of `color` at `opacity` that \
              fills the layer's box",
    },
    ItemKind {
        name: "transform",
        role: Role::Effect,
        registry: &TRANSFORM_EFFECT,
        primary: Primary::Property("scale"),
        settings: &[],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
        doc: "turns hues and changes saturation and lightness",
    },
    ItemKind {
        name: "color-cycle",
        role: Role::Effect,
        registry: &COLOR_CYCLE,
        primary: Primary::Setting("phase-from"),
        settings: &[
            SettingSpec {
                name: "phase-from",
                choices: PHASES,
                default: "intensity",
                doc: "which level of a pixel's straight colour is its phase: `intensity` the mean \
                      of its colours, `value` the largest, `zero` none",
            },
            SettingSpec {
                name: "add-from",
                choices: PHASES,
                default: "intensity",
                doc: "which level of the `add` layer is added to the phase",
            },
            SettingSpec {
                name: "add-mode",
                choices: &["wrap", "clamp", "average", "screen"],
                default: "wrap",
                doc: "how the added level joins the phase: summed and wrapped, summed and held at \
                      1, averaged, or screened",
            },
            SettingSpec {
                name: "modify",
                choices: &[
                    "all",
                    "red",
                    "green",
                    "blue",
                    "red-green",
                    "green-blue",
                    "red-blue",
                    "hue",
                    "lightness",
                    "saturation",
                    "hue-lightness",
                    "lightness-saturation",
                    "hue-saturation",
                    "none",
                ],
                default: "all",
                doc: "which channels the cycle's colour replaces: some of red, green and blue, or \
                      of hue, lightness and saturation, or none",
            },
            SettingSpec {
                name: "interpolate",
                choices: &["on", "off"],
                default: "on",
                doc: "whether the cycle runs straight from point to point (`on`) or holds each \
                      point's colour until the next (`off`)",
            },
            SettingSpec {
                name: "modify-alpha",
                choices: &["on", "off"],
                default: "on",
                doc: "whether the cycle's alpha multiplies the pixel's",
            },
        ],
        input: Some(Input {
            key: "add",
            reads: Reads::Drawing,
        }),
        outline: None,
        pivots: None,
        timed: false,
        doc: "sends a phase read off each pixel, plus one read off the `add` layer, round a cycle \
              of colours that replace the pixel's",
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: true,
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
        pivots: None,
        timed: false,
        doc: "moves each pixel by the levels of another layer, the map",
    },
    ItemKind {
        name: "set-matte",
        role: Role::Effect,
        registry: &SET_MATTE,
        primary: Primary::Setting("channel"),
        settings: &[
            SettingSpec {
                name: "channel",
                choices: MATTE_LEVELS,
                default: "alpha",
                doc: "which level of the matte layer becomes the layer's alpha; `full` keeps all \
                      of it and `off` none",
            },
            SettingSpec {
                name: "inverted",
                choices: &["off", "on"],
                default: "off",
                doc: "whether the matte is turned into its opposite, keeping the layer where the \
                      level is low",
            },
            SettingSpec {
                name: "behaviour",
                choices: &["stretch", "center"],
                default: "stretch",
                doc: "how the matte layer's box lies on the layer's when their sizes differ: \
                      stretched over it, or centred on it at its own size",
            },
            SettingSpec {
                name: "composite",
                choices: &["on", "off"],
                default: "on",
                doc: "whether the matte multiplies the layer's own alpha (`on`) or replaces it \
                      inside the layer's box (`off`)",
            },
            SettingSpec {
                name: "premultiply",
                choices: &["on", "off"],
                default: "on",
                doc: "whether the matte layer's colour is read laid over black (`on`), so its \
                      transparent parts read as 0, or as its own colour (`off`)",
            },
        ],
        input: Some(Input {
            key: "matte",
            reads: Reads::Laid,
        }),
        outline: None,
        pivots: None,
        timed: false,
        doc: "takes the layer's alpha from a level of another layer, the matte, or of its own \
              drawing before its effects when it names none",
    },
    ItemKind {
        name: "shift-channels",
        role: Role::Effect,
        registry: &SHIFT_CHANNELS,
        primary: Primary::Setting("alpha-from"),
        settings: &[
            SettingSpec {
                name: "alpha-from",
                choices: shift_levels!("alpha", "red", "green", "blue"),
                default: "alpha",
                doc: "which level of the layer's own colour becomes its alpha",
            },
            SettingSpec {
                name: "red-from",
                choices: shift_levels!("red", "alpha", "green", "blue"),
                default: "red",
                doc: "which level becomes its red",
            },
            SettingSpec {
                name: "green-from",
                choices: shift_levels!("green", "alpha", "red", "blue"),
                default: "green",
                doc: "which level becomes its green",
            },
            SettingSpec {
                name: "blue-from",
                choices: shift_levels!("blue", "alpha", "red", "green"),
                default: "blue",
                doc: "which level becomes its blue",
            },
        ],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "rebuilds each channel from a level of the layer's straight colour: a channel, \
              luminance, hue, lightness, saturation, or fixed `full` or `off`",
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
        doc: "turns the layer between rows and columns and rings about its middle",
    },
    ItemKind {
        name: "lens-distortion",
        role: Role::Effect,
        registry: &LENS_DISTORTION,
        primary: Primary::Property("field-of-view"),
        settings: &[
            SettingSpec {
                name: "distortion",
                choices: &["add", "remove"],
                default: "add",
                doc: "`add` draws the layer as the lens sees it, drawn in towards its axis; \
                      `remove` undoes the lens, stretching the layer out from its axis",
            },
            SettingSpec {
                name: "field",
                choices: &["horizontal", "vertical", "diagonal"],
                default: "horizontal",
                doc: "the span of the layer's box the field of view covers: its width, its \
                      height or its diagonal",
            },
            SettingSpec {
                name: "grow",
                choices: &["off", "2x", "4x"],
                default: "off",
                doc: "whether the layer's box grows to hold what the lens draws beyond it, \
                      up to twice or four times its width and height about its middle",
            },
        ],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "bends the layer about `center` as a lens with a wide field of view draws it, \
              or undoes that bend",
    },
    ItemKind {
        name: "bezier-warp",
        role: Role::Effect,
        registry: &BEZIER_WARP,
        primary: Primary::Property("top-left"),
        settings: &[],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "lays the layer's box on a patch whose four sides are cubic curves through its \
              corners and handles, each moved from where it rests on the box",
    },
    ItemKind {
        name: "corner-pin",
        role: Role::Effect,
        registry: &CORNER_PIN,
        primary: Primary::Property("top-left"),
        settings: &[],
        input: None,
        outline: None,
        pivots: Some("pivots"),
        timed: false,
        doc: "lays the layer's box onto four corners through the perspective that takes it \
              there, each corner moved from where it rests: the box's corner, or where the \
              layer `pivots` names for it lands",
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
        doc: "fills the layer with a gradient between two colours",
    },
    ItemKind {
        name: "four-color",
        role: Role::Effect,
        registry: &FOUR_COLOR,
        primary: Primary::Property("opacity"),
        settings: &[SettingSpec {
            name: "mode",
            choices: GENERATOR_MODES,
            default: "replace",
            doc: "`replace` draws it alone where the layer is; `over` lays it over the layer; \
                  the others blend it with the layer by the operation of that name, then lay \
                  that over by its opacity",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "fills the layer with four colours at four points, blended",
    },
    ItemKind {
        name: "beam",
        role: Role::Effect,
        registry: &BEAM,
        primary: Primary::Property("time"),
        settings: &[SettingSpec {
            name: "composite",
            choices: &["off", "on"],
            default: "off",
            doc: "whether the layer's own drawing stays under the beam",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "draws a beam `length` of the line from `start` to `end` long, travelling along \
              it with `time`, its thickness running from `start-thickness` to \
              `end-thickness`, in place of the layer, kept to its box",
    },
    ItemKind {
        name: "contour-strokes",
        role: Role::Effect,
        registry: &CONTOUR_STROKES,
        primary: Primary::Property("width"),
        settings: &[
            SettingSpec {
                name: "source",
                choices: &["contours", "outline"],
                default: "contours",
                doc: "what the strokes follow: the contours of the layer's drawing, or the outline of the layer's mask `along` names",
            },
            SettingSpec {
                name: "channel",
                choices: &[
                    "intensity",
                    "red",
                    "green",
                    "blue",
                    "alpha",
                    "hue",
                    "lightness",
                    "saturation",
                    "value",
                ],
                default: "intensity",
                doc: "the level the contours follow",
            },
            SettingSpec {
                name: "render",
                choices: &["all", "selected"],
                default: "all",
                doc: "whether every contour is stroked or only the `contour`-th",
            },
            SettingSpec {
                name: "shorter",
                choices: &["same", "fewer"],
                default: "same",
                doc: "whether shorter contours take as many strokes as the longest, or fewer in proportion",
            },
            SettingSpec {
                name: "random-phase",
                choices: &["off", "on"],
                default: "off",
                doc: "whether each contour's strokes start a random share further round",
            },
            SettingSpec {
                name: "mode",
                choices: &["transparent", "over", "under", "stencil"],
                default: "transparent",
                doc: "how the strokes and the layer combine: the strokes alone, over or under it, or the layer only where they are",
            },
        ],
        input: Some(Input {
            key: "layer",
            reads: Reads::Drawing,
        }),
        outline: Some("along"),
        pivots: None,
        timed: false,
        doc: "draws strokes along the contours where a level of the layer's drawing, or of the \
              layer `layer` names, crosses `threshold`, or along the outline of its mask `along` \
              names",
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
                choices: GENERATOR_MODES,
                default: "replace",
                doc: "`replace` draws it alone where the layer is; `over` lays it over the layer; \
                      the others blend it with the layer by the operation of that name, then lay \
                      that over by its opacity",
            },
        ],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
        doc: "hides the layer in stripes, from one side of each",
    },
    ItemKind {
        name: "linear-wipe",
        role: Role::Effect,
        registry: &LINEAR_WIPE,
        primary: Primary::Property("completion"),
        settings: &[],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "hides the layer behind a straight edge that crosses its box, from 0 (all shown) \
              to 1 (all hidden)",
    },
    ItemKind {
        name: "iris-wipe",
        role: Role::Effect,
        registry: &IRIS_WIPE,
        primary: Primary::Property("outer-radius"),
        settings: &[SettingSpec {
            name: "inner",
            choices: &["off", "on"],
            default: "off",
            doc: "whether every other corner sits at `inner-radius`, making the outline a star",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "cuts a hole in the layer, a polygon or star around `center`",
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
                    "smeary",
                    "swirly",
                ],
                default: "basic",
                doc: "how octaves sum: as they are, folded about mid grey, or each moved by the octaves before it (`smeary` across, `swirly` turning)",
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
                choices: NOISE_MODES,
                default: "over",
                doc: "`over` lays it over the layer; `replace` draws it alone where the layer is; \
                      the others blend it with the layer by the operation of that name, then lay \
                      that over by its opacity",
            },
        ],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
        doc: "blurs the layer with boxes run one after another",
    },
    ItemKind {
        name: "radial-blur",
        role: Role::Effect,
        registry: &RADIAL_BLUR,
        primary: Primary::Property("amount"),
        settings: &[
            SettingSpec {
                name: "type",
                choices: &["zoom", "spin"],
                default: "zoom",
                doc: "which lines the streaks run along: rays out from `center`, or rings round it",
            },
            SettingSpec {
                name: "stretch",
                choices: &["behind", "across"],
                default: "behind",
                doc: "where along its line each pixel takes its mean: from itself back towards \
                      `center` or clockwise, so the layer streaks outwards or counterclockwise, \
                      or half of it either side",
            },
            SettingSpec {
                name: "fade",
                choices: &["none", "linear"],
                default: "none",
                doc: "how the pixels of a streak weigh: all alike, or from all at the pixel to \
                      none at the streak's end",
            },
        ],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "streaks the layer along rays out from a point, or rings round it",
    },
    ItemKind {
        name: "vector-blur",
        role: Role::Effect,
        registry: &VECTOR_BLUR,
        primary: Primary::Property("amount"),
        settings: &[
            SettingSpec {
                name: "type",
                choices: &[
                    "natural",
                    "constant-length",
                    "perpendicular",
                    "direction-center",
                    "direction-fading",
                ],
                default: "natural",
                doc: "how each streak follows the slope: both ways and as long as the slope is \
                      steep, always `amount` long, across the slope, or behind the pixel only, \
                      flat or fading",
            },
            SettingSpec {
                name: "map-level",
                choices: &[
                    "lightness",
                    "red",
                    "green",
                    "blue",
                    "alpha",
                    "luminance",
                    "hue",
                    "saturation",
                ],
                default: "lightness",
                doc: "which level of `map` the slope is taken of",
            },
        ],
        input: Some(Input {
            key: "map",
            reads: Reads::Drawing,
        }),
        outline: None,
        pivots: None,
        timed: false,
        doc: "smears the layer along the slope of a map's level, its own without one",
    },
    ItemKind {
        name: "lens-blur",
        role: Role::Effect,
        registry: &LENS_BLUR,
        primary: Primary::Property("radius"),
        settings: &[
            SettingSpec {
                name: "iris",
                choices: &[
                    "hexagon", "triangle", "square", "pentagon", "heptagon", "octagon", "nonagon",
                    "decagon",
                ],
                default: "hexagon",
                doc: "the iris's shape: a polygon of three to ten sides, six unless set",
            },
            SettingSpec {
                name: "map-level",
                choices: &["luminance", "red", "green", "blue", "alpha"],
                default: "luminance",
                doc: "which level of `map` sets each pixel's radius",
            },
            SettingSpec {
                name: "placement",
                choices: &["center", "stretch"],
                default: "center",
                doc: "how `map`'s box lies on the layer's: centred on it, or stretched over it",
            },
            SettingSpec {
                name: "inverted",
                choices: &["off", "on"],
                default: "off",
                doc: "whether `map`'s level is taken from 1",
            },
            SettingSpec {
                name: "edges",
                choices: &["transparent", "repeat"],
                default: "transparent",
                doc: "what lies beyond the frame's edges while blurring: nothing, or the edge pixels repeated",
            },
        ],
        input: Some(Input {
            key: "map",
            reads: Reads::Drawing,
        }),
        outline: None,
        pivots: None,
        timed: false,
        doc: "spreads each pixel over the shape of a lens's iris, by a map's levels or evenly",
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
        pivots: None,
        timed: false,
        doc: "draws the layer's shape as a soft shadow behind it",
    },
    ItemKind {
        name: "bevel",
        role: Role::Effect,
        registry: &BEVEL,
        primary: Primary::Property("thickness"),
        settings: &[],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "lights the layer as if its alpha rose from its outline over `thickness` px, \
              from `angle`",
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
                choices: &["with-original", "glow-only", "behind-original"],
                default: "with-original",
                doc: "`with-original` lays the glow over the layer by `operation`; `glow-only` \
                      draws the glow alone; `behind-original` draws it behind the layer, laid \
                      over it by `operation`",
            },
            SettingSpec {
                name: "operation",
                choices: GLOW_OPERATIONS,
                default: "add",
                doc: "how the glow and the layer combine: `add` sums them; `normal` and the \
                      blend modes of the same names blend them; `dissolve` shows the glow at a \
                      scattered share of the pixels as large as its alpha; `stencil-alpha` and \
                      `stencil-luma` keep the layer where the glow has alpha or brightness, \
                      `silhouette-alpha` and `silhouette-luma` cut it out there; \
                      `premultiplied-over` lays the glow over without holding its colours to its \
                      alpha; `alpha-add` adds their alphas",
            },
        ],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
        doc: "sharpens edges by moving pixels away from their neighbours",
    },
    ItemKind {
        name: "find-edges",
        role: Role::Effect,
        registry: &FIND_EDGES,
        primary: Primary::Setting("lines"),
        settings: &[SettingSpec {
            name: "lines",
            choices: &["dark-on-light", "light-on-dark"],
            default: "dark-on-light",
            doc: "whether the edges are dark lines on white or light lines on black",
        }],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
        doc: "draws where the colours change as lines, by each channel's slope across the \
              pixels around",
    },
    ItemKind {
        name: "black-white",
        role: Role::Effect,
        registry: &BLACK_WHITE,
        primary: Primary::Property("tint"),
        settings: &[],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
        integral: false,
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
            "skew",
            0.0,
            Some((-85.0, 85.0)),
            "how far each part slants about its anchor, in degrees: positive leans its top \
             forward along `skew-axis`",
        ),
        number(
            "skew-axis",
            0.0,
            None,
            "the direction a part slants along, in degrees clockwise from across",
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
        point(
            "blur",
            [0.0, 0.0],
            &["blur-x", "blur-y"],
            true,
            "how much each part blurs across and down, in px of the Gaussian blur's blurriness; \
             one number blurs both ways",
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
/// of the parts it counts, 0 before the first and 1 after the last, or in
/// `index` units the parts themselves, `n` after the last of `n`.
pub static RANGE: Registry = Registry {
    kind: "range",
    rows: &[
        number(
            "start",
            0.0,
            None,
            "where the range starts, as a fraction of the parts or, in `index` units, a number of parts",
        ),
        number(
            "end",
            1.0,
            None,
            "where the range ends, as a fraction of the parts or, in `index` units, a number of parts",
        ),
        number(
            "offset",
            0.0,
            None,
            "how far start and end both move, as a fraction of the parts or, in `index` units, \
             a number of parts",
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
    pivots: None,
    timed: false,
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
    pivots: None,
    timed: false,
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
            SettingSpec {
                name: "units",
                choices: &["fraction", "index"],
                default: "fraction",
                doc: "what `start`, `end` and `offset` count: fractions of the parts, or parts",
            },
        ],
        input: None,
        outline: None,
        pivots: None,
        timed: false,
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
        pivots: None,
        timed: false,
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
            integral: false,
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
            integral: false,
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
    pivots: None,
    timed: false,
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
        ANIMATOR, ELLIPSE, EXPAND, FILL, FILL_EFFECT, MASK, PART, PATH, Prop, RANGE, RECT,
        REPEATER, ROUND_CORNERS, STAR, STROKE, TRIM,
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
    /// How far round corners cut back each side of a corner.
    pub const ROUND_CORNERS_RADIUS: Prop = Prop::of(&ROUND_CORNERS, 0);
    /// How many copies a repeater draws.
    pub const REPEATER_COPIES: Prop = Prop::of(&REPEATER, 0);
    /// How many steps of its transform a repeater's copies start along.
    pub const REPEATER_OFFSET: Prop = Prop::of(&REPEATER, 1);
    /// How far each of a repeater's copies moves from the one before.
    pub const REPEATER_POSITION: Prop = Prop::of(&REPEATER, 2);
    /// The point a repeater's copies turn and scale about.
    pub const REPEATER_ANCHOR: Prop = Prop::of(&REPEATER, 3);
    /// The size factor of each of a repeater's copies over the one before.
    pub const REPEATER_SCALE: Prop = Prop::of(&REPEATER, 4);
    /// The turn of each of a repeater's copies over the one before.
    pub const REPEATER_ROTATE: Prop = Prop::of(&REPEATER, 5);
    /// The opacity of a repeater's first copy.
    pub const REPEATER_START_OPACITY: Prop = Prop::of(&REPEATER, 6);
    /// The opacity a repeater's copies fade towards.
    pub const REPEATER_END_OPACITY: Prop = Prop::of(&REPEATER, 7);
    /// How far an expansion moves outlines out, in px.
    pub const EXPAND_AMOUNT: Prop = Prop::of(&EXPAND, 0);
    /// How far an expansion's miter joins may reach, in amounts.
    pub const EXPAND_MITER: Prop = Prop::of(&EXPAND, 1);
    /// How many expanded outlines an expansion makes of each.
    pub const EXPAND_COPIES: Prop = Prop::of(&EXPAND, 2);
    /// How many amounts an expansion's copies move further out.
    pub const EXPAND_COPY_OFFSET: Prop = Prop::of(&EXPAND, 3);
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
    /// How far an animator slants each part.
    pub const ANIMATOR_SKEW: Prop = Prop::of(&ANIMATOR, 4);
    /// The direction an animator slants each part along.
    pub const ANIMATOR_SKEW_AXIS: Prop = Prop::of(&ANIMATOR, 5);
    /// The opacity an animator fades each part to.
    pub const ANIMATOR_OPACITY: Prop = Prop::of(&ANIMATOR, 6);
    /// The fill colour an animator turns each part to.
    pub const ANIMATOR_FILL: Prop = Prop::of(&ANIMATOR, 7);
    /// The stroke colour an animator turns each part to.
    pub const ANIMATOR_STROKE: Prop = Prop::of(&ANIMATOR, 8);
    /// The stroke width an animator turns each part to.
    pub const ANIMATOR_STROKE_WIDTH: Prop = Prop::of(&ANIMATOR, 9);
    /// The tracking an animator adds after each part.
    pub const ANIMATOR_TRACKING: Prop = Prop::of(&ANIMATOR, 10);
    /// How much an animator blurs each part.
    pub const ANIMATOR_BLUR: Prop = Prop::of(&ANIMATOR, 11);
    /// How many frames each part of a staggered animator waits.
    pub const ANIMATOR_STAGGER: Prop = Prop::of(&ANIMATOR, 12);
    /// Which shuffle a staggered animator's `random` order takes.
    pub const ANIMATOR_SEED: Prop = Prop::of(&ANIMATOR, 13);
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
