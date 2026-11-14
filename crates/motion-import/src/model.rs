//! The typed project that `decode` builds from a file: items, compositions,
//! layers and their property trees. It derives `Serialize`, so
//! `motion inspect --json` prints it as it is.
//!
//! Times are seconds and frame rates frames per second. Other values keep the
//! file's units (pixels, degrees, opacity and scale as fractions of 1), except
//! colours, whose channels are fractions from 0 to 1.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// The name serde gives `value`, such as `text_document` or `alpha_inverted`,
/// so the report and the tree name things as `--json` does.
pub(crate) fn label(value: &impl Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        Ok(other) => other.to_string(),
        Err(error) => error.to_string(),
    }
}

/// A whole `.aep` project.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    /// The file version stored in `head`, which decides some record layouts.
    pub version: Version,
    /// The project's colour depth: 8, 16 or 32 bits per channel, when known.
    pub bits_per_channel: Option<u8>,
    /// Every item. A folder's contents follow the folder and name it as theirs.
    pub items: Vec<Item>,
    /// The chunks the reader passed over, by name, with how often each appears.
    pub skipped: BTreeMap<String, usize>,
}

/// A file version: the major number grows with each release of the desktop tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Version {
    /// The major number, such as 93.
    pub major: u16,
    /// The minor number, which differs between builds of one release.
    pub minor: u16,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// One project item: a folder, a composition or a footage item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    /// The file's id, shared by items and layers, so links survive.
    pub id: u32,
    /// The name shown for the item; for footage without a name of its own,
    /// the solid's, the placeholder's or the file's name.
    pub name: String,
    /// The id of the folder that holds the item, or `None` at the root.
    pub folder: Option<u32>,
    /// What the item is.
    #[serde(flatten)]
    pub kind: ItemKind,
}

/// What an item is, with the settings of its kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemKind {
    /// A folder; its contents name it as their folder.
    Folder,
    /// A composition.
    Composition(Composition),
    /// Footage: a solid, a file or a placeholder.
    Footage(Footage),
}

impl ItemKind {
    /// The kind's name, as `--json` writes it.
    pub fn name(&self) -> &'static str {
        match self {
            ItemKind::Folder => "folder",
            ItemKind::Composition(_) => "composition",
            ItemKind::Footage(_) => "footage",
        }
    }
}

/// A composition's settings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Composition {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Pixel aspect ratio: 1 for square pixels.
    pub pixel_aspect: f64,
    /// Frames per second.
    pub frame_rate: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// Background colour.
    pub background: Color,
    /// The part of the composition that renders by default.
    pub work_area: Span,
    /// The layers, top first.
    pub layers: Vec<Layer>,
}

/// One layer of a composition.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layer {
    /// The file's id, which parent and matte links refer to.
    pub id: u32,
    /// The name shown for the layer; its source item's when it has none.
    pub name: String,
    /// What sort of layer it is.
    pub kind: LayerKind,
    /// The id of the item it shows, if any.
    pub source: Option<u32>,
    /// The id of its parent layer, if any.
    pub parent: Option<u32>,
    /// When it starts showing, in composition seconds.
    pub in_point: f64,
    /// When it stops showing, in composition seconds.
    pub out_point: f64,
    /// Where its own time 0 falls, in composition seconds.
    pub start_time: f64,
    /// How fast its own time runs against the composition's: 2 plays at half
    /// speed, a negative value plays backwards.
    pub stretch: f64,
    /// Its on-off switches.
    pub switches: Switches,
    /// How it blends with the layers below.
    pub blend_mode: BlendMode,
    /// How it uses its track matte.
    pub track_matte: TrackMatte,
    /// The id of its track matte layer, in files that store one; older files
    /// use the layer above.
    pub matte_layer: Option<u32>,
    /// The root of its property tree: transform, masks, effects, contents.
    pub properties: Group,
}

/// A property group, such as `ADBE Transform Group`, a mask or an effect.
/// Match names are data: the tree has no type per property.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    /// The name the file matches the group by, such as `ADBE Effect Parade`;
    /// empty on a layer's root group.
    pub match_name: String,
    /// The name the user gave it, if any (`Mask 1`, `Group 1`).
    pub name: Option<String>,
    /// Whether it applies; a switched-off effect or layer style does not.
    pub enabled: bool,
    /// Its groups and properties, in file order.
    pub children: Vec<Child>,
}

impl Group {
    /// Every property below the group, depth first.
    pub fn properties(&self) -> Box<dyn Iterator<Item = &Property> + '_> {
        Box::new(self.children.iter().flat_map(|child| match child {
            Child::Group(group) => group.properties(),
            Child::Property(property) => Box::new(std::iter::once(property)),
        }))
    }
}

/// One child of a [`Group`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Child {
    /// A nested group.
    Group(Group),
    /// A property.
    Property(Property),
}

