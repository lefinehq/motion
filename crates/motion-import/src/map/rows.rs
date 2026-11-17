//! The mapping tables: for each project group the scene draws, the item kind
//! it becomes and a row per property, saying where the property goes, how its
//! value converts and what it is when the project leaves it out. Adding a
//! supported property is adding a row; a test checks every row's target
//! against the scene format's registries.

use motion_format::item::{ANIMATORS, EFFECTS, ItemKind, MASKS, SELECTORS, SHAPES};

use super::keys::Convert;

/// Where a property goes.
#[derive(Debug, Clone, Copy)]
pub(super) enum To {
    /// A property of the item, or one component of it, by its scene name.
    Prop(&'static str),
    /// A 3D vector as two properties: its first two numbers to the point
    /// property named first, through the row's conversion, and its third
    /// to the number property named second, its depth ([`Convert::Depth`]).
    /// A point that moves along a path moves along it in space.
    Split(&'static str, &'static str),
    /// A setting of the item: the project's code for each choice.
    Setting(&'static str, &'static [(i64, &'static str)]),
    /// The layer the kind's input key names, from a layer parameter.
    Input,
    /// The mask the kind's outline key names, from a path parameter.
    Outline,
    /// The five curves of a curves effect, from the data the project keeps
    /// for them (`aRbp`).
    Curves,
    /// The five sets of levels of a levels effect, from the data the
    /// project keeps for them in its histogram parameter.
    Levels,
    /// Nowhere: it changes nothing a frame shows, for this reason.
    Ignore(&'static str),
    /// Nowhere, which is right only at its default; any other value is
    /// reported as unsupported, for this reason.
    OnlyDefault(&'static str),
    /// Nowhere, which is right at its default, and at any value while the
    /// item's property named second holds 0 all the time or is left out:
    /// it only says how that property applies. Otherwise it is reported as
    /// unsupported, for the reason given first.
    OnlyDefaultWith(&'static str, &'static str),
}

/// What a property is when the project leaves it out, in the project's units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Missing {
    /// Nothing to write: the scene's own rest is the same, or an item applies
    /// only what it lists (an animator).
    Absent,
    /// A number.
    Number(f64),
    /// A vector.
    Vector([f64; 2]),
    /// A colour, `[r, g, b, a]`.
    Color([f64; 4]),
    /// The centre of the layer's composition.
    CompCentre,
    /// A camera's zoom: a 50 mm lens on 36 mm film over the composition's
    /// width ([`motion_format::property::default_zoom`]).
    CameraZoom,
    /// A camera's position: the composition's centre, as far in front of
    /// it as the default zoom ([`Missing::CameraZoom`]), so it sees the
    /// plane at depth 0 at 100 %.
    CameraPosition,
    /// Where a layer's anchor point rests: the middle of its source, the
    /// top-left corner of a null's, or the origin for a layer without one.
    SourceCentre,
    /// The default the project's effect definition gives.
    Parameter,
}

/// One property of a group.
#[derive(Debug, Clone, Copy)]
pub(super) struct Row {
    /// The property's match name.
    pub match_name: &'static str,
    /// Where it goes.
    pub to: To,
    /// How its value converts.
    pub convert: Convert,
    /// Its default.
    pub default: Missing,
}

/// A shorthand for a row.
const fn row(match_name: &'static str, to: To, convert: Convert, default: Missing) -> Row {
    Row {
        match_name,
        to,
        convert,
        default,
    }
}

/// What a child group of an item holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Part {
    /// The item's own items: a shape group's shapes, an animator's selectors.
    Items,
    /// More of the item's own properties.
    Rows,
}

/// One project group that becomes a scene item.
#[derive(Debug)]
pub(super) struct Kind {
    /// The group's match name.
    pub match_name: &'static str,
    /// The scene kind it becomes.
    pub item: &'static str,
    /// The scene table that kind is in.
    pub table: &'static [ItemKind],
    /// Its properties, wherever they sit among its parts.
    pub rows: &'static [Row],
    /// Its child groups by match name, and what each holds.
    pub parts: &'static [(&'static str, Part)],
    /// A checkbox that picks other rows when it is off.
    pub switch: Option<Switch>,
}

/// A property of a kind that changes where other properties go, such as a
/// transform's uniform scale or a range selector's units.
#[derive(Debug)]
pub(super) struct Switch {
    /// The property's match name.
    pub match_name: &'static str,
    /// The code it holds where the project leaves it out.
    pub missing: f64,
    /// Whether a code picks the kind's own rows; any other picks `off_rows`.
    pub on: fn(f64) -> bool,
    /// The kind's rows when it is off.
    pub off_rows: &'static [Row],
}

/// A checkbox, on unless it is 0.
fn checked(code: f64) -> bool {
    code != 0.0
}

impl Kind {
    /// The scene kind.
    pub(super) fn scene_kind(&self) -> &'static ItemKind {
        self.table
            .iter()
            .find(|kind| kind.name == self.item)
            .expect("a mapping table names a scene kind")
    }

    /// Its rows for a group whose properties are `properties`: the ones its
    /// switch picks, if it has one.
    pub(super) fn rows_for(&self, properties: &[&crate::model::Property]) -> &'static [Row] {
        let Some(switch) = &self.switch else {
            return self.rows;
        };
        let on = properties
            .iter()
            .find(|p| p.match_name == switch.match_name)
            .and_then(|p| match &p.value {
                crate::model::Stream::Static(crate::model::Value::Number(n)) => Some(*n),
                _ => None,
            })
            .map_or((switch.on)(switch.missing), switch.on);
        if on { self.rows } else { switch.off_rows }
    }

    /// What the child group `match_name` holds, if it is one of the kind's parts.
    pub(super) fn part(&self, match_name: &str) -> Option<Part> {
        self.parts
            .iter()
            .find(|(name, _)| *name == match_name)
            .map(|(_, part)| *part)
    }
}

