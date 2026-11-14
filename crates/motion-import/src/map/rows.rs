//! The mapping tables: for each project group the scene draws, the item kind
//! it becomes and a row per property, saying where the property goes, how its
//! value converts and what it is when the project leaves it out. Adding a
//! supported property is adding a row; a test checks every row's target
//! against the scene format's registries.

use motion_format::item::{ANIMATORS, EFFECTS, ItemKind, SELECTORS, SHAPES};

use super::keys::Convert;

/// Where a property goes.
#[derive(Debug, Clone, Copy)]
pub(super) enum To {
    /// A property of the item, or one component of it, by its scene name.
    Prop(&'static str),
    /// A setting of the item: the project's code for each choice.
    Setting(&'static str, &'static [(i64, &'static str)]),
    /// Nowhere: it changes nothing a frame shows, for this reason.
    Ignore(&'static str),
    /// Nowhere, which is right only at its default; any other value is
    /// reported as unsupported, for this reason.
    OnlyDefault(&'static str),
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
    /// The centre of the layer's source, or the origin for a layer without one.
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
}

impl Kind {
    /// The scene kind.
    pub(super) fn scene_kind(&self) -> &'static ItemKind {
        self.table
            .iter()
            .find(|kind| kind.name == self.item)
            .expect("a mapping table names a scene kind")
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
}

const LINE_CAPS: &[(i64, &str)] = &[(1, "butt"), (2, "round"), (3, "square")];
const LINE_JOINS: &[(i64, &str)] = &[(1, "miter"), (2, "round"), (3, "bevel")];
const FILL_RULES: &[(i64, &str)] = &[(1, "nonzero"), (2, "evenodd")];
const BASED_ON: &[(i64, &str)] = &[(1, "char"), (2, "char-no-space"), (3, "word"), (4, "line")];
const MODES: &[(i64, &str)] = &[
    (1, "add"),
    (2, "subtract"),
    (3, "intersect"),
    (4, "min"),
    (5, "max"),
    (6, "difference"),
];
const SHAPES_OF_RANGE: &[(i64, &str)] = &[
    (1, "square"),
    (2, "ramp-up"),
    (3, "ramp-down"),
    (4, "triangle"),
    (5, "round"),
    (6, "smooth"),
];

use Convert::{Color, Number, Path, Percent, Point, PointPercent};
use Missing::{Absent, CompCentre, Parameter, SourceCentre};

/// A layer's transform, in `ADBE Transform Group`. Layer scale and opacity
/// are fractions in the project already.
pub(super) static TRANSFORM: [Row; 10] = [
    row("ADBE Anchor Point", To::Prop("anchor"), Point, SourceCentre),
    row("ADBE Position", To::Prop("position"), Point, CompCentre),
    row("ADBE Position_0", To::Prop("x"), Number, Absent),
    row("ADBE Position_1", To::Prop("y"), Number, Absent),
    row(
        "ADBE Position_2",
        To::OnlyDefault("depth on a 3D layer"),
        Number,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Scale",
        To::Prop("scale"),
        Point,
        Missing::Vector([1.0, 1.0]),
    ),
    row(
        "ADBE Rotate Z",
        To::Prop("rotate"),
        Number,
        Missing::Number(0.0),
    ),
    row(
        "ADBE Opacity",
        To::Prop("opacity"),
        Number,
        Missing::Number(1.0),
    ),
    row(
        "ADBE Orientation",
        To::OnlyDefault("3D orientation"),
        Point,
        Absent,
    ),
    row(
        "ADBE Envir Appear in Reflect",
        To::Ignore("environment reflections of 3D layers"),
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
pub(super) static KINDS: [Kind; 9] = [
    Kind {
        match_name: "ADBE Vector Group",
        item: "group",
        table: &SHAPES,
        rows: &VECTOR_TRANSFORM,
        parts: &[
            ("ADBE Vectors Group", Part::Items),
            ("ADBE Vector Transform Group", Part::Rows),
        ],
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
                To::OnlyDefault("inverted fill"),
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
                To::OnlyDefault("tracking before and after"),
                Number,
                Missing::Number(1.0),
            ),
        ],
        parts: &[
            ("ADBE Text Selectors", Part::Items),
            ("ADBE Text Animator Properties", Part::Rows),
        ],
    },
    Kind {
        match_name: "ADBE Text Selector",
        item: "range",
        table: &SELECTORS,
        rows: &[
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
                To::OnlyDefault("ranges in index units"),
                Number,
                Absent,
            ),
            row(
                "ADBE Text Index End",
                To::OnlyDefault("ranges in index units"),
                Number,
                Absent,
            ),
            row(
                "ADBE Text Index Offset",
                To::OnlyDefault("ranges in index units"),
                Number,
                Absent,
            ),
            row(
                "ADBE Text Range Units",
                To::OnlyDefault("ranges in index units"),
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
                To::OnlyDefault("ease high"),
                Number,
                Missing::Number(0.0),
            ),
            row(
                "ADBE Text Levels Min Ease",
                To::OnlyDefault("ease low"),
                Number,
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
        ],
        parts: &[("ADBE Text Range Advanced", Part::Rows)],
    },
];

/// The policy for groups that are not items, by match name.
pub(super) static POLICIES: [(&str, Policy); 28] = [
    ("ADBE Root Vectors Group", Policy::Through),
    ("ADBE Effect Parade", Policy::Through),
    ("ADBE Text Animators", Policy::Through),
    ("ADBE Mask Parade", Policy::OnlyEmpty("masks")),
    ("ADBE Layer Styles", Policy::Styles),
    (
        "ADBE Plane Options Group",
        Policy::Ignore("options of 3D layers"),
    ),
    (
        "ADBE Extrsn Options Group",
        Policy::Ignore("extrusion of 3D layers"),
    ),
    (
        "ADBE Material Options Group",
        Policy::Ignore("materials of 3D layers"),
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
    (
        "ADBE Text More Options",
        Policy::OnlyEmpty("text anchor grouping and render order"),
    ),
    (
        "ADBE Slider Control",
        Policy::Ignore("an expression control, which draws nothing"),
    ),
    (
        "ADBE Angle Control",
        Policy::Ignore("an expression control, which draws nothing"),
    ),
    (
        "ADBE Checkbox Control",
        Policy::Ignore("an expression control, which draws nothing"),
    ),
    (
        "ADBE Color Control",
        Policy::Ignore("an expression control, which draws nothing"),
    ),
    (
        "ADBE Point Control",
        Policy::Ignore("an expression control, which draws nothing"),
    ),
    (
        "ADBE Point3D Control",
        Policy::Ignore("an expression control, which draws nothing"),
    ),
    (
        "ADBE Layer Control",
        Policy::Ignore("an expression control, which draws nothing"),
    ),
    (
        "ADBE Camera Options Group",
        Policy::Ignore("the options of a camera layer, which is reported"),
    ),
];

/// The kind for the group `match_name`, if it becomes an item.
pub(super) fn kind(match_name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|kind| kind.match_name == match_name)
}

/// The policy for the group `match_name`, if it has one.
pub(super) fn policy(match_name: &str) -> Option<Policy> {
    POLICIES
        .iter()
        .find(|(name, _)| *name == match_name)
        .map(|(_, policy)| *policy)
}

/// The properties of a layer's root group and their fate, by match name.
pub(super) static LAYER_ROOT: [Row; 3] = [
    row("ADBE Marker", To::Ignore("markers"), Number, Absent),
    row(
        "ADBE Time Remapping",
        To::OnlyDefault("time remapping"),
        Number,
        Absent,
    ),
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