/// An animatable property, such as `ADBE Position`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Property {
    /// The name the file matches the property by.
    pub match_name: String,
    /// The name the user gave it, if any.
    pub name: Option<String>,
    /// Whether it applies.
    pub enabled: bool,
    /// Its value, or its keyframes.
    pub value: Stream,
    /// The expression that computes it, if any.
    pub expression: Option<Expression>,
    /// The smallest value it takes, where the file stores one.
    pub min: Option<f64>,
    /// The largest value it takes, where the file stores one.
    pub max: Option<f64>,
}

/// A property's value over time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    /// One value at all times.
    Static(Value),
    /// Keyframes, sorted by time.
    Animated(Vec<Keyframe>),
}

/// One keyframe of a property
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#interpreting-keyframes)).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Keyframe {
    /// When it is, in composition seconds.
    pub time: f64,
    /// Its value.
    pub value: Value,
    /// How the segment that ends here moves.
    pub in_interpolation: Interpolation,
    /// How the segment that starts here moves.
    pub out_interpolation: Interpolation,
    /// The temporal ease into the keyframe: one per dimension for numbers,
    /// one for the whole value for spatial values and colours.
    pub in_ease: Vec<Ease>,
    /// The temporal ease out of the keyframe, laid out as `in_ease`.
    pub out_ease: Vec<Ease>,
    /// The motion path's tangents, on spatial properties.
    pub spatial: Option<Spatial>,
    /// The out ease follows the in ease.
    pub continuous: bool,
    /// The ease is computed from the neighbouring keyframes.
    pub auto_bezier: bool,
    /// The time is derived from the path, between the keyframes around it.
    pub roving: bool,
}

/// A temporal ease on one side of a keyframe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Ease {
    /// Speed, in the value's units per second.
    pub speed: f64,
    /// How far the ease reaches into the segment, from 0 to 1.
    pub influence: f64,
}

/// A spatial keyframe's motion-path tangents, as offsets from its value.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spatial {
    /// The tangent into the keyframe.
    pub in_tangent: Vec<f64>,
    /// The tangent out of the keyframe.
    pub out_tangent: Vec<f64>,
    /// The path runs smoothly through the keyframe.
    pub continuous: bool,
    /// The tangents are computed from the neighbouring keyframes.
    pub auto_bezier: bool,
}

/// An expression attached to a property.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expression {
    /// Its source text, line breaks as typed.
    pub text: String,
    /// Whether it is switched on.
    pub enabled: bool,
}

/// One value of a property.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    /// A number: an opacity, a rotation, a slider.
    Number(f64),
    /// A 2D vector: a size, a shape position.
    Vector2([f64; 2]),
    /// A 3D vector: a position, an anchor point, a scale.
    Vector3([f64; 3]),
    /// A colour.
    Color(Color),
    /// A bezier path: a mask's or a shape's.
    Path(Path),
    /// A text layer's text and its style.
    Text(Box<TextDocument>),
    /// A value the reader does not decode yet.
    Opaque {
        /// What sort of value it is.
        reason: Reason,
    },
}

/// A bezier path
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listshap)).
/// A shape's path is in the pixels of its group; a mask's is in the pixels
/// of a layer without a source, and in fractions of the source's size on a
/// layer with one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Path {
    /// The last vertex joins the first.
    pub closed: bool,
    /// The vertices, in order.
    pub vertices: Vec<Vertex>,
}

/// One vertex of a [`Path`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Vertex {
    /// Where it is.
    pub point: [f64; 2],
    /// The incoming control point, as an offset from `point`.
    pub in_tangent: [f64; 2],
    /// The outgoing control point, as an offset from `point`.
    pub out_tangent: [f64; 2],
}

/// What sort of value an [`Value::Opaque`] holds, so the report can count
/// how often each appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// The colour stops of a gradient.
    Gradient,
    /// Layer markers.
    Markers,
    /// A stream whose value lives in a record of its own, such as an
    /// effect's arbitrary data.
    Valueless,
    /// A value with a number of dimensions no stream kind has.
    Dimensions,
    /// A property whose body is neither a stream nor a group, such as a
    /// layer's alternate source.
    OtherBody,
}

/// A layer's switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Switches {
    /// It renders.
    pub enabled: bool,
    /// It is a 3D layer.
    pub three_d: bool,
    /// It is soloed.
    pub solo: bool,
    /// It is hidden from the timeline when shy layers are hidden.
    pub shy: bool,
    /// It is a guide, which does not render.
    pub guide: bool,
    /// It has motion blur.
    pub motion_blur: bool,
    /// It is a null: a layer that only carries a transform.
    pub null: bool,
    /// It is an adjustment layer, applying its effects to the layers below.
    pub adjustment: bool,
}