/// What to do with a group that is not an item.
#[derive(Debug, Clone, Copy)]
pub(super) enum Policy {
    /// Its children are items or groups of the layer's own: a root vectors
    /// group, an effects parade, a text animators group.
    Through,
    /// It changes nothing a frame shows, for this reason.
    Ignore(&'static str),
    /// It is right only empty; anything in it is unsupported, for this reason.
    OnlyEmpty(&'static str),
    /// Layer styles: each style is a group that is right only switched off.
    Styles,
    /// An expression control: it draws nothing, and expressions read it, so
    /// the report keeps it with its values for live links (D44).
    Control,
    /// An effect the scene does not draw yet that draws something sparse of
    /// its own in place of what its layer draws: the original
    /// keeps none of the layer's pixels, only what the effect draws from
    /// them. The effect is unsupported, and its layer draws nothing rather
    /// than the solid the effect would have covered.
    ///
    /// Effects that fill their layer, such as gradients and noise, are not
    /// this: until the scene draws one, its solid stands closer to what it
    /// draws than nothing does. Nor are those that draw over the layer, such
    /// as a lens flare, which keep its pixels.
    InPlace,
}

/// A mask path, in the mask space until the import knows the layer's.
const MASK_PATH: Convert = Convert::PathIn([1.0, 1.0]);

/// The scene's mask modes by the project's (`PF_MaskMode`).
pub(super) const MASK_MODES: &[(u32, &str)] = &[
    (0, "none"),
    (1, "add"),
    (2, "subtract"),
    (3, "intersect"),
    (4, "lighten"),
    (5, "darken"),
    (6, "difference"),
];

const LINE_CAPS: &[(i64, &str)] = &[(1, "butt"), (2, "round"), (3, "square")];
const LINE_JOINS: &[(i64, &str)] = &[(1, "miter"), (2, "round"), (3, "bevel")];
const FILL_RULES: &[(i64, &str)] = &[(1, "nonzero"), (2, "evenodd")];
const STAR_TYPES: &[(i64, &str)] = &[(1, "star"), (2, "polygon")];
/// A shape's direction: 1 is the default, clockwise, and 3 reverses it. The
/// meaning of 2, which some paths, rectangles and ellipses store, is not
/// known, so it is reported.
const SHAPE_DIRECTIONS: &[(i64, &str)] = &[(1, "clockwise"), (3, "counterclockwise")];
/// Trim Multiple Shapes: simultaneously cuts each outline the same way,
/// individually cuts them one after another as one line.
const TRIM_MODES: &[(i64, &str)] = &[(1, "each"), (2, "joined")];
/// Merge Paths' modes, in the editor's order.
const MERGE_MODES: &[(i64, &str)] = &[
    (1, "merge"),
    (2, "add"),
    (3, "subtract"),
    (4, "intersect"),
    (5, "exclude"),
];
const COMPOSITES: &[(i64, &str)] = &[(1, "below"), (2, "above")];
const BASED_ON: &[(i64, &str)] = &[(1, "char"), (2, "char-no-space"), (3, "word"), (4, "line")];
const MODES: &[(i64, &str)] = &[
    (1, "add"),
    (2, "subtract"),
    (3, "intersect"),
    (4, "min"),
    (5, "max"),
    (6, "difference"),
];
const COLORIZE: &[(i64, &str)] = &[(0, "adjust"), (1, "colorize")];
const EXPOSURE_SPACES: &[(i64, &str)] = &[(0, "linear"), (1, "gamma")];
// The scene's particle units per one of CC Particle Systems II's: births
// a second per unit of birth rate, px a second (and a second per second)
// per unit of velocity and gravity, drag a second per unit of resistance,
// px per unit of size. Read off the corpus's reference frames.
const CC_RATE: f64 = 120.0;
const CC_SPEED: f64 = 250.0;
const CC_RESISTANCE: f64 = 0.1;
const CC_SIZE: f64 = 100.0;
const CC_ANIMATIONS: &[(i64, &str)] = &[
    (1, "explosive"),
    (2, "explosive"),
    (8, "direction"),
    (9, "direction"),
];
const CC_SHAPES: &[(i64, &str)] = &[(4, "disc"), (5, "faded"), (6, "faded"), (7, "bubble")];
const CC_OPACITY_MAPS: &[(i64, &str)] = &[
    (1, "constant"),
    (2, "fade-out-sharp"),
    (3, "fade-out"),
    (4, "fade-in"),
    (5, "fade-in-out"),
];
const CC_TRANSFERS: &[(i64, &str)] = &[(1, "over"), (2, "screen"), (3, "add")];
const MAP_LEVELS: &[(i64, &str)] = &[
    (1, "red"),
    (2, "green"),
    (3, "blue"),
    (4, "alpha"),
    (5, "luminance"),
    (6, "hue"),
    (7, "lightness"),
    (8, "saturation"),
    (9, "full"),
    (10, "half"),
    (11, "off"),
];
const MAP_BEHAVIOURS: &[(i64, &str)] = &[(1, "center"), (2, "stretch"), (3, "tile")];
const MAP_EDGES: &[(i64, &str)] = &[(0, "clear"), (1, "wrap")];
const TILE_EDGES: &[(i64, &str)] = &[(0, "repeat"), (1, "mirror")];
const TILE_SHIFTS: &[(i64, &str)] = &[(0, "vertical"), (1, "horizontal")];
const DISPLACEMENTS: &[(i64, &str)] = &[
    (1, "turbulent"),
    (2, "bulge"),
    (3, "twist"),
    (5, "turbulent-smoother"),
    (6, "bulge-smoother"),
    (7, "twist-smoother"),
    (9, "vertical"),
    (10, "horizontal"),
    (11, "cross"),
];
// Pinning the horizontal edges keeps the top and bottom put.
const PINNINGS: &[(i64, &str)] = &[
    (1, "none"),
    (3, "all"),
    (4, "top-bottom"),
    (5, "left-right"),
];
const RAMP_SHAPES: &[(i64, &str)] = &[(1, "linear"), (2, "radial")];
const OFF_ON: &[(i64, &str)] = &[(0, "off"), (1, "on")];
const AUDIO_PATHS: &[(i64, &str)] = &[(0, "line"), (1, "circle")];
const AUDIO_DISPLAYS: &[(i64, &str)] = &[(1, "digital"), (2, "lines"), (3, "dots")];
const AUDIO_SIDES: &[(i64, &str)] = &[(1, "a"), (2, "b"), (3, "both")];
/// The tilings the scene draws, of the 16 the project offers: the
/// others are reported.
const TILINGS: &[(i64, &str)] = &[(1, "repeat"), (4, "unfold"), (17, "brick"), (11, "none")];
const POLAR_CONVERSIONS: &[(i64, &str)] = &[(1, "rect-to-polar"), (2, "polar-to-rect")];
const GENERATOR_MODES: &[(i64, &str)] = &[(1, "replace"), (2, "over")];
const GRID_SIZES: &[(i64, &str)] = &[(1, "corner"), (2, "width"), (3, "width-height")];
const GRID_DRAWS: &[(i64, &str)] = &[(0, "lines"), (1, "cells")];
const FRACTALS: &[(i64, &str)] = &[
    (1, "basic"),
    (3, "turbulent-smooth"),
    (4, "turbulent-basic"),
    (5, "turbulent-sharp"),
];
const NOISES: &[(i64, &str)] = &[
    (1, "block"),
    (2, "linear"),
    (3, "soft-linear"),
    (4, "spline"),
];
const INVERTED: &[(i64, &str)] = &[(0, "normal"), (1, "inverted")];
// A result beyond 0 to 1 has nowhere to go in 8 bits, so it clips.
const OVERFLOWS: &[(i64, &str)] = &[
    (1, "clip"),
    (2, "soft-clamp"),
    (3, "wrap-back"),
    (4, "clip"),
];
const DIMENSIONS: &[(i64, &str)] = &[(1, "both"), (2, "horizontal"), (3, "vertical")];
const EDGES: &[(i64, &str)] = &[(0, "transparent"), (1, "repeat")];
const SHADOW_ONLY: &[(i64, &str)] = &[(0, "shown"), (1, "hidden")];
const GLOW_COMPOSITE: &[(i64, &str)] =
    &[(1, "with-original"), (2, "with-original"), (3, "glow-only")];
/// Why a levels effect's channel menu changes nothing a frame shows.
const PANEL_CHANNEL: &str = "which channel's levels the effect's panel shows";
/// Why a levels effect's sliders change nothing a frame shows.
const PANEL_LEVELS: &str =
    "the panel's view of one channel's levels, which the effect keeps in its histogram data";
const CHANNELS: &[(i64, &str)] = &[
    (1, "rgb"),
    (2, "red"),
    (3, "green"),
    (4, "blue"),
    (5, "alpha"),
];
const SHAPES_OF_RANGE: &[(i64, &str)] = &[
    (1, "square"),
    (2, "ramp-up"),
    (3, "ramp-down"),
    (4, "triangle"),
    (5, "round"),
    (6, "smooth"),
];

use Convert::{Angles, Color, Level, Number, Path, Percent, Point, PointPercent, Times};
use Missing::{Absent, CameraPosition, CameraZoom, CompCentre, Parameter, SourceCentre};

/// A point the project keeps in fractions of the layer's source, as it
/// keeps the anchor point of a layer with a source: the walk turns it into
/// pixels of the source, or keeps it on a layer without one, whose anchor
/// is in pixels already.
const IN_SOURCE: Convert = Convert::Sized {
    size: [1.0, 1.0],
    origin: [0.0, 0.0],
};

/// The rows of the transform effect that do not depend on whether its scale
/// is uniform. Its points are fractions of the layer's size, which the walk
/// turns into pixels, so they are written as points.
macro_rules! transform_effect_rows {
    ($($scale:expr),+) => {
        [
            row(
                "ADBE Geometry2-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Geometry2-0001", To::Prop("anchor"), Point, Parameter),
            row("ADBE Geometry2-0002", To::Prop("position"), Point, Parameter),
            row(
                "ADBE Geometry2-0011",
                To::Ignore("whether the scale is uniform, which decides where it goes"),
                Number,
                Absent,
            ),
            $($scale,)+
            row("ADBE Geometry2-0005", To::OnlyDefault("skew"), Number, Absent),
            row(
                "ADBE Geometry2-0006",
                To::Ignore("the axis of a skew of 0"),
                Number,
                Absent,
            ),
            row("ADBE Geometry2-0007", To::Prop("rotate"), Number, Absent),
            row("ADBE Geometry2-0008", To::Prop("opacity"), Percent, Absent),
            row(
                "ADBE Geometry2-0009",
                To::Ignore("the shutter of motion blur, which is reported on its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Geometry2-0010",
                To::Ignore("the shutter of motion blur, which is reported on its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Geometry2-0012",
                To::Ignore("how pixels are sampled; the scene samples them bilinearly"),
                Number,
                Absent,
            ),
        ]
    };
}

macro_rules! fractal_noise_rows {
    ($($scale:expr),+) => {
        [
            row(
                "ADBE Fractal Noise-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0001",
                To::Setting("fractal", FRACTALS),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0002",
                To::Setting("noise", NOISES),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0003",
                To::Setting("polarity", INVERTED),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0004",
                To::Prop("contrast"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0005",
                To::Prop("brightness"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0006",
                To::Setting("overflow", OVERFLOWS),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0007",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0008",
                To::Prop("rotation"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0009",
                To::Ignore("whether the scale is uniform, which decides where it goes"),
                Number,
                Absent,
            ),
            $($scale,)+
            row(
                "ADBE Fractal Noise-0013",
                To::Prop("offset"),
                Point,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0031",
                To::OnlyDefault("a perspective offset"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0014",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0015",
                To::Prop("complexity"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0016",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0017",
                To::Prop("sub-influence"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0018",
                To::Prop("sub-scaling"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0019",
                To::Prop("sub-rotation"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0020",
                To::Prop("sub-offset"),
                Point,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0021",
                To::OnlyDefault("a sub-scale about the centre"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0022",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0023",
                To::Prop("evolution"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0024",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0025",
                To::OnlyDefault("a cycled evolution"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0026",
                To::Ignore("the cycle of a cycled evolution, which is reported on its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0027",
                To::Prop("seed"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0028",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fractal Noise-0029",
                To::Prop("opacity"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Fractal Noise-0030",
                To::Setting("mode", GENERATOR_MODES),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ]
    };
}

/// Fractal noise with a uniform scale.
static FRACTAL_NOISE_UNIFORM: [Row; 33] = fractal_noise_rows!(
    row(
        "ADBE Fractal Noise-0010",
        To::Prop("scale"),
        Percent,
        Parameter,
    ),
    row(
        "ADBE Fractal Noise-0011",
        To::Ignore("the width of a uniform scale"),
        Number,
        Absent,
    ),
    row(
        "ADBE Fractal Noise-0012",
        To::Ignore("the height of a uniform scale"),
        Number,
        Absent,
    )
);

/// Fractal noise with a width and height apart.
static FRACTAL_NOISE_APART: [Row; 33] = fractal_noise_rows!(
    row(
        "ADBE Fractal Noise-0010",
        To::Ignore("the scale of a scale apart on each axis"),
        Number,
        Absent,
    ),
    row(
        "ADBE Fractal Noise-0011",
        To::Prop("scale-width"),
        Percent,
        Parameter,
    ),
    row(
        "ADBE Fractal Noise-0012",
        To::Prop("scale-height"),
        Percent,
        Parameter,
    )
);

/// The transform effect with a uniform scale: its height scales both axes.
static TRANSFORM_EFFECT_UNIFORM: [Row; 13] = transform_effect_rows!(
    row("ADBE Geometry2-0003", To::Prop("scale"), Percent, Absent),
    row(
        "ADBE Geometry2-0004",
        To::Ignore("the width of a uniform scale, which its height sets"),
        Number,
        Absent
    )
);

/// The transform effect with a scale apart on each axis.
static TRANSFORM_EFFECT_APART: [Row; 13] = transform_effect_rows!(
    row("ADBE Geometry2-0003", To::Prop("scale-y"), Percent, Absent),
    row("ADBE Geometry2-0004", To::Prop("scale-x"), Percent, Absent)
);

/// Why the depth and turns out of the screen of a flat layer are ignored.
const FLAT: &str = "depth and turns out of the screen on a flat layer, which only a 3D layer takes";

/// A layer's transform, in `ADBE Transform Group`: `$anchor` and
/// `$position` are where the anchor point and the position go, then the
/// rows for depth, scale and opacity. Layer scale and opacity are
/// fractions in the project already.
macro_rules! layer_transform {
    (
        $anchor:expr,
        $position:expr,
        $depth:expr,
        $scale:expr,
        $orientation:expr,
        $turn_x:expr,
        $turn_y:expr,
        $opacity:expr $(,)?
    ) => {
        [
            $anchor,
            $position,
            row("ADBE Position_0", To::Prop("x"), Number, Absent),
            row("ADBE Position_1", To::Prop("y"), Number, Absent),
            $depth,
            $scale,
            $orientation,
            $turn_x,
            $turn_y,
            row(
                "ADBE Rotate Z",
                To::Prop("rotate"),
                Number,
                Missing::Number(0.0),
            ),
            $opacity,
            row(
                "ADBE Envir Appear in Reflect",
                To::Ignore("environment reflections of 3D layers"),
                Number,
                Absent,
            ),
        ]
    };
}

/// The opacity of a layer that draws.
const OPACITY: Row = row(
    "ADBE Opacity",
    To::Prop("opacity"),
    Number,
    Missing::Number(1.0),
);

/// The position of a 3D layer or a camera, with its depth.
const POSITION_3D: Row = row(
    "ADBE Position",
    To::Split("position", "z"),
    Point,
    CompCentre,
);

/// The depth of a separated position.
const DEPTH: Row = row("ADBE Position_2", To::Prop("z"), Number, Absent);

/// A 3D layer's orientation and its turns about X and Y.
const ORIENTATION: Row = row("ADBE Orientation", To::Prop("orient"), Angles, Absent);
const TURN_X: Row = row(
    "ADBE Rotate X",
    To::Prop("rotate-x"),
    Number,
    Missing::Number(0.0),
);
const TURN_Y: Row = row(
    "ADBE Rotate Y",
    To::Prop("rotate-y"),
    Number,
    Missing::Number(0.0),
);

/// A flat layer's transform: what holds depth or turns out of the screen
/// is ignored, as the original ignores it on a flat layer.
pub(super) static TRANSFORM: [Row; 12] = layer_transform!(
    row(
        "ADBE Anchor Point",
        To::Prop("anchor"),
        IN_SOURCE,
        SourceCentre
    ),
    row("ADBE Position", To::Prop("position"), Point, CompCentre),
    row("ADBE Position_2", To::Ignore(FLAT), Number, Absent),
    row(
        "ADBE Scale",
        To::Prop("scale"),
        Point,
        Missing::Vector([1.0, 1.0]),
    ),
    row("ADBE Orientation", To::Ignore(FLAT), Point, Absent),
    row("ADBE Rotate X", To::Ignore(FLAT), Number, Absent),
    row("ADBE Rotate Y", To::Ignore(FLAT), Number, Absent),
    OPACITY,
);

/// A 3D layer's transform ([D58]): its anchor point, position and scale
/// keep their depth, and it turns about X and Y and by its orientation.
///
/// [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
pub(super) static TRANSFORM_3D: [Row; 12] = layer_transform!(
    row(
        "ADBE Anchor Point",
        To::Split("anchor", "anchor-z"),
        IN_SOURCE,
        SourceCentre,
    ),
    POSITION_3D,
    DEPTH,
    row(
        "ADBE Scale",
        To::Split("scale", "scale-z"),
        Point,
        Missing::Vector([1.0, 1.0]),
    ),
    ORIENTATION,
    TURN_X,
    TURN_Y,
    OPACITY,
);

/// A camera's or a light's transform: `$anchor` is where its point of
/// interest goes; a camera's scale and opacity change nothing, and a
/// light's position, which the project leaves out, defaults as a layer's.
macro_rules! viewer_transform {
    ($anchor:expr, $position:expr, $why:expr $(,)?) => {
        layer_transform!(
            $anchor,
            $position,
            DEPTH,
            row("ADBE Scale", To::Ignore($why), Point, Absent),
            ORIENTATION,
            TURN_X,
            TURN_Y,
            row("ADBE Opacity", To::Ignore($why), Number, Absent),
        )
    };
}

/// Why a camera's scale and opacity are ignored.
const CAMERA_DRAWS_NOTHING: &str = "the scale and opacity of a camera, which draws nothing";

/// The position of a camera, whose default sees the plane at depth 0 at 100 %.
const CAMERA_POSITION: Row = row(
    "ADBE Position",
    To::Split("position", "z"),
    Point,
    CameraPosition,
);

/// A two-node camera's transform: it turns towards its point of interest,
/// its anchor point, which becomes its `target`. Every camera of the
/// samples stores its point of interest, so its default is unmeasured; it
/// is taken as an anchor point's.
pub(super) static CAMERA_AIMED: [Row; 12] = viewer_transform!(
    row(
        "ADBE Anchor Point",
        To::Split("target", "target-z"),
        Point,
        SourceCentre,
    ),
    CAMERA_POSITION,
    CAMERA_DRAWS_NOTHING,
);

/// A one-node camera's transform: it looks along its own depth axis, and
/// its point of interest changes nothing.
pub(super) static CAMERA_FREE: [Row; 12] = viewer_transform!(
    row(
        "ADBE Anchor Point",
        To::Ignore("the point of interest of a camera that does not turn towards it"),
        Point,
        Absent,
    ),
    CAMERA_POSITION,
    CAMERA_DRAWS_NOTHING,
);

/// A light's transform, for the layers it moves: the light itself draws
/// nothing (reported), and neither does its point of interest.
pub(super) static LIGHT_TRANSFORM: [Row; 12] = viewer_transform!(
    row(
        "ADBE Anchor Point",
        To::Ignore("the point of interest of a light, which is drawn as nothing"),
        Point,
        Absent,
    ),
    POSITION_3D,
    "the scale and opacity of a light, which is drawn as nothing",
);

/// Why the settings of depth of field are ignored.
const DEPTH_OF_FIELD: &str = "a setting of depth of field, which is reported where it is on";

/// A camera's options, in `ADBE Camera Options Group`: its zoom, and depth
/// of field, which the scene does not draw.
pub(super) static CAMERA_OPTIONS: [Row; 12] = [
    row("ADBE Camera Zoom", To::Prop("zoom"), Number, CameraZoom),
    row(
        "ADBE Camera Depth of Field",
        To::OnlyDefault("depth of field (drawn sharp)"),
        Number,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Camera Focus Distance",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Camera Aperture",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Camera Blur Level",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Iris Shape",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Iris Rotation",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Iris Roundness",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Iris Aspect Ratio",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Iris Diffraction Fringe",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Iris Highlight Gain",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
    row(
        "ADBE Iris Highlight Threshold",
        To::Ignore(DEPTH_OF_FIELD),
        Number,
        Absent,
    ),
];

/// A shape group's transform rows, in `ADBE Vector Transform Group`; its
/// opacity and scale are percents.
const VECTOR_TRANSFORM: [Row; 7] = [
    row("ADBE Vector Anchor", To::Prop("anchor"), Point, Absent),
    row("ADBE Vector Position", To::Prop("position"), Point, Absent),
    row(
        "ADBE Vector Scale",
        To::Prop("scale"),
        PointPercent,
        Missing::Vector([100.0, 100.0]),
    ),
    row(
        "ADBE Vector Rotation",
        To::Prop("rotate"),
        Number,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Vector Group Opacity",
        To::Prop("opacity"),
        Percent,
        Missing::Number(100.0),
    ),
    row(
        "ADBE Vector Skew",
        To::OnlyDefault("skew"),
        Number,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Vector Skew Axis",
        To::Ignore("the axis of a skew of 0"),
        Number,
        Absent,
    ),
];

/// The groups that become scene items, by match name.
pub(super) static KINDS: [Kind; 46] = [
    // A mask: its path in the mask space, which the import scales to the
    // layer's px; its mode and side come from its `mkif`.
    Kind {
        match_name: "ADBE Mask Atom",
        item: "mask",
        table: &MASKS,
        rows: &[
            row("ADBE Mask Shape", To::Prop("path"), MASK_PATH, Absent),
            row(
                "ADBE Mask Feather",
                To::Prop("feather"),
                Point,
                Missing::Vector([0.0, 0.0]),
            ),
            row(
                "ADBE Mask Opacity",
                To::Prop("opacity"),
                Percent,
                Missing::Number(100.0),
            ),
            row(
                "ADBE Mask Offset",
                To::Prop("expand"),
                Number,
                Missing::Number(0.0),
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Group",
        item: "group",
        table: &SHAPES,
        rows: &VECTOR_TRANSFORM,
        parts: &[
            ("ADBE Vectors Group", Part::Items),
            ("ADBE Vector Transform Group", Part::Rows),
        ],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Shape - Rect",
        item: "rect",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Rect Size",
                To::Prop("size"),
                Point,
                Missing::Vector([100.0, 100.0]),
            ),
            row(
                "ADBE Vector Rect Position",
                To::Prop("position"),
                Point,
                Absent,
            ),
            row(
                "ADBE Vector Rect Roundness",
                To::Prop("round"),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Shape Direction",
                To::OnlyDefault("reversed path direction"),
                Number,
                Missing::Number(1.0),
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Shape - Ellipse",
        item: "ellipse",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Ellipse Size",
                To::Prop("size"),
                Point,
                Missing::Vector([100.0, 100.0]),
            ),
            row(
                "ADBE Vector Ellipse Position",
                To::Prop("position"),
                Point,
                Absent,
            ),
            row(
                "ADBE Vector Shape Direction",
                To::OnlyDefault("reversed path direction"),
                Number,
                Missing::Number(1.0),
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Shape - Star",
        item: "star",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Star Type",
                To::Setting("type", STAR_TYPES),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Star Points",
                To::Prop("points"),
                Number,
                Missing::Number(5.0),
            ),
            row(
                "ADBE Vector Star Position",
                To::Prop("position"),
                Point,
                Absent,
            ),
            row(
                "ADBE Vector Star Rotation",
                To::Prop("rotate"),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Star Inner Radius",
                To::Prop("inner-radius"),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Star Outer Radius",
                To::Prop("outer-radius"),
                Number,
                Absent,
            ),
            // The project's match names spell roundness without its second n.
            row(
                "ADBE Vector Star Inner Roundess",
                To::Prop("inner-roundness"),
                Percent,
                Absent,
            ),
            row(
                "ADBE Vector Star Outer Roundess",
                To::Prop("outer-roundness"),
                Percent,
                Absent,
            ),
            row(
                "ADBE Vector Shape Direction",
                To::Setting("direction", SHAPE_DIRECTIONS),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Shape - Group",
        item: "path",
        table: &SHAPES,
        rows: &[
            row("ADBE Vector Shape", To::Prop("path"), Path, Absent),
            row(
                "ADBE Vector Shape Direction",
                To::OnlyDefault("reversed path direction"),
                Number,
                Missing::Number(1.0),
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Graphic - Fill",
        item: "fill",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Fill Color",
                To::Prop("color"),
                Color,
                Missing::Color([1.0, 0.0, 0.0, 1.0]),
            ),
            row(
                "ADBE Vector Fill Opacity",
                To::Prop("opacity"),
                Percent,
                Missing::Number(100.0),
            ),
            row(
                "ADBE Vector Fill Rule",
                To::Setting("rule", FILL_RULES),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Blend Mode",
                To::OnlyDefault("blend mode"),
                Number,
                Missing::Number(1.0),
            ),
            row(
                "ADBE Vector Composite Order",
                To::OnlyDefault("drawing over earlier paints"),
                Number,
                Missing::Number(1.0),
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Graphic - Stroke",
        item: "stroke",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Stroke Color",
                To::Prop("color"),
                Color,
                Missing::Color([1.0, 1.0, 1.0, 1.0]),
            ),
            row(
                "ADBE Vector Stroke Opacity",
                To::Prop("opacity"),
                Percent,
                Missing::Number(100.0),
            ),
            row(
                "ADBE Vector Stroke Width",
                To::Prop("width"),
                Number,
                Missing::Number(2.0),
            ),
            row(
                "ADBE Vector Stroke Line Cap",
                To::Setting("cap", LINE_CAPS),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Stroke Line Join",
                To::Setting("join", LINE_JOINS),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Stroke Miter Limit",
                To::Prop("miter"),
                Number,
                Missing::Number(4.0),
            ),
            row(
                "ADBE Vector Blend Mode",
                To::OnlyDefault("blend mode"),
                Number,
                Missing::Number(1.0),
            ),
            row(
                "ADBE Vector Composite Order",
                To::OnlyDefault("drawing over earlier paints"),
                Number,
                Missing::Number(1.0),
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Filter - Trim",
        item: "trim",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Trim Start",
                To::Prop("start"),
                Percent,
                Missing::Number(0.0),
            ),
            row(
                "ADBE Vector Trim End",
                To::Prop("end"),
                Percent,
                Missing::Number(100.0),
            ),
            row(
                "ADBE Vector Trim Offset",
                To::Prop("offset"),
                Number,
                Missing::Number(0.0),
            ),
            row(
                "ADBE Vector Trim Type",
                To::Setting("mode", TRIM_MODES),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Filter - Merge",
        item: "merge",
        table: &SHAPES,
        rows: &[row(
            "ADBE Vector Merge Type",
            To::Setting("mode", MERGE_MODES),
            Number,
            Absent,
        )],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Filter - RC",
        item: "round-corners",
        table: &SHAPES,
        rows: &[row(
            "ADBE Vector RoundCorner Radius",
            To::Prop("radius"),
            Number,
            Missing::Number(10.0),
        )],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Filter - Repeater",
        item: "repeater",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Repeater Copies",
                To::Prop("copies"),
                Number,
                Missing::Number(3.0),
            ),
            row(
                "ADBE Vector Repeater Offset",
                To::Prop("offset"),
                Number,
                Missing::Number(0.0),
            ),
            row(
                "ADBE Vector Repeater Order",
                To::Setting("composite", COMPOSITES),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Repeater Anchor",
                To::Prop("anchor"),
                Point,
                Missing::Vector([0.0, 0.0]),
            ),
            // The editor gives a new repeater a position of [100, 0] and
            // stores it; a project that leaves the position out holds the
            // origin (evidence R3).
            row(
                "ADBE Vector Repeater Position",
                To::Prop("position"),
                Point,
                Missing::Vector([0.0, 0.0]),
            ),
            row(
                "ADBE Vector Repeater Scale",
                To::Prop("scale"),
                PointPercent,
                Missing::Vector([100.0, 100.0]),
            ),
            row(
                "ADBE Vector Repeater Rotation",
                To::Prop("rotate"),
                Number,
                Missing::Number(0.0),
            ),
            row(
                "ADBE Vector Repeater Opacity 1",
                To::Prop("start-opacity"),
                Percent,
                Missing::Number(100.0),
            ),
            row(
                "ADBE Vector Repeater Opacity 2",
                To::Prop("end-opacity"),
                Percent,
                Missing::Number(100.0),
            ),
        ],
        parts: &[("ADBE Vector Repeater Transform", Part::Rows)],
        switch: None,
    },
    Kind {
        match_name: "ADBE Vector Filter - Offset",
        item: "expand",
        table: &SHAPES,
        rows: &[
            row(
                "ADBE Vector Offset Amount",
                To::Prop("amount"),
                Number,
                Missing::Number(10.0),
            ),
            row(
                "ADBE Vector Offset Line Join",
                To::Setting("join", LINE_JOINS),
                Number,
                Absent,
            ),
            row(
                "ADBE Vector Offset Miter Limit",
                To::Prop("miter"),
                Number,
                Missing::Number(4.0),
            ),
            row(
                "ADBE Vector Offset Copies",
                To::Prop("copies"),
                Number,
                Missing::Number(1.0),
            ),
            row(
                "ADBE Vector Offset Copy Offset",
                To::Prop("copy-offset"),
                Number,
                Missing::Number(0.0),
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Fill",
        item: "fill",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Fill-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fill-0001",
                To::OnlyDefault("filling only inside a mask"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fill-0007",
                To::OnlyDefault("filling inside all masks"),
                Number,
                Parameter,
            ),
            row("ADBE Fill-0002", To::Prop("color"), Color, Parameter),
            row(
                "ADBE Fill-0006",
                To::Setting("inverted", OFF_ON),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fill-0003",
                To::OnlyDefault("horizontal feather"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fill-0004",
                To::OnlyDefault("vertical feather"),
                Number,
                Parameter,
            ),
            row("ADBE Fill-0005", To::Prop("opacity"), Number, Parameter),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Text Animator",
        item: "animator",
        table: &ANIMATORS,
        rows: &[
            row("ADBE Text Position 3D", To::Prop("position"), Point, Absent),
            row(
                "ADBE Text Anchor Point 3D",
                To::Prop("anchor"),
                Point,
                Absent,
            ),
            row(
                "ADBE Text Scale 3D",
                To::Prop("scale"),
                PointPercent,
                Absent,
            ),
            row("ADBE Text Rotation", To::Prop("rotate"), Number, Absent),
            row("ADBE Text Opacity", To::Prop("opacity"), Percent, Absent),
            row("ADBE Text Fill Color", To::Prop("fill"), Color, Absent),
            row("ADBE Text Stroke Color", To::Prop("stroke"), Color, Absent),
            row(
                "ADBE Text Stroke Width",
                To::Prop("stroke-width"),
                Number,
                Absent,
            ),
            row(
                "ADBE Text Tracking Amount",
                To::Prop("tracking"),
                Number,
                Absent,
            ),
            row(
                "ADBE Text Track Type",
                To::OnlyDefaultWith("tracking before and after", "ADBE Text Tracking Amount"),
                Number,
                Missing::Number(1.0),
            ),
        ],
        parts: &[
            ("ADBE Text Selectors", Part::Items),
            ("ADBE Text Animator Properties", Part::Rows),
        ],
        switch: None,
    },
    Kind {
        match_name: "ADBE Text Selector",
        item: "range",
        table: &SELECTORS,
        rows: &RANGE_IN_PERCENT,
        parts: &[("ADBE Text Range Advanced", Part::Rows)],
        switch: Some(Switch {
            match_name: "ADBE Text Range Units",
            missing: 1.0,
            on: in_percent,
            off_rows: &RANGE_IN_INDEX,
        }),
    },
    Kind {
        match_name: "ADBE Text Expressible Selector",
        item: "parts",
        table: &SELECTORS,
        rows: &[
            row(
                "ADBE Text Expressible Amount",
                To::Ignore("written as the selector's parts, one per part of the text"),
                Number,
                Absent,
            ),
            row(
                "ADBE Text Range Type2",
                To::Setting("by", BASED_ON),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Geometry2",
        item: "transform",
        table: &EFFECTS,
        rows: &TRANSFORM_EFFECT_UNIFORM,
        parts: &[],
        switch: Some(Switch {
            match_name: "ADBE Geometry2-0011",
            missing: 1.0,
            on: checked,
            off_rows: &TRANSFORM_EFFECT_APART,
        }),
    },
    Kind {
        match_name: "ADBE Tint",
        item: "tint",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Tint-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Tint-0001", To::Prop("black"), Color, Parameter),
            row("ADBE Tint-0002", To::Prop("white"), Color, Parameter),
            row("ADBE Tint-0003", To::Prop("amount"), Percent, Parameter),
            row(
                "ADBE Tint-0004",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE HUE SATURATION",
        item: "hue-saturation",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE HUE SATURATION-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE HUE SATURATION-0002",
                To::OnlyDefault("hue and saturation of one colour range"),
                Number,
                Parameter,
            ),
            row(
                "ADBE HUE SATURATION-0003",
                To::Ignore("the colour ranges, which the master channel does not use"),
                Number,
                Absent,
            ),
            row("ADBE HUE SATURATION-0004", To::Prop("hue"), Number, Absent),
            row(
                "ADBE HUE SATURATION-0005",
                To::Prop("saturation"),
                Percent,
                Absent,
            ),
            row(
                "ADBE HUE SATURATION-0006",
                To::Prop("lightness"),
                Percent,
                Absent,
            ),
            row(
                "ADBE HUE SATURATION-0007",
                To::Setting("mode", COLORIZE),
                Number,
                Absent,
            ),
            row(
                "ADBE HUE SATURATION-0008",
                To::Prop("colorize-hue"),
                Number,
                Absent,
            ),
            row(
                "ADBE HUE SATURATION-0009",
                To::Prop("colorize-saturation"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE HUE SATURATION-0010",
                To::Prop("colorize-lightness"),
                Percent,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Exposure2",
        item: "exposure",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Exposure2-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0001",
                To::OnlyDefault("exposure per channel"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Exposure2-0002",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0006",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0007",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0011",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0012",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0016",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0017",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0021",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row("ADBE Exposure2-0003", To::Prop("exposure"), Number, Absent),
            row("ADBE Exposure2-0004", To::Prop("offset"), Number, Absent),
            row("ADBE Exposure2-0005", To::Prop("gamma"), Number, Absent),
            row(
                "ADBE Exposure2-0008",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0009",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0010",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0013",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0014",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0015",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0018",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0019",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0020",
                To::Ignore("a channel's own exposure, which the master channel does not use"),
                Number,
                Absent,
            ),
            row(
                "ADBE Exposure2-0022",
                To::Setting("space", EXPOSURE_SPACES),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Easy Levels2",
        item: "levels",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Easy Levels2-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0001",
                To::Ignore(PANEL_CHANNEL),
                Number,
                Absent,
            ),
            row("ADBE Easy Levels2-0002", To::Levels, Number, Absent),
            row(
                "ADBE Easy Levels2-0003",
                To::Ignore(PANEL_LEVELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0004",
                To::Ignore(PANEL_LEVELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0005",
                To::Ignore(PANEL_LEVELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0006",
                To::Ignore(PANEL_LEVELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0007",
                To::Ignore(PANEL_LEVELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0008",
                To::Ignore("clipping, which an 8-bit project does and the scene always does"),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0009",
                To::Ignore("clipping, which an 8-bit project does and the scene always does"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Invert",
        item: "invert",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Invert-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Invert-0001",
                To::Setting("channel", CHANNELS),
                Number,
                Absent,
            ),
            row("ADBE Invert-0002", To::Prop("blend"), Percent, Absent),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "CC Particle Systems II",
        item: "particles",
        table: &EFFECTS,
        rows: &[
            row(
                "CC Particle Systems II-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0001",
                To::Prop("rate"),
                Times(CC_RATE),
                Parameter,
            ),
            row(
                "CC Particle Systems II-0002",
                To::Prop("longevity"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0003",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0004",
                To::Prop("position"),
                Point,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0005",
                To::Prop("radius-x"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0006",
                To::Prop("radius-y"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0007",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0008",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0009",
                To::Setting("animation", CC_ANIMATIONS),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0010",
                To::Prop("velocity"),
                Times(CC_SPEED),
                Parameter,
            ),
            row(
                "CC Particle Systems II-0011",
                To::OnlyDefault("particles inheriting the producer's velocity"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0012",
                To::Prop("gravity"),
                Times(CC_SPEED),
                Parameter,
            ),
            row(
                "CC Particle Systems II-0013",
                To::Prop("resistance"),
                Times(CC_RESISTANCE),
                Parameter,
            ),
            row(
                "CC Particle Systems II-0014",
                To::Prop("direction"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0015",
                To::Prop("extra"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0016",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0017",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0018",
                To::Setting("shape", CC_SHAPES),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0019",
                To::Prop("birth-size"),
                Times(CC_SIZE),
                Parameter,
            ),
            row(
                "CC Particle Systems II-0020",
                To::Prop("death-size"),
                Times(CC_SIZE),
                Parameter,
            ),
            row(
                "CC Particle Systems II-0021",
                To::Prop("size-variation"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0022",
                To::Setting("opacity-map", CC_OPACITY_MAPS),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0023",
                To::Prop("max-opacity"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0029",
                To::OnlyDefault("volume shading of particles"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0024",
                To::OnlyDefault("a colour map other than birth to death"),
                Number,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0025",
                To::Prop("birth-color"),
                Color,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0026",
                To::Prop("death-color"),
                Color,
                Parameter,
            ),
            row(
                "CC Particle Systems II-0028",
                To::Setting("transfer", CC_TRANSFERS),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0027",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "CC Particle Systems II-0030",
                To::Prop("seed"),
                Number,
                Parameter,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Displacement Map",
        item: "displacement-map",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Displacement Map-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Displacement Map-0001", To::Input, Number, Absent),
            row(
                "ADBE Displacement Map-0002",
                To::Setting("horizontal-from", MAP_LEVELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Displacement Map-0003",
                To::Prop("horizontal"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Displacement Map-0004",
                To::Setting("vertical-from", MAP_LEVELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Displacement Map-0005",
                To::Prop("vertical"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Displacement Map-0006",
                To::Setting("behaviour", MAP_BEHAVIOURS),
                Number,
                Absent,
            ),
            row(
                "ADBE Displacement Map-0007",
                To::Setting("edges", MAP_EDGES),
                Number,
                Absent,
            ),
            row(
                "ADBE Displacement Map-0008",
                To::Ignore(
                    "whether the layer grows to hold what moves past it; the frame holds it",
                ),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE AudSpect",
        item: "audio-spectrum",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE AudSpect-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE AudSpect-0001", To::Input, Number, Absent),
            row("ADBE AudSpect-0002", To::Prop("start"), Point, Parameter),
            row("ADBE AudSpect-0003", To::Prop("end"), Point, Parameter),
            row("ADBE AudSpect-0004", To::Outline, Number, Absent),
            row(
                "ADBE AudSpect-0005",
                To::Setting("path", AUDIO_PATHS),
                Number,
                Absent,
            ),
            row("ADBE AudSpect-0006", To::Prop("from"), Number, Parameter),
            row("ADBE AudSpect-0007", To::Prop("to"), Number, Parameter),
            row("ADBE AudSpect-0008", To::Prop("bands"), Number, Parameter),
            row("ADBE AudSpect-0009", To::Prop("height"), Number, Parameter),
            row(
                "ADBE AudSpect-0010",
                To::Prop("duration"),
                Number,
                Parameter,
            ),
            row("ADBE AudSpect-0011", To::Prop("offset"), Number, Parameter),
            row(
                "ADBE AudSpect-0012",
                To::Prop("thickness"),
                Number,
                Parameter,
            ),
            row(
                "ADBE AudSpect-0013",
                To::Prop("softness"),
                Number,
                Parameter,
            ),
            row(
                "ADBE AudSpect-0014",
                To::Prop("inside-color"),
                Color,
                Parameter,
            ),
            row(
                "ADBE AudSpect-0015",
                To::Prop("outside-color"),
                Color,
                Parameter,
            ),
            row(
                "ADBE AudSpect-0016",
                To::OnlyDefault("blending where bands overlap"),
                Number,
                Missing::Number(0.0),
            ),
            row("ADBE AudSpect-0017", To::Prop("hue"), Number, Parameter),
            row(
                "ADBE AudSpect-0018",
                To::OnlyDefault("a hue that starts at the loudest band"),
                Number,
                Missing::Number(0.0),
            ),
            row(
                "ADBE AudSpect-0019",
                To::Setting("symmetry", OFF_ON),
                Number,
                Absent,
            ),
            row(
                "ADBE AudSpect-0020",
                To::Setting("display", AUDIO_DISPLAYS),
                Number,
                Absent,
            ),
            row(
                "ADBE AudSpect-0021",
                To::Setting("side", AUDIO_SIDES),
                Number,
                Absent,
            ),
            row(
                "ADBE AudSpect-0022",
                To::Setting("average", OFF_ON),
                Number,
                Absent,
            ),
            row(
                "ADBE AudSpect-0023",
                To::Setting("composite", OFF_ON),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Tile",
        item: "motion-tile",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Tile-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Tile-0001", To::Prop("center"), Point, Parameter),
            row("ADBE Tile-0002", To::Prop("tile-width"), Percent, Parameter),
            row(
                "ADBE Tile-0003",
                To::Prop("tile-height"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Tile-0004",
                To::Prop("output-width"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Tile-0005",
                To::Prop("output-height"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Tile-0006",
                To::Setting("edges", TILE_EDGES),
                Number,
                Absent,
            ),
            row("ADBE Tile-0007", To::Prop("phase"), Number, Parameter),
            row(
                "ADBE Tile-0008",
                To::Setting("shift", TILE_SHIFTS),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Offset",
        item: "wrap",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Offset-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Offset-0001", To::Prop("to"), Point, Parameter),
            row("ADBE Offset-0002", To::Prop("blend"), Number, Parameter),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Turbulent Displace",
        item: "turbulent-displace",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Turbulent Displace-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Turbulent Displace-0001",
                To::Setting("displacement", DISPLACEMENTS),
                Number,
                Absent,
            ),
            row(
                "ADBE Turbulent Displace-0002",
                To::Prop("amount"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0003",
                To::Prop("size"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0004",
                To::Prop("offset"),
                Point,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0005",
                To::Prop("complexity"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0006",
                To::Prop("evolution"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0007",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Turbulent Displace-0008",
                To::OnlyDefault("a cycled evolution"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0009",
                To::Ignore("the cycle of a cycled evolution, which is reported on its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Turbulent Displace-0010",
                To::Prop("seed"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0011",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Turbulent Displace-0012",
                To::Setting("pinning", PINNINGS),
                Number,
                Absent,
            ),
            row(
                "ADBE Turbulent Displace-0013",
                To::OnlyDefault("a layer resized to fit what it displaces"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Turbulent Displace-0014",
                To::Ignore("antialiasing; the scene reads between pixels"),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Mirror",
        item: "mirror",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Mirror-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Mirror-0001", To::Prop("center"), Point, Parameter),
            row("ADBE Mirror-0002", To::Prop("angle"), Number, Parameter),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Polar Coordinates",
        item: "polar-coordinates",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Polar Coordinates-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Polar Coordinates-0001",
                To::Prop("interpolation"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Polar Coordinates-0002",
                To::Setting("conversion", POLAR_CONVERSIONS),
                Number,
                Parameter,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "CC RepeTile",
        item: "repeat-tile",
        table: &EFFECTS,
        rows: &[
            row(
                "CC RepeTile-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("CC RepeTile-0001", To::Prop("right"), Number, Parameter),
            row("CC RepeTile-0002", To::Prop("left"), Number, Parameter),
            row("CC RepeTile-0003", To::Prop("down"), Number, Parameter),
            row("CC RepeTile-0004", To::Prop("up"), Number, Parameter),
            row(
                "CC RepeTile-0005",
                To::Setting("tiling", TILINGS),
                Number,
                Absent,
            ),
            row(
                "CC RepeTile-0006",
                To::OnlyDefault("tiles blended across their borders"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Ramp",
        item: "ramp",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Ramp-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Ramp-0001", To::Prop("start"), Point, Parameter),
            row("ADBE Ramp-0002", To::Prop("start-color"), Color, Parameter),
            row("ADBE Ramp-0003", To::Prop("end"), Point, Parameter),
            row("ADBE Ramp-0004", To::Prop("end-color"), Color, Parameter),
            row(
                "ADBE Ramp-0005",
                To::Setting("shape", RAMP_SHAPES),
                Number,
                Absent,
            ),
            row(
                "ADBE Ramp-0006",
                To::OnlyDefault("ramp scatter, a dither of the gradient"),
                Number,
                Parameter,
            ),
            row("ADBE Ramp-0007", To::Prop("blend"), Number, Parameter),
            row(
                "ADBE Ramp-0008",
                To::Ignore("a button that swaps the colours"),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE 4ColorGradient",
        item: "four-color",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE 4ColorGradient-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE 4ColorGradient-0011",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE 4ColorGradient-0001",
                To::Prop("point-1"),
                Point,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0002",
                To::Prop("color-1"),
                Color,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0003",
                To::Prop("point-2"),
                Point,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0004",
                To::Prop("color-2"),
                Color,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0005",
                To::Prop("point-3"),
                Point,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0006",
                To::Prop("color-3"),
                Color,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0007",
                To::Prop("point-4"),
                Point,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0008",
                To::Prop("color-4"),
                Color,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0012",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE 4ColorGradient-0009",
                To::Prop("blend"),
                Number,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0010",
                To::OnlyDefault("jitter, a dither of the gradient"),
                Number,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0013",
                To::Prop("opacity"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE 4ColorGradient-0014",
                To::Setting("mode", GENERATOR_MODES),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Laser",
        item: "beam",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Laser-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Laser-0001", To::Prop("start"), Point, Parameter),
            row("ADBE Laser-0002", To::Prop("end"), Point, Parameter),
            row("ADBE Laser-0003", To::Prop("length"), Number, Parameter),
            row("ADBE Laser-0004", To::Prop("time"), Number, Parameter),
            row(
                "ADBE Laser-0005",
                To::Prop("start-thickness"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Laser-0006",
                To::Prop("end-thickness"),
                Number,
                Parameter,
            ),
            row("ADBE Laser-0007", To::Prop("softness"), Number, Parameter),
            row(
                "ADBE Laser-0008",
                To::Prop("inside-color"),
                Color,
                Parameter,
            ),
            row(
                "ADBE Laser-0009",
                To::Prop("outside-color"),
                Color,
                Parameter,
            ),
            row(
                "ADBE Laser-0010",
                To::OnlyDefault("a beam drawn flat, without 3D perspective"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Laser-0011",
                To::Setting("composite", OFF_ON),
                Number,
                Parameter,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Grid",
        item: "grid",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Grid-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Grid-0001", To::Prop("anchor"), Point, Parameter),
            row(
                "ADBE Grid-0002",
                To::Setting("size", GRID_SIZES),
                Number,
                Absent,
            ),
            row("ADBE Grid-0003", To::Prop("corner"), Point, Parameter),
            row("ADBE Grid-0004", To::Prop("width"), Number, Parameter),
            row("ADBE Grid-0005", To::Prop("height"), Number, Parameter),
            row("ADBE Grid-0006", To::Prop("border"), Number, Parameter),
            row(
                "ADBE Grid-0007",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Grid-0008",
                To::OnlyDefault("a feathered grid"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Grid-0009",
                To::OnlyDefault("a feathered grid"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Grid-0010",
                To::Ignore("a group of parameters"),
                Number,
                Absent,
            ),
            row(
                "ADBE Grid-0011",
                To::Setting("draw", GRID_DRAWS),
                Number,
                Absent,
            ),
            row("ADBE Grid-0012", To::Prop("color"), Color, Parameter),
            row("ADBE Grid-0013", To::Prop("opacity"), Percent, Parameter),
            row(
                "ADBE Grid-0014",
                To::Setting("mode", GENERATOR_MODES),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Venetian Blinds",
        item: "venetian-blinds",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Venetian Blinds-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Venetian Blinds-0001",
                To::Prop("completion"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Venetian Blinds-0002",
                To::Prop("direction"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Venetian Blinds-0003",
                To::Prop("width"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Venetian Blinds-0004",
                To::Prop("feather"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Fractal Noise",
        item: "fractal-noise",
        table: &EFFECTS,
        rows: &FRACTAL_NOISE_UNIFORM,
        parts: &[],
        switch: Some(Switch {
            match_name: "ADBE Fractal Noise-0009",
            missing: 1.0,
            on: checked,
            off_rows: &FRACTAL_NOISE_APART,
        }),
    },
    Kind {
        match_name: "ADBE Gaussian Blur 2",
        item: "gaussian-blur",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Gaussian Blur 2-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Gaussian Blur 2-0001",
                To::Prop("blurriness"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Gaussian Blur 2-0002",
                To::Setting("dimensions", DIMENSIONS),
                Number,
                Absent,
            ),
            row(
                "ADBE Gaussian Blur 2-0003",
                To::Setting("edges", EDGES),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Fast Blur",
        item: "gaussian-blur",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Fast Blur-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Fast Blur-0001",
                To::Prop("blurriness"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Fast Blur-0002",
                To::Setting("dimensions", DIMENSIONS),
                Number,
                Absent,
            ),
            row(
                "ADBE Fast Blur-0003",
                To::Setting("edges", EDGES),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Box Blur2",
        item: "box-blur",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Box Blur2-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Box Blur2-0001", To::Prop("radius"), Number, Parameter),
            row(
                "ADBE Box Blur2-0002",
                To::Prop("iterations"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Box Blur2-0003",
                To::Setting("dimensions", DIMENSIONS),
                Number,
                Absent,
            ),
            row(
                "ADBE Box Blur2-0004",
                To::Setting("edges", EDGES),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Drop Shadow",
        item: "drop-shadow",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Drop Shadow-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Drop Shadow-0001", To::Prop("color"), Color, Parameter),
            row(
                "ADBE Drop Shadow-0002",
                To::Prop("opacity"),
                Level,
                Parameter,
            ),
            row(
                "ADBE Drop Shadow-0003",
                To::Prop("direction"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Drop Shadow-0004",
                To::Prop("distance"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Drop Shadow-0005",
                To::Prop("softness"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Drop Shadow-0006",
                To::Setting("layer", SHADOW_ONLY),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Glo2",
        item: "glow",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Glo2-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0001",
                To::OnlyDefault("a glow based on alpha"),
                Number,
                Parameter,
            ),
            row("ADBE Glo2-0002", To::Prop("threshold"), Level, Parameter),
            row("ADBE Glo2-0003", To::Prop("radius"), Number, Parameter),
            row("ADBE Glo2-0004", To::Prop("intensity"), Number, Parameter),
            row(
                "ADBE Glo2-0005",
                To::Setting("composite", GLOW_COMPOSITE),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0006",
                To::OnlyDefault("a glow operation other than add"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Glo2-0007",
                To::OnlyDefault("glow colours other than the original's"),
                Number,
                Parameter,
            ),
            row(
                "ADBE Glo2-0008",
                To::Ignore(
                    "the A and B colours, which only glow colours other than the original's use",
                ),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0009",
                To::Ignore(
                    "the A and B colours, which only glow colours other than the original's use",
                ),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0010",
                To::Ignore(
                    "the A and B colours, which only glow colours other than the original's use",
                ),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0011",
                To::Ignore(
                    "the A and B colours, which only glow colours other than the original's use",
                ),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0012",
                To::Ignore(
                    "the A and B colours, which only glow colours other than the original's use",
                ),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0013",
                To::Ignore(
                    "the A and B colours, which only glow colours other than the original's use",
                ),
                Number,
                Absent,
            ),
            row(
                "ADBE Glo2-0014",
                To::Setting("dimensions", DIMENSIONS),
                Number,
                Absent,
            ),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Sharpen",
        item: "sharpen",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Sharpen-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row("ADBE Sharpen-0001", To::Prop("amount"), Number, Parameter),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE Black&White",
        item: "black-white",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE Black&White-0000",
                To::Ignore("the layer the effect reads, its own"),
                Number,
                Absent,
            ),
            row(
                "ADBE Black&White-0001",
                To::Prop("reds"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Black&White-0002",
                To::Prop("yellows"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Black&White-0003",
                To::Prop("greens"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Black&White-0004",
                To::Prop("cyans"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Black&White-0005",
                To::Prop("blues"),
                Percent,
                Parameter,
            ),
            row(
                "ADBE Black&White-0006",
                To::Prop("magentas"),
                Percent,
                Parameter,
            ),
            row("ADBE Black&White-0007", To::Prop("tint"), Number, Absent),
            row(
                "ADBE Black&White-0008",
                To::Prop("tint-color"),
                Color,
                Parameter,
            ),
        ],
        parts: &[],
        switch: None,
    },
    Kind {
        match_name: "ADBE CurvesCustom",
        item: "curves",
        table: &EFFECTS,
        rows: &[
            row(
                "ADBE CurvesCustom-0000",
                To::Ignore("which curve the effect's panel shows"),
                Number,
                Absent,
            ),
            row("ADBE CurvesCustom-0001", To::Curves, Number, Absent),
            row(
                "ADBE Force CPU GPU",
                To::Ignore("where the original renders it"),
                Number,
                Absent,
            ),
        ],
        parts: &[],
        switch: None,
    },
];

/// The policy for groups that are not items, by match name.
pub(super) static POLICIES: [(&str, Policy); 32] = [
    ("ADBE Root Vectors Group", Policy::Through),
    ("ADBE Effect Parade", Policy::Through),
    ("ADBE Text Animators", Policy::Through),
    ("ADBE Mask Parade", Policy::Through),
    ("ADBE Layer Styles", Policy::Styles),
    (
        "ADBE Plane Options Group",
        Policy::Ignore("options of 3D layers"),
    ),
    (
        "ADBE Extrsn Options Group",
        Policy::Ignore(
            "extrusion of 3D layers, which only renderers other than the classic one draw \
             (they are reported)",
        ),
    ),
    (
        "ADBE Material Options Group",
        Policy::Ignore("materials of 3D layers, which only lights change (they are reported)"),
    ),
    ("ADBE Audio Group", Policy::Ignore("audio")),
    ("ADBE Data Group", Policy::Ignore("data for templates")),
    (
        "ADBE Layer Overrides",
        Policy::Ignore("properties exposed to other compositions"),
    ),
    ("ADBE Layer Sets", Policy::Ignore("layer sets of templates")),
    (
        "ADBE Source Options Group",
        Policy::Ignore("options of the source"),
    ),
    (
        "ADBE Vector Materials Group",
        Policy::Ignore("materials of 3D shapes"),
    ),
    (
        "ADBE Vector Stroke Dashes",
        Policy::OnlyEmpty("dashed strokes"),
    ),
    (
        "ADBE Vector Stroke Taper",
        Policy::OnlyEmpty("tapered strokes"),
    ),
    ("ADBE Vector Stroke Wave", Policy::OnlyEmpty("wavy strokes")),
    (
        "ADBE Effect Built In Params",
        Policy::OnlyEmpty("an effect's compositing options"),
    ),
    (
        "ADBE Text Path Options",
        Policy::OnlyEmpty("text on a path"),
    ),
    ("ADBE Slider Control", Policy::Control),
    ("ADBE Angle Control", Policy::Control),
    ("ADBE Checkbox Control", Policy::Control),
    ("ADBE Color Control", Policy::Control),
    ("ADBE Point Control", Policy::Control),
    ("ADBE Point3D Control", Policy::Control),
    ("ADBE Layer Control", Policy::Control),
    // A waveform, particles and stars, each mapped once a project shows its
    // parameters.
    ("ADBE AudWave", Policy::InPlace),
    ("CC Particle World", Policy::InPlace),
    ("CC Star Burst", Policy::InPlace),
    ("tc Particular", Policy::InPlace),
    ("tc Form", Policy::InPlace),
    (
        "ADBE Light Options Group",
        Policy::Ignore("the options of a light layer, which is reported"),
    ),
];

/// A range selector's rows when it counts in percent of the parts.
static RANGE_IN_PERCENT: [Row; 16] = [
    row(
        "ADBE Text Percent Start",
        To::Prop("start"),
        Percent,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Percent End",
        To::Prop("end"),
        Percent,
        Missing::Number(100.0),
    ),
    row(
        "ADBE Text Percent Offset",
        To::Prop("offset"),
        Percent,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Index Start",
        To::Ignore("the range in parts, which percent units leave unused"),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Index End",
        To::Ignore("the range in parts, which percent units leave unused"),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Index Offset",
        To::Ignore("the range in parts, which percent units leave unused"),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Range Units",
        To::Setting("units", RANGE_UNITS),
        Number,
        Missing::Number(1.0),
    ),
    row(
        "ADBE Text Range Type2",
        To::Setting("by", BASED_ON),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Selector Mode",
        To::Setting("mode", MODES),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Selector Max Amount",
        To::Prop("amount"),
        Percent,
        Missing::Number(100.0),
    ),
    row(
        "ADBE Text Range Shape",
        To::Setting("shape", SHAPES_OF_RANGE),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Selector Smoothness",
        To::Prop("smooth"),
        Percent,
        Missing::Number(100.0),
    ),
    row(
        "ADBE Text Levels Max Ease",
        To::Prop("ease-high"),
        Percent,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Levels Min Ease",
        To::Prop("ease-low"),
        Percent,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Randomize Order",
        To::OnlyDefault("random order"),
        Number,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Random Seed",
        To::Ignore("the seed of an order that is not random"),
        Number,
        Absent,
    ),
];

/// A range selector's rows when it counts in parts (`ADBE Text Range Units`
/// 2): its start, end and offset are the index rows, in parts.
static RANGE_IN_INDEX: [Row; 16] = [
    row(
        "ADBE Text Percent Start",
        To::Ignore("the range in percent, which index units leave unused"),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Percent End",
        To::Ignore("the range in percent, which index units leave unused"),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Percent Offset",
        To::Ignore("the range in percent, which index units leave unused"),
        Number,
        Absent,
    ),
    row("ADBE Text Index Start", To::Prop("start"), Number, Absent),
    row("ADBE Text Index End", To::Prop("end"), Number, Absent),
    row("ADBE Text Index Offset", To::Prop("offset"), Number, Absent),
    row(
        "ADBE Text Range Units",
        To::Setting("units", RANGE_UNITS),
        Number,
        Missing::Number(1.0),
    ),
    row(
        "ADBE Text Range Type2",
        To::Setting("by", BASED_ON),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Selector Mode",
        To::Setting("mode", MODES),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Selector Max Amount",
        To::Prop("amount"),
        Percent,
        Missing::Number(100.0),
    ),
    row(
        "ADBE Text Range Shape",
        To::Setting("shape", SHAPES_OF_RANGE),
        Number,
        Absent,
    ),
    row(
        "ADBE Text Selector Smoothness",
        To::Prop("smooth"),
        Percent,
        Missing::Number(100.0),
    ),
    row(
        "ADBE Text Levels Max Ease",
        To::Prop("ease-high"),
        Percent,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Levels Min Ease",
        To::Prop("ease-low"),
        Percent,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Randomize Order",
        To::OnlyDefault("random order"),
        Number,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Text Random Seed",
        To::Ignore("the seed of an order that is not random"),
        Number,
        Absent,
    ),
];

/// The scene's range units by the project's code.
const RANGE_UNITS: &[(i64, &str)] = &[(1, "fraction"), (2, "index")];

/// Whether range units of `code` count in percent: anything but index units.
fn in_percent(code: f64) -> bool {
    code != 2.0
}

/// The kind for the group `match_name`, if it becomes an item.
pub(super) fn kind(match_name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|kind| kind.match_name == match_name)
}

/// The policy for the group `match_name`, if it has one. Pseudo effects,
/// made of controls only, are expression controls.
pub(super) fn policy(match_name: &str) -> Option<Policy> {
    if match_name.starts_with("Pseudo/") {
        return Some(Policy::Control);
    }
    POLICIES
        .iter()
        .find(|(name, _)| *name == match_name)
        .map(|(_, policy)| *policy)
}

/// The properties of a layer's root group and their fate, by match name.
pub(super) static LAYER_ROOT: [Row; 3] = [
    row("ADBE Marker", To::Ignore("markers"), Number, Absent),
    row("ADBE Time Remapping", To::Prop("remap"), Number, Absent),
    row(
        "ADBE Layer Source Alternate",
        To::Ignore("the source's alternate"),
        Number,
        Absent,
    ),
];

#[cfg(test)]
#[path = "../../tests/unit/map/rows.rs"]
mod tests;
