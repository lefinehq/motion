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
    /// The five curves of a curves effect, from the data the project keeps
    /// for them (`aRbp`).
    Curves,
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
    /// A checkbox that picks other rows when it is off.
    pub switch: Option<Switch>,
}

/// A checkbox of a kind that changes where other properties go, such as a
/// transform's uniform scale.
#[derive(Debug)]
pub(super) struct Switch {
    /// The checkbox's match name.
    pub match_name: &'static str,
    /// Whether it is on where the project leaves it out.
    pub on: bool,
    /// The kind's rows when it is off.
    pub off_rows: &'static [Row],
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
                crate::model::Stream::Static(crate::model::Value::Number(n)) => Some(*n != 0.0),
                _ => None,
            })
            .unwrap_or(switch.on);
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
}

const LINE_CAPS: &[(i64, &str)] = &[(1, "butt"), (2, "round"), (3, "square")];
const LINE_JOINS: &[(i64, &str)] = &[(1, "miter"), (2, "round"), (3, "bevel")];
const FILL_RULES: &[(i64, &str)] = &[(1, "nonzero"), (2, "evenodd")];
/// Trim Multiple Shapes: simultaneously cuts each outline the same way,
/// individually cuts them one after another as one line.
const TRIM_MODES: &[(i64, &str)] = &[(1, "each"), (2, "joined")];
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

use Convert::{Color, Number, Path, Percent, Point, PointPercent};
use Missing::{Absent, CompCentre, Parameter, SourceCentre};

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
pub(super) static KINDS: [Kind; 18] = [
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
                To::OnlyDefault("tracking before and after"),
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
            on: true,
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
                To::Setting("channel", CHANNELS),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0002",
                To::Ignore("the histogram, which only shows"),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0003",
                To::Prop("input-black"),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0004",
                To::Prop("input-white"),
                Number,
                Absent,
            ),
            row("ADBE Easy Levels2-0005", To::Prop("gamma"), Number, Absent),
            row(
                "ADBE Easy Levels2-0006",
                To::Prop("output-black"),
                Number,
                Absent,
            ),
            row(
                "ADBE Easy Levels2-0007",
                To::Prop("output-white"),
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
    ("ADBE Slider Control", Policy::Control),
    ("ADBE Angle Control", Policy::Control),
    ("ADBE Checkbox Control", Policy::Control),
    ("ADBE Color Control", Policy::Control),
    ("ADBE Point Control", Policy::Control),
    ("ADBE Point3D Control", Policy::Control),
    ("ADBE Layer Control", Policy::Control),
    (
        "ADBE Camera Options Group",
        Policy::Ignore("the options of a camera layer, which is reported"),
    ),
];

/// Effects that draw something sparse of their own in place of what their
/// layer draws: particles, beams, grids and audio shapes. While the scene
/// cannot draw one, its layer draws nothing rather than the solid the
/// effect would have covered. Effects that fill their layer, such as
/// gradients and noise, are left out: until the scene draws them, their
/// solid stands closer to what they draw than nothing does.
pub(super) const REPLACING: &[&str] = &[
    "ADBE Grid",
    "ADBE AudSpect",
    "ADBE AudWave",
    "ADBE Laser",
    "CC Particle World",
    "CC Particle Systems II",
    "tc Particular",
    "tc Form",
];

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