/// Declares a unit enum read from one of the file's numeric codes: the
/// variants are the table, and `Other` keeps a code the reader does not know.
macro_rules! coded {
    (
        $(#[$meta:meta])*
        pub enum $name:ident: $code:ty {
            $($(#[$doc:meta])* $variant:ident = $pattern:pat,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($(#[$doc])* $variant,)*
            /// A code this reader does not know yet.
            Other($code),
        }

        impl From<$code> for $name {
            fn from(code: $code) -> Self {
                match code {
                    $($pattern => Self::$variant,)*
                    other => Self::Other(other),
                }
            }
        }
    };
}

mod text;

pub use text::{
    BaselineOption, Caps, CharacterStyle, ComposedLine, ComposedSegment, Justification, Kerning,
    LeadingType, LineJoin, ParagraphStyle, Run, TextDocument, TextFrame,
};

coded! {
    /// What sort of layer it is
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#ldta), 0x83).
    pub enum LayerKind: u8 {
        /// Footage, a solid, a composition, a null or an adjustment layer.
        Av = 0,
        /// A light.
        Light = 1,
        /// A camera.
        Camera = 2,
        /// Text.
        Text = 3,
        /// Vector shapes.
        Shape = 4,
        /// A 3D model.
        Model = 5,
        /// A parametric mesh.
        Mesh = 7,
    }
}

coded! {
    /// How a layer blends with the layers below
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#ldta), 0x63).
    pub enum BlendMode: u8 {
        /// Normal; cameras and lights store 0.
        Normal = 0 | 2,
        /// Dissolve.
        Dissolve = 3,
        /// Add.
        Add = 4,
        /// Multiply.
        Multiply = 5,
        /// Screen.
        Screen = 6,
        /// Overlay.
        Overlay = 7,
        /// Soft light.
        SoftLight = 8,
        /// Hard light.
        HardLight = 9,
        /// Darken.
        Darken = 10,
        /// Lighten.
        Lighten = 11,
        /// Classic difference.
        ClassicDifference = 12,
        /// Hue.
        Hue = 13,
        /// Saturation.
        Saturation = 14,
        /// Colour.
        Color = 15,
        /// Luminosity.
        Luminosity = 16,
        /// Stencil alpha.
        StencilAlpha = 17,
        /// Stencil luma.
        StencilLuma = 18,
        /// Silhouette alpha.
        SilhouetteAlpha = 19,
        /// Silhouette luma.
        SilhouetteLuma = 20,
        /// Luminescent premultiply.
        LuminescentPremultiply = 21,
        /// Alpha add.
        AlphaAdd = 22,
        /// Classic colour dodge.
        ClassicColorDodge = 23,
        /// Classic colour burn.
        ClassicColorBurn = 24,
        /// Exclusion.
        Exclusion = 25,
        /// Difference.
        Difference = 26,
        /// Colour dodge.
        ColorDodge = 27,
        /// Colour burn.
        ColorBurn = 28,
        /// Linear dodge.
        LinearDodge = 29,
        /// Linear burn.
        LinearBurn = 30,
        /// Linear light.
        LinearLight = 31,
        /// Vivid light.
        VividLight = 32,
        /// Pin light.
        PinLight = 33,
        /// Hard mix.
        HardMix = 34,
        /// Lighter colour.
        LighterColor = 35,
        /// Darker colour.
        DarkerColor = 36,
        /// Subtract.
        Subtract = 37,
        /// Divide.
        Divide = 38,
    }
}

coded! {
    /// How a layer uses its track matte
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#ldta), 0x6B).
    pub enum TrackMatte: u8 {
        /// No matte.
        None = 0,
        /// The matte's alpha.
        Alpha = 1,
        /// The matte's alpha, inverted.
        AlphaInverted = 2,
        /// The matte's luminance.
        Luma = 3,
        /// The matte's luminance, inverted.
        LumaInverted = 4,
    }
}

/// A time range in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Span {
    /// Where the range starts.
    pub start: f64,
    /// Where the range ends.
    pub end: f64,
}

/// A footage item's media: its size and timing, and where it comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Footage {
    /// Width in pixels; 0 for audio.
    pub width: u32,
    /// Height in pixels; 0 for audio.
    pub height: u32,
    /// Pixel aspect ratio: 1 for square pixels.
    pub pixel_aspect: f64,
    /// Frames per second it plays at; 0 for stills.
    pub frame_rate: f64,
    /// Duration in seconds; 0 for stills and solids.
    pub duration: f64,
    /// Where the pixels come from.
    #[serde(flatten)]
    pub source: Source,
}

/// Where a footage item's pixels come from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Source {
    /// A solid colour.
    Solid {
        /// The colour.
        color: Color,
    },
    /// A file on disk, or the folder of an image sequence.
    File {
        /// The absolute path as stored, or `None` when its record cannot be read.
        path: Option<String>,
    },
    /// A stand-in for missing media.
    Placeholder,
}

/// A colour, each channel a fraction from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Color {
    /// Red.
    pub r: f64,
    /// Green.
    pub g: f64,
    /// Blue.
    pub b: f64,
    /// Alpha.
    pub a: f64,
}

coded! {
    /// How a keyframe's side of a segment moves
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#keyframe-items)).
    pub enum Interpolation: u8 {
        /// A straight line.
        Linear = 1,
        /// A curve shaped by the eases.
        Bezier = 2,
        /// The value holds until the next keyframe.
        Hold = 3,
    }
}
