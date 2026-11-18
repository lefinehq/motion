//! Effects as a registry ([D34]): each is a row of [`EFFECTS`] with a name,
//! typed parameters and what it does to a layer, as an [`Apply`]: move the
//! layer's content as it is drawn, or change the pixels it drew. Adding an
//! effect is adding a row and its function; the renderer never names an
//! effect.
//!
//! A layer whose effects all move it, or can paint their change over its
//! content without reading it, is drawn as it is. Any other is drawn on a
//! canvas of its own, and its effects run in order over the pixels.
//!
//! An [`Effect`] in a display list is built with [`Effect::new`], which checks
//! its parameters against the row, so drawing it cannot fail.
//!
//! [D34]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

use std::fmt;

use nakkash_format::{Diagnostic, Kind, Result};
use vello_cpu::RenderContext;
use vello_cpu::kurbo::{Affine, BezPath, Point, Rect, Vec2};
use vello_cpu::peniko::{BlendMode, Compose, Mix};

use crate::list::Node;
use crate::pixels::Pixels;
use crate::style::Color;
use crate::total::Total;
use crate::{blur, colour, distort, generate, particles};

/// The type of one effect parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    /// A number.
    Number,
    /// A colour.
    Color,
    /// A point, in px of the layer's own coordinates.
    Point,
    /// One name out of the parameter's `choices`.
    Choice,
    /// A curve of levels, see [`Curve`].
    Curve,
}

/// A value of an effect parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    /// A number.
    Number(f32),
    /// A colour.
    Color(Color),
    /// A point, in px of the layer's own coordinates.
    Point([f32; 2]),
    /// The index of a name in the parameter's `choices`.
    Choice(u8),
    /// A curve of levels.
    Curve(Curve),
}

/// A curve of levels: up to [`Curve::MAX`] points from input to output,
/// each 0 to 1, in rising input, joined by a natural cubic spline and flat
/// beyond the first and last. No points is the straight line, which
/// changes nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    len: u8,
    points: [[f32; 2]; Curve::MAX],
}

impl Curve {
    /// The most points a curve holds.
    pub const MAX: usize = 16;

    /// The straight line.
    pub const IDENTITY: Curve = Curve {
        len: 0,
        points: [[0.0; 2]; Curve::MAX],
    };

    /// The curve through `points`.
    ///
    /// # Errors
    /// [`Kind::OutOfRange`] for more than [`Curve::MAX`] points, a level
    /// outside 0 to 1 or an input not above the one before.
    pub fn new(points: &[[f32; 2]]) -> Result<Curve> {
        if points.len() > Curve::MAX {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("a curve of {} points", points.len()),
            )
            .expected(format!("at most {} points", Curve::MAX)));
        }
        if let Some(i) = points
            .iter()
            .position(|p| p.iter().any(|c| !(0.0..=1.0).contains(c)))
        {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("curve point {i} at {:?}", points[i]),
            )
            .expected("levels from 0 to 1"));
        }
        if let Some(i) = points.windows(2).position(|w| w[1][0] <= w[0][0]) {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("curve point {} not right of the one before", i + 1),
            )
            .expected("points in rising input"));
        }
        let mut curve = Curve::IDENTITY;
        curve.points[..points.len()].copy_from_slice(points);
        curve.len = points.len() as u8;
        Ok(curve)
    }

    /// Its points.
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points[..usize::from(self.len)]
    }
}

impl ParamValue {
    /// The type of the value.
    pub fn param_type(&self) -> ParamType {
        match self {
            ParamValue::Number(_) => ParamType::Number,
            ParamValue::Color(_) => ParamType::Color,
            ParamValue::Point(_) => ParamType::Point,
            ParamValue::Choice(_) => ParamType::Choice,
            ParamValue::Curve(_) => ParamType::Curve,
        }
    }
}

/// One parameter of an effect: its name, its default and the values it accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    /// The name display lists use.
    pub name: &'static str,
    /// Its value when a display list leaves it out; also gives its type.
    pub default: ParamValue,
    /// The allowed numbers, inclusive, for a number parameter.
    pub range: Option<(f32, f32)>,
    /// The names a choice parameter picks from.
    pub choices: &'static [&'static str],
    /// What it does.
    pub doc: &'static str,
}

/// One row of the registry.
pub struct EffectSpec {
    /// The name display lists use.
    pub name: &'static str,
    /// Its parameters, in order.
    pub params: &'static [ParamSpec],
    /// What it does.
    pub doc: &'static str,
    /// What it does to the layer, given the parameter values in the order of
    /// `params`.
    pub apply: Apply,
    /// How far what it draws reaches from what the layer drew, given the
    /// parameter values. A moving row's move places the content itself,
    /// so its reach is the move's and this says nothing more.
    pub reach: fn(&[ParamValue]) -> Reach,
}

/// How far an effect's drawing reaches from what its layer drew, in px of
/// the layer's own coordinates: the room a projected layer's buffer keeps
/// around its content ([D58]). Each row errs on the large side, since too
/// little room cuts what the effect draws and too much only costs pixels.
///
/// [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reach {
    /// No farther than this from what the layer drew, and reading no
    /// farther than this from each pixel it changes: 0 for an effect that
    /// changes pixels only where they are.
    Within(f32),
    /// Anywhere in the layer's box ([`Layer::bounds`](crate::Layer::bounds)),
    /// reading anywhere in it; anywhere at all for a layer without one.
    Bounds,
    /// Anywhere in the layer's box grown by these px on its left, top,
    /// right and bottom, reading anywhere in its box: the box it leaves to
    /// the effects after it, and the one its layer's clip grows to. For a
    /// layer without a box, anywhere at all.
    Grows([f32; 4]),
    /// Anywhere on the layer's plane, reading anywhere on it.
    Anywhere,
}

/// What an effect does to the layer it is on.
#[derive(Debug, Clone, Copy)]
pub enum Apply {
    /// Moves the layer's content: a transform in the layer's own
    /// coordinates and an opacity. Before any effect that reads pixels, it
    /// applies as the content is drawn; after one, it moves the pixels.
    Move(fn(&[ParamValue]) -> Move),
    /// Changes the pixels the layer drew.
    Pixels {
        /// The same change painted over the content without reading it,
        /// for a layer drawn as it is; `None` when the change needs the
        /// pixels.
        over: Option<fn(&mut EffectContext<'_>, &[ParamValue])>,
        /// The change, in place.
        pixels: fn(&mut Pixels, &[ParamValue], &Space),
    },
}

/// Where an effect's pixels are: lengths and points of its parameters are
/// in the layer's own coordinates, which `transform` takes to pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Space {
    /// From the layer's own coordinates to pixels of the grid.
    pub transform: Affine,
    /// The layer's box in its own coordinates, when it has one: what its
    /// rect, image, video or composition fills.
    pub bounds: Option<Rect>,
    /// The drawing of the layer the effect reads, if it reads one.
    pub input: Option<Input>,
    /// The layer's own time, in seconds.
    pub time: f64,
    /// The sound the effect reads, if it reads one: the window it asked
    /// for.
    pub sound: Option<std::sync::Arc<crate::sound::Heard>>,
    /// The outline the effect follows, if it follows one: one of the
    /// layer's masks, in the layer's own coordinates.
    pub outline: Option<std::sync::Arc<BezPath>>,
    /// The running totals of the parameters that have one, each by the
    /// parameter's index ([`Effect::with_total`]).
    pub totals: Vec<(usize, std::sync::Arc<Total>)>,
}

impl Space {
    /// The running total of parameter `param`, if it has one.
    pub fn total(&self, param: usize) -> Option<&Total> {
        self.totals
            .iter()
            .find(|(i, _)| *i == param)
            .map(|(_, total)| &**total)
    }
}

/// The drawing of a layer an effect reads, in that layer's own
/// coordinates laid on the reading layer's: its point `p` is at
/// `transform * p` of the grid, as the reading layer's own point `p` is.
#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    /// What it drew.
    pub pixels: Pixels,
    /// Its box in its own coordinates, when it has one.
    pub bounds: Option<Rect>,
}

/// How a moving effect moves the layer's content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Move {
    /// From the content's coordinates to the layer's.
    pub transform: Affine,
    /// What the content's opacity is multiplied by, 0 to 1.
    pub opacity: f32,
}

impl fmt::Debug for EffectSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EffectSpec")
            .field("name", &self.name)
            .finish()
    }
}

impl PartialEq for EffectSpec {
    fn eq(&self, other: &EffectSpec) -> bool {
        self.name == other.name
    }
}

/// Every effect the renderer can draw.
pub const EFFECTS: &[EffectSpec] = &[
    EffectSpec {
        name: "fill",
        params: &[
            ParamSpec {
                name: "color",
                default: ParamValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour the content takes",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the colour replaces the content's own, 0 to 1",
            },
            ParamSpec {
                name: "inverted",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["off", "on"],
                doc: "whether the colour fills where the content is not, inside the layer's box, \
                      with the content's transparency as its alpha",
            },
        ],
        doc: "recolours the content with one colour, keeping its alpha; inverted, fills the \
              rest of the layer's box and clears the content",
        reach: stays,
        apply: Apply::Pixels {
            over: Some(fill),
            pixels: fill_pixels,
        },
    },
    EffectSpec {
        name: "transform",
        params: &[
            ParamSpec {
                name: "position",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where `anchor` goes, in px of the layer's own coordinates",
            },
            ParamSpec {
                name: "anchor",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "the point of the content that moves to `position`, and that it scales \
                      and turns about",
            },
            ParamSpec {
                name: "scale",
                default: ParamValue::Point([1.0, 1.0]),
                range: None,
                choices: &[],
                doc: "size factor on each axis",
            },
            ParamSpec {
                name: "rotate",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "rotation in degrees, clockwise",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "what the content's opacity is multiplied by, 0 to 1",
            },
        ],
        doc: "moves, scales, turns and fades the content inside the layer",
        reach: stays,
        apply: Apply::Move(transform),
    },
    EffectSpec {
        name: "tint",
        params: &[
            ParamSpec {
                name: "black",
                default: ParamValue::Color(Color::rgba(0.0, 0.0, 0.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour black maps to",
            },
            ParamSpec {
                name: "white",
                default: ParamValue::Color(Color::rgba(1.0, 1.0, 1.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour white maps to",
            },
            ParamSpec {
                name: "amount",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the tint replaces the colours",
            },
        ],
        doc: "maps each colour's luma from black to white",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: colour::tint,
        },
    },
    EffectSpec {
        name: "hue-saturation",
        params: &[
            ParamSpec {
                name: "hue",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "degrees hues turn",
            },
            ParamSpec {
                name: "saturation",
                default: ParamValue::Number(0.0),
                range: Some((-1.0, 1.0)),
                choices: &[],
                doc: "share towards no or full saturation",
            },
            ParamSpec {
                name: "lightness",
                default: ParamValue::Number(0.0),
                range: Some((-1.0, 1.0)),
                choices: &[],
                doc: "share towards black or white",
            },
            ParamSpec {
                name: "colorize-hue",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "the hue colourising gives, in degrees",
            },
            ParamSpec {
                name: "colorize-saturation",
                default: ParamValue::Number(0.25),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "the saturation colourising gives",
            },
            ParamSpec {
                name: "colorize-lightness",
                default: ParamValue::Number(0.0),
                range: Some((-1.0, 1.0)),
                choices: &[],
                doc: "share towards black or white when colourising",
            },
            ParamSpec {
                name: "mode",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["adjust", "colorize"],
                doc: "adjust or colourise",
            },
        ],
        doc: "turns hues and changes saturation and lightness",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: colour::hue_saturation,
        },
    },
    EffectSpec {
        name: "exposure",
        params: &[
            ParamSpec {
                name: "exposure",
                default: ParamValue::Number(0.0),
                range: Some((-100.0, 100.0)),
                choices: &[],
                doc: "stops of light",
            },
            ParamSpec {
                name: "offset",
                default: ParamValue::Number(0.0),
                range: Some((-2.0, 2.0)),
                choices: &[],
                doc: "light added after the exposure",
            },
            ParamSpec {
                name: "gamma",
                default: ParamValue::Number(1.0),
                range: Some((0.1, 10.0)),
                choices: &[],
                doc: "a power of 1/gamma on the result",
            },
            ParamSpec {
                name: "space",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["linear", "gamma"],
                doc: "whether it works on light or on the values",
            },
        ],
        doc: "changes exposure in stops",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: colour::exposure,
        },
    },
    EffectSpec {
        name: "levels",
        params: &[
            ParamSpec {
                name: "input-black",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "the level that becomes output-black",
            },
            ParamSpec {
                name: "input-white",
                default: ParamValue::Number(1.0),
                range: None,
                choices: &[],
                doc: "the level that becomes output-white",
            },
            ParamSpec {
                name: "gamma",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 5.0)),
                choices: &[],
                doc: "the power between",
            },
            ParamSpec {
                name: "output-black",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "the darkest level out",
            },
            ParamSpec {
                name: "output-white",
                default: ParamValue::Number(1.0),
                range: None,
                choices: &[],
                doc: "the brightest level out",
            },
            ParamSpec {
                name: "channel",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["rgb", "red", "green", "blue", "alpha"],
                doc: "the channel it works on",
            },
        ],
        doc: "maps levels between two ranges through a gamma",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: colour::levels,
        },
    },
    EffectSpec {
        name: "invert",
        params: &[
            ParamSpec {
                name: "blend",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the original stays",
            },
            ParamSpec {
                name: "channel",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["rgb", "red", "green", "blue", "alpha"],
                doc: "the channel it inverts",
            },
        ],
        doc: "turns each level into its opposite",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: colour::invert,
        },
    },
    EffectSpec {
        name: "particles",
        params: &[
            ParamSpec {
                name: "rate",
                default: ParamValue::Number(40.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "how many particles are born a second",
            },
            ParamSpec {
                name: "longevity",
                default: ParamValue::Number(2.0),
                range: Some((0.0, 3_000.0)),
                choices: &[],
                doc: "how long each lives, in seconds",
            },
            ParamSpec {
                name: "position",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "the producer's centre, in px of the layer",
            },
            ParamSpec {
                name: "radius-x",
                default: ParamValue::Number(3.0),
                range: Some((0.0, 10_000.0)),
                choices: &[],
                doc: "how far across from `position` particles are born, in px",
            },
            ParamSpec {
                name: "radius-y",
                default: ParamValue::Number(3.0),
                range: Some((0.0, 10_000.0)),
                choices: &[],
                doc: "how far up or down from `position` particles are born, in px",
            },
            ParamSpec {
                name: "velocity",
                default: ParamValue::Number(100.0),
                range: None,
                choices: &[],
                doc: "how fast particles leave, in px a second",
            },
            ParamSpec {
                name: "gravity",
                default: ParamValue::Number(100.0),
                range: None,
                choices: &[],
                doc: "how fast they fall faster, in px a second each second; less than 0 rises",
            },
            ParamSpec {
                name: "resistance",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 1_000.0)),
                choices: &[],
                doc: "how much of their speed they lose, a share each second",
            },
            ParamSpec {
                name: "direction",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "where `direction` particles fly, in degrees clockwise from up",
            },
            ParamSpec {
                name: "extra",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 100.0)),
                choices: &[],
                doc: "how far speeds and directions vary",
            },
            ParamSpec {
                name: "birth-size",
                default: ParamValue::Number(4.0),
                range: Some((0.0, 10_000.0)),
                choices: &[],
                doc: "how wide a particle is at birth, in px",
            },
            ParamSpec {
                name: "death-size",
                default: ParamValue::Number(10.0),
                range: Some((0.0, 10_000.0)),
                choices: &[],
                doc: "how wide it is at death, in px",
            },
            ParamSpec {
                name: "size-variation",
                default: ParamValue::Number(0.5),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how far sizes vary, as a share",
            },
            ParamSpec {
                name: "max-opacity",
                default: ParamValue::Number(0.75),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "the most a particle shows",
            },
            ParamSpec {
                name: "birth-color",
                default: ParamValue::Color(Color::rgba(0.98, 0.98, 0.125, 1.0)),
                range: None,
                choices: &[],
                doc: "its colour at birth",
            },
            ParamSpec {
                name: "death-color",
                default: ParamValue::Color(Color::rgba(0.784, 0.063, 0.063, 1.0)),
                range: None,
                choices: &[],
                doc: "its colour at death",
            },
            ParamSpec {
                name: "seed",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "which particles they are",
            },
            ParamSpec {
                name: "animation",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["explosive", "direction"],
                doc: "how particles leave: every way, or towards `direction`",
            },
            ParamSpec {
                name: "shape",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["faded", "disc", "bubble"],
                doc: "what a particle looks like: a soft dot, a solid one, or a ring",
            },
            ParamSpec {
                name: "opacity-map",
                default: ParamValue::Choice(0),
                range: None,
                choices: &[
                    "fade-out",
                    "constant",
                    "fade-out-sharp",
                    "fade-in",
                    "fade-in-out",
                ],
                doc: "how a particle's opacity runs over its life",
            },
            ParamSpec {
                name: "transfer",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["over", "screen", "add"],
                doc: "how particles lay on each other",
            },
        ],
        doc: "draws particles a producer sends out over time",
        reach: anywhere,
        apply: Apply::Pixels {
            over: None,
            pixels: particles::particles,
        },
    },
    EffectSpec {
        name: "audio-spectrum",
        params: &[
            ParamSpec {
                name: "start",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the line starts, or the circle's centre, in px of the layer",
            },
            ParamSpec {
                name: "end",
                default: ParamValue::Point([200.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the line ends, or a point of the circle, in px of the layer",
            },
            ParamSpec {
                name: "height",
                default: ParamValue::Number(480.0),
                range: Some((0.0, 32_000.0)),
                choices: &[],
                doc: "how far the loudest it draws reaches from the line, in px",
            },
            ParamSpec {
                name: "duration",
                default: ParamValue::Number(90.0),
                range: Some((1.0, 30_000.0)),
                choices: &[],
                doc: "how much sound it reads about the layer's time, in ms",
            },
            ParamSpec {
                name: "offset",
                default: ParamValue::Number(0.0),
                range: Some((-30_000.0, 30_000.0)),
                choices: &[],
                doc: "how far after the layer's time the window is centred, in ms",
            },
            ParamSpec {
                name: "thickness",
                default: ParamValue::Number(3.0),
                range: Some((0.0, 4_000.0)),
                choices: &[],
                doc: "how thick a line, bar or dot is, in px",
            },
            ParamSpec {
                name: "softness",
                default: ParamValue::Number(0.5),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of a line's thickness fades out at its edges, 0 to 1",
            },
            ParamSpec {
                name: "inside-color",
                default: ParamValue::Color(Color::WHITE),
                range: None,
                choices: &[],
                doc: "the colour at the line",
            },
            ParamSpec {
                name: "outside-color",
                default: ParamValue::Color(Color::WHITE),
                range: None,
                choices: &[],
                doc: "the colour at the far ends",
            },
            ParamSpec {
                name: "from",
                default: ParamValue::Number(20.0),
                range: Some((1.0, 48_000.0)),
                choices: &[],
                doc: "the lowest frequency, in Hz",
            },
            ParamSpec {
                name: "to",
                default: ParamValue::Number(2_000.0),
                range: Some((1.0, 48_000.0)),
                choices: &[],
                doc: "the highest frequency, in Hz",
            },
            ParamSpec {
                name: "bands",
                default: ParamValue::Number(64.0),
                range: Some((1.0, 4_096.0)),
                choices: &[],
                doc: "how many frequencies it shows",
            },
            ParamSpec {
                name: "hue",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "how far the hue turns from the first band to the last, in degrees",
            },
            ParamSpec {
                name: "display",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["digital", "lines", "dots"],
                doc: "how a band shows",
            },
            ParamSpec {
                name: "side",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["both", "a", "b"],
                doc: "which side of the line bands rise on",
            },
            ParamSpec {
                name: "path",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["line", "circle"],
                doc: "where bands stand",
            },
            ParamSpec {
                name: "average",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["off", "on"],
                doc: "whether each band is averaged over short windows",
            },
            ParamSpec {
                name: "symmetry",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["on", "off"],
                doc: "whether the hue turn runs to the middle band and back",
            },
            ParamSpec {
                name: "composite",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["off", "on"],
                doc: "whether the layer's own drawing stays under the bands",
            },
        ],
        doc: "draws the spectrum of another layer's sound at the layer's time",
        reach: in_box,
        apply: Apply::Pixels {
            over: None,
            pixels: crate::sound::spectrum,
        },
    },
    EffectSpec {
        name: "audio-waveform",
        params: &[
            ParamSpec {
                name: "start",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the line starts, or the circle's centre, in px of the layer",
            },
            ParamSpec {
                name: "end",
                default: ParamValue::Point([200.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the line ends, or a point of the circle, in px of the layer",
            },
            ParamSpec {
                name: "height",
                default: ParamValue::Number(480.0),
                range: Some((0.0, 32_000.0)),
                choices: &[],
                doc: "how far the loudest it draws reaches from the line, in px",
            },
            ParamSpec {
                name: "duration",
                default: ParamValue::Number(90.0),
                range: Some((1.0, 30_000.0)),
                choices: &[],
                doc: "how much sound it reads about the layer's time, in ms",
            },
            ParamSpec {
                name: "offset",
                default: ParamValue::Number(0.0),
                range: Some((-30_000.0, 30_000.0)),
                choices: &[],
                doc: "how far after the layer's time the window is centred, in ms",
            },
            ParamSpec {
                name: "thickness",
                default: ParamValue::Number(3.0),
                range: Some((0.0, 4_000.0)),
                choices: &[],
                doc: "how thick a line, bar or dot is, in px",
            },
            ParamSpec {
                name: "softness",
                default: ParamValue::Number(0.5),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of a line's thickness fades out at its edges, 0 to 1",
            },
            ParamSpec {
                name: "inside-color",
                default: ParamValue::Color(Color::WHITE),
                range: None,
                choices: &[],
                doc: "the colour at the line",
            },
            ParamSpec {
                name: "outside-color",
                default: ParamValue::Color(Color::WHITE),
                range: None,
                choices: &[],
                doc: "the colour at the far ends",
            },
            ParamSpec {
                name: "samples",
                default: ParamValue::Number(32.0),
                range: Some((2.0, 4_096.0)),
                choices: &[],
                doc: "how many points of the wave it draws",
            },
            ParamSpec {
                name: "display",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["digital", "lines", "dots"],
                doc: "how the wave shows",
            },
            ParamSpec {
                name: "channel",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["mono", "left", "right"],
                doc: "which channel the wave follows",
            },
            ParamSpec {
                name: "path",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["line", "circle"],
                doc: "where the wave runs",
            },
            ParamSpec {
                name: "composite",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["off", "on"],
                doc: "whether the layer's own drawing stays under the wave",
            },
        ],
        doc: "draws the wave of another layer's sound at the layer's time",
        reach: in_box,
        apply: Apply::Pixels {
            over: None,
            pixels: crate::sound::waveform,
        },
    },
    EffectSpec {
        name: "displacement-map",
        params: &[
            ParamSpec {
                name: "horizontal",
                default: ParamValue::Number(5.0),
                range: Some((-32_000.0, 32_000.0)),
                choices: &[],
                doc: "the farthest a pixel moves across, in px",
            },
            ParamSpec {
                name: "vertical",
                default: ParamValue::Number(5.0),
                range: Some((-32_000.0, 32_000.0)),
                choices: &[],
                doc: "the farthest a pixel moves up or down, in px",
            },
            ParamSpec {
                name: "horizontal-from",
                default: ParamValue::Choice(0),
                range: None,
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
                doc: "which level of the map moves pixels across",
            },
            ParamSpec {
                name: "vertical-from",
                default: ParamValue::Choice(0),
                range: None,
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
                doc: "which level of the map moves pixels up or down",
            },
            ParamSpec {
                name: "behaviour",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["center", "stretch", "tile"],
                doc: "how the map lies on the layer",
            },
            ParamSpec {
                name: "edges",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["clear", "wrap"],
                doc: "whether moved pixels wrap around the box",
            },
        ],
        doc: "moves each pixel by the levels of another layer",
        reach: displacement_reach,
        apply: Apply::Pixels {
            over: None,
            pixels: distort::displacement_map,
        },
    },
    EffectSpec {
        name: "motion-tile",
        params: &[
            ParamSpec {
                name: "center",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where a tile is centred, in px of the layer",
            },
            ParamSpec {
                name: "tile-width",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "a tile's width, as a share of the layer's box",
            },
            ParamSpec {
                name: "tile-height",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "a tile's height, as a share of the layer's box",
            },
            ParamSpec {
                name: "output-width",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 300.0)),
                choices: &[],
                doc: "how wide the tiles reach, as a share of the box, about its middle",
            },
            ParamSpec {
                name: "output-height",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 300.0)),
                choices: &[],
                doc: "how high the tiles reach, as a share of the box, about its middle",
            },
            ParamSpec {
                name: "phase",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "how far every other column or row shifts, in degrees; a turn is a tile",
            },
            ParamSpec {
                name: "edges",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["repeat", "mirror"],
                doc: "whether every other tile flips",
            },
            ParamSpec {
                name: "shift",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["vertical", "horizontal"],
                doc: "whether the phase shifts columns up and down, or rows across",
            },
        ],
        doc: "repeats the layer in tiles around a point",
        reach: anywhere,
        apply: Apply::Pixels {
            over: None,
            pixels: distort::motion_tile,
        },
    },
    EffectSpec {
        name: "wrap",
        params: &[
            ParamSpec {
                name: "to",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the middle of the layer's box moves to, in px of the layer",
            },
            ParamSpec {
                name: "blend",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the layer stays as it was",
            },
        ],
        doc: "moves the layer's content within its box, wrapping around",
        reach: in_box,
        apply: Apply::Pixels {
            over: None,
            pixels: distort::wrap,
        },
    },
    EffectSpec {
        name: "turbulent-displace",
        params: &[
            ParamSpec {
                name: "amount",
                default: ParamValue::Number(50.0),
                range: Some((-10_000.0, 10_000.0)),
                choices: &[],
                doc: "how far pixels move, in percent of `size`",
            },
            ParamSpec {
                name: "size",
                default: ParamValue::Number(100.0),
                range: Some((2.0, 1_000.0)),
                choices: &[],
                doc: "how wide the noise's cells are, in px",
            },
            ParamSpec {
                name: "offset",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the noise is centred, in px of the layer",
            },
            ParamSpec {
                name: "complexity",
                default: ParamValue::Number(1.0),
                range: Some((1.0, 10.0)),
                choices: &[],
                doc: "how many octaves the noise sums",
            },
            ParamSpec {
                name: "evolution",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "how far the noise has evolved, in degrees; a turn is a cell",
            },
            ParamSpec {
                name: "seed",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "which noise it is",
            },
            ParamSpec {
                name: "displacement",
                default: ParamValue::Choice(0),
                range: None,
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
                doc: "how rough the movement is",
            },
            ParamSpec {
                name: "pinning",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["all", "left-right", "top-bottom", "none"],
                doc: "which edges of the layer's box stay put",
            },
        ],
        doc: "moves each pixel by a smooth noise",
        reach: turbulent_reach,
        apply: Apply::Pixels {
            over: None,
            pixels: distort::turbulent_displace,
        },
    },
    EffectSpec {
        name: "mirror",
        params: &[
            ParamSpec {
                name: "center",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "a point of the line it reflects across, in px of the layer",
            },
            ParamSpec {
                name: "angle",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "which way the reflected side faces, in degrees clockwise",
            },
        ],
        doc: "reflects one side of a line through the layer onto the other",
        reach: in_box,
        apply: Apply::Pixels {
            over: None,
            pixels: distort::mirror,
        },
    },
    EffectSpec {
        name: "polar-coordinates",
        params: &[
            ParamSpec {
                name: "interpolation",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how far each pixel goes towards where the conversion takes it",
            },
            ParamSpec {
                name: "conversion",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["rect-to-polar", "polar-to-rect"],
                doc: "rows into rings about the middle, or rings into rows",
            },
        ],
        doc: "turns the layer between rows and columns and rings about its middle",
        reach: in_box,
        apply: Apply::Pixels {
            over: None,
            pixels: distort::polar_coordinates,
        },
    },
    EffectSpec {
        name: "repeat-tile",
        params: &[
            ParamSpec {
                name: "right",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "how far the tiles reach right of the layer's box, in px of the layer",
            },
            ParamSpec {
                name: "left",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "how far the tiles reach left of the box",
            },
            ParamSpec {
                name: "down",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "how far the tiles reach below the box",
            },
            ParamSpec {
                name: "up",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "how far the tiles reach above the box",
            },
            ParamSpec {
                name: "tiling",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["repeat", "unfold", "brick", "none"],
                doc: "how the tiles lie: alike, every other one flipped, every other row \
                      shifted by half a tile, or none, the box grown empty",
            },
        ],
        doc: "repeats the layer's box around it",
        reach: repeat_tile_reach,
        apply: Apply::Pixels {
            over: None,
            pixels: distort::repeat_tile,
        },
    },
    EffectSpec {
        name: "ramp",
        params: &[
            ParamSpec {
                name: "start",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the ramp starts",
            },
            ParamSpec {
                name: "start-color",
                default: ParamValue::Color(Color::rgba(0.0, 0.0, 0.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour at `start`",
            },
            ParamSpec {
                name: "end",
                default: ParamValue::Point([0.0, 100.0]),
                range: None,
                choices: &[],
                doc: "where the ramp ends",
            },
            ParamSpec {
                name: "end-color",
                default: ParamValue::Color(Color::rgba(1.0, 1.0, 1.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour at `end`",
            },
            ParamSpec {
                name: "blend",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the layer's own colour stays",
            },
            ParamSpec {
                name: "shape",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["linear", "radial"],
                doc: "along a line or around `start`",
            },
        ],
        doc: "fills the layer with a gradient between two colours",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: generate::ramp,
        },
    },
    EffectSpec {
        name: "four-color",
        params: &[
            ParamSpec {
                name: "point-1",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where `color-1` is",
            },
            ParamSpec {
                name: "color-1",
                default: ParamValue::Color(Color::rgba(1.0, 1.0, 0.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the first colour",
            },
            ParamSpec {
                name: "point-2",
                default: ParamValue::Point([100.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where `color-2` is",
            },
            ParamSpec {
                name: "color-2",
                default: ParamValue::Color(Color::rgba(0.0, 1.0, 0.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the second colour",
            },
            ParamSpec {
                name: "point-3",
                default: ParamValue::Point([0.0, 100.0]),
                range: None,
                choices: &[],
                doc: "where `color-3` is",
            },
            ParamSpec {
                name: "color-3",
                default: ParamValue::Color(Color::rgba(1.0, 0.0, 1.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the third colour",
            },
            ParamSpec {
                name: "point-4",
                default: ParamValue::Point([100.0, 100.0]),
                range: None,
                choices: &[],
                doc: "where `color-4` is",
            },
            ParamSpec {
                name: "color-4",
                default: ParamValue::Color(Color::rgba(0.0, 0.0, 1.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the fourth colour",
            },
            ParamSpec {
                name: "blend",
                default: ParamValue::Number(100.0),
                range: Some((1.0, 10_000.0)),
                choices: &[],
                doc: "how far the colours mix",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the gradient shows",
            },
            ParamSpec {
                name: "mode",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["replace", "over"],
                doc: "alone, or over the layer",
            },
        ],
        doc: "fills the layer with four colours at four points, blended",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: generate::four_color,
        },
    },
    EffectSpec {
        name: "beam",
        params: &[
            ParamSpec {
                name: "start",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the line starts, in px of the layer's own coordinates",
            },
            ParamSpec {
                name: "end",
                default: ParamValue::Point([100.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the line ends, in px of the layer's own coordinates",
            },
            ParamSpec {
                name: "length",
                default: ParamValue::Number(0.25),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how long the beam is, as a share of the line",
            },
            ParamSpec {
                name: "time",
                default: ParamValue::Number(0.5),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how far the beam has travelled: its front is `time × (1 + length)` of \
                      the way along the line",
            },
            ParamSpec {
                name: "start-thickness",
                default: ParamValue::Number(8.0),
                range: Some((0.0, 4_000.0)),
                choices: &[],
                doc: "how thick the beam is at `start`, in px",
            },
            ParamSpec {
                name: "end-thickness",
                default: ParamValue::Number(8.0),
                range: Some((0.0, 4_000.0)),
                choices: &[],
                doc: "how thick the beam is at `end`, in px",
            },
            ParamSpec {
                name: "softness",
                default: ParamValue::Number(0.5),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the half thickness fades out at the edges, 0 to 1",
            },
            ParamSpec {
                name: "inside-color",
                default: ParamValue::Color(Color::rgba(1.0, 0.75, 0.98, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour along the beam's middle",
            },
            ParamSpec {
                name: "outside-color",
                default: ParamValue::Color(Color::rgba(1.0, 0.1, 0.1, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour at the beam's edges",
            },
            ParamSpec {
                name: "composite",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["off", "on"],
                doc: "whether the layer's own drawing stays under the beam",
            },
        ],
        doc: "draws a beam travelling along a line, in place of the layer",
        reach: in_box,
        apply: Apply::Pixels {
            over: None,
            pixels: generate::beam,
        },
    },
    EffectSpec {
        name: "grid",
        params: &[
            ParamSpec {
                name: "anchor",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where a corner of the cells is",
            },
            ParamSpec {
                name: "corner",
                default: ParamValue::Point([32.0, 24.0]),
                range: None,
                choices: &[],
                doc: "the opposite corner of the first cell",
            },
            ParamSpec {
                name: "width",
                default: ParamValue::Number(32.0),
                range: Some((1.0, 4_000.0)),
                choices: &[],
                doc: "a cell's width",
            },
            ParamSpec {
                name: "height",
                default: ParamValue::Number(24.0),
                range: Some((1.0, 4_000.0)),
                choices: &[],
                doc: "a cell's height",
            },
            ParamSpec {
                name: "border",
                default: ParamValue::Number(5.0),
                range: Some((0.0, 2_000.0)),
                choices: &[],
                doc: "how wide the lines are",
            },
            ParamSpec {
                name: "color",
                default: ParamValue::Color(Color::rgba(1.0, 1.0, 1.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the lines' colour",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the grid shows",
            },
            ParamSpec {
                name: "size",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["corner", "width", "width-height"],
                doc: "what sets a cell's size",
            },
            ParamSpec {
                name: "draw",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["lines", "cells"],
                doc: "the lines or the cells",
            },
            ParamSpec {
                name: "mode",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["replace", "over"],
                doc: "alone, or over the layer",
            },
        ],
        doc: "draws a grid of lines",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: generate::grid,
        },
    },
    EffectSpec {
        name: "venetian-blinds",
        params: &[
            ParamSpec {
                name: "completion",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of each stripe is hidden",
            },
            ParamSpec {
                name: "direction",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "which way the stripes run, in degrees",
            },
            ParamSpec {
                name: "width",
                default: ParamValue::Number(20.0),
                range: Some((1.0, 32_000.0)),
                choices: &[],
                doc: "how wide each stripe is",
            },
            ParamSpec {
                name: "feather",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 32_000.0)),
                choices: &[],
                doc: "how soft a stripe's edge is",
            },
        ],
        doc: "hides the layer in stripes",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: generate::venetian_blinds,
        },
    },
    EffectSpec {
        name: "fractal-noise",
        params: &[
            ParamSpec {
                name: "contrast",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 100.0)),
                choices: &[],
                doc: "how far levels spread from mid grey",
            },
            ParamSpec {
                name: "brightness",
                default: ParamValue::Number(0.0),
                range: Some((-100.0, 100.0)),
                choices: &[],
                doc: "what is added to every level",
            },
            ParamSpec {
                name: "rotation",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "how far the noise turns",
            },
            ParamSpec {
                name: "scale",
                default: ParamValue::Number(1.0),
                range: Some((0.01, 100.0)),
                choices: &[],
                doc: "the noise's size",
            },
            ParamSpec {
                name: "scale-width",
                default: ParamValue::Number(1.0),
                range: Some((0.01, 100.0)),
                choices: &[],
                doc: "the noise's width, times `scale`",
            },
            ParamSpec {
                name: "scale-height",
                default: ParamValue::Number(1.0),
                range: Some((0.01, 100.0)),
                choices: &[],
                doc: "the noise's height, times `scale`",
            },
            ParamSpec {
                name: "offset",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "where the noise is centred",
            },
            ParamSpec {
                name: "complexity",
                default: ParamValue::Number(6.0),
                range: Some((1.0, 20.0)),
                choices: &[],
                doc: "how many octaves it sums",
            },
            ParamSpec {
                name: "sub-influence",
                default: ParamValue::Number(0.7),
                range: Some((0.0, 100.0)),
                choices: &[],
                doc: "each octave's weight against the one before",
            },
            ParamSpec {
                name: "sub-scaling",
                default: ParamValue::Number(0.56),
                range: Some((0.1, 100.0)),
                choices: &[],
                doc: "each octave's size against the one before",
            },
            ParamSpec {
                name: "sub-rotation",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "how far each octave turns",
            },
            ParamSpec {
                name: "sub-offset",
                default: ParamValue::Point([0.0, 0.0]),
                range: None,
                choices: &[],
                doc: "how far each octave moves",
            },
            ParamSpec {
                name: "evolution",
                default: ParamValue::Number(0.0),
                range: None,
                choices: &[],
                doc: "how far the noise has evolved",
            },
            ParamSpec {
                name: "seed",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 100_000.0)),
                choices: &[],
                doc: "which noise it is",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of the noise shows",
            },
            ParamSpec {
                name: "fractal",
                default: ParamValue::Choice(0),
                range: None,
                choices: &[
                    "basic",
                    "turbulent-smooth",
                    "turbulent-basic",
                    "turbulent-sharp",
                ],
                doc: "how octaves sum",
            },
            ParamSpec {
                name: "noise",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["spline", "soft-linear", "linear", "block"],
                doc: "how values between lattice points blend",
            },
            ParamSpec {
                name: "overflow",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["clip", "soft-clamp", "wrap-back"],
                doc: "how levels outside 0 to 1 come back",
            },
            ParamSpec {
                name: "polarity",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["normal", "inverted"],
                doc: "whether the levels are inverted",
            },
            ParamSpec {
                name: "mode",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["over", "replace"],
                doc: "over the layer, or alone",
            },
        ],
        doc: "fills the layer with grey noise in octaves",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: generate::fractal_noise,
        },
    },
    EffectSpec {
        name: "gaussian-blur",
        params: &[
            ParamSpec {
                name: "blurriness",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 30_000.0)),
                choices: &[],
                doc: "how far it blurs, in px",
            },
            ParamSpec {
                name: "dimensions",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["both", "horizontal", "vertical"],
                doc: "which way it blurs",
            },
            ParamSpec {
                name: "edges",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["transparent", "repeat"],
                doc: "what lies beyond the edges",
            },
        ],
        doc: "blurs with a Gaussian",
        reach: gaussian_reach,
        apply: Apply::Pixels {
            over: None,
            pixels: blur::gaussian_blur,
        },
    },
    EffectSpec {
        name: "box-blur",
        params: &[
            ParamSpec {
                name: "radius",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 30_000.0)),
                choices: &[],
                doc: "the radius of each box, in px",
            },
            ParamSpec {
                name: "iterations",
                default: ParamValue::Number(3.0),
                range: Some((1.0, 50.0)),
                choices: &[],
                doc: "how many boxes it runs",
            },
            ParamSpec {
                name: "dimensions",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["both", "horizontal", "vertical"],
                doc: "which way it blurs",
            },
            ParamSpec {
                name: "edges",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["transparent", "repeat"],
                doc: "what lies beyond the edges",
            },
        ],
        doc: "blurs with boxes run one after another",
        reach: box_reach,
        apply: Apply::Pixels {
            over: None,
            pixels: blur::box_blur,
        },
    },
    EffectSpec {
        name: "drop-shadow",
        params: &[
            ParamSpec {
                name: "color",
                default: ParamValue::Color(Color::rgba(0.0, 0.0, 0.0, 1.0)),
                range: None,
                choices: &[],
                doc: "the shadow's colour",
            },
            ParamSpec {
                name: "opacity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "the shadow's opacity",
            },
            ParamSpec {
                name: "direction",
                default: ParamValue::Number(135.0),
                range: None,
                choices: &[],
                doc: "where the shadow falls, in degrees clockwise from up",
            },
            ParamSpec {
                name: "distance",
                default: ParamValue::Number(5.0),
                range: Some((0.0, 4_000.0)),
                choices: &[],
                doc: "how far the shadow falls, in px",
            },
            ParamSpec {
                name: "softness",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 30_000.0)),
                choices: &[],
                doc: "how far the shadow blurs, in px",
            },
            ParamSpec {
                name: "layer",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["shown", "hidden"],
                doc: "whether the layer is drawn over its shadow",
            },
        ],
        doc: "draws the layer's shape as a soft shadow behind it",
        reach: shadow_reach,
        apply: Apply::Pixels {
            over: None,
            pixels: blur::drop_shadow,
        },
    },
    EffectSpec {
        name: "glow",
        params: &[
            ParamSpec {
                name: "threshold",
                default: ParamValue::Number(0.6),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "the luma below which nothing glows",
            },
            ParamSpec {
                name: "radius",
                default: ParamValue::Number(10.0),
                range: Some((0.0, 1_000.0)),
                choices: &[],
                doc: "how far the glow spreads, in px",
            },
            ParamSpec {
                name: "intensity",
                default: ParamValue::Number(1.0),
                range: Some((0.0, 255.0)),
                choices: &[],
                doc: "what the glow's light is multiplied by",
            },
            ParamSpec {
                name: "dimensions",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["both", "horizontal", "vertical"],
                doc: "which way it blurs",
            },
            ParamSpec {
                name: "composite",
                default: ParamValue::Choice(0),
                range: None,
                choices: &["with-original", "glow-only", "behind-original"],
                doc: "whether the glow is laid over the layer, drawn alone, or drawn behind \
                      the layer laid over it",
            },
            ParamSpec {
                name: "operation",
                default: ParamValue::Choice(0),
                range: None,
                choices: blur::GLOW_OPERATIONS,
                doc: "how the glow and the layer combine",
            },
        ],
        doc: "adds the blurred light of the bright colours",
        reach: glow_reach,
        apply: Apply::Pixels {
            over: None,
            pixels: blur::glow,
        },
    },
    EffectSpec {
        name: "sharpen",
        params: &[ParamSpec {
            name: "amount",
            default: ParamValue::Number(0.0),
            range: Some((0.0, 4_000.0)),
            choices: &[],
            doc: "in percent of the difference from the neighbours' mean",
        }],
        doc: "sharpens edges",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: blur::sharpen,
        },
    },
    EffectSpec {
        name: "black-white",
        params: &[
            ParamSpec {
                name: "reds",
                default: ParamValue::Number(0.4),
                range: Some((-2.0, 3.0)),
                choices: &[],
                doc: "the grey reds turn into, as a share of their level",
            },
            ParamSpec {
                name: "yellows",
                default: ParamValue::Number(0.6),
                range: Some((-2.0, 3.0)),
                choices: &[],
                doc: "the grey yellows turn into, as a share of their level",
            },
            ParamSpec {
                name: "greens",
                default: ParamValue::Number(0.4),
                range: Some((-2.0, 3.0)),
                choices: &[],
                doc: "the grey greens turn into, as a share of their level",
            },
            ParamSpec {
                name: "cyans",
                default: ParamValue::Number(0.6),
                range: Some((-2.0, 3.0)),
                choices: &[],
                doc: "the grey cyans turn into, as a share of their level",
            },
            ParamSpec {
                name: "blues",
                default: ParamValue::Number(0.2),
                range: Some((-2.0, 3.0)),
                choices: &[],
                doc: "the grey blues turn into, as a share of their level",
            },
            ParamSpec {
                name: "magentas",
                default: ParamValue::Number(0.8),
                range: Some((-2.0, 3.0)),
                choices: &[],
                doc: "the grey magentas turn into, as a share of their level",
            },
            ParamSpec {
                name: "tint",
                default: ParamValue::Number(0.0),
                range: Some((0.0, 1.0)),
                choices: &[],
                doc: "how much of `tint-color` colours the grey",
            },
            ParamSpec {
                name: "tint-color",
                default: ParamValue::Color(Color::rgba(0.882_353, 0.827_451, 0.701_961, 1.0)),
                range: None,
                choices: &[],
                doc: "the colour the grey takes",
            },
        ],
        doc: "turns colours to grey, each hue as light as its share says",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: colour::black_white,
        },
    },
    EffectSpec {
        name: "curves",
        params: &[
            ParamSpec {
                name: "rgb",
                default: ParamValue::Curve(Curve::IDENTITY),
                range: None,
                choices: &[],
                doc: "the curve of red, green and blue alike, after their own",
            },
            ParamSpec {
                name: "red",
                default: ParamValue::Curve(Curve::IDENTITY),
                range: None,
                choices: &[],
                doc: "the curve of red",
            },
            ParamSpec {
                name: "green",
                default: ParamValue::Curve(Curve::IDENTITY),
                range: None,
                choices: &[],
                doc: "the curve of green",
            },
            ParamSpec {
                name: "blue",
                default: ParamValue::Curve(Curve::IDENTITY),
                range: None,
                choices: &[],
                doc: "the curve of blue",
            },
            ParamSpec {
                name: "alpha",
                default: ParamValue::Curve(Curve::IDENTITY),
                range: None,
                choices: &[],
                doc: "the curve of alpha",
            },
        ],
        doc: "maps each channel's levels through a curve: first its own, then `rgb`",
        reach: stays,
        apply: Apply::Pixels {
            over: None,
            pixels: colour::curves,
        },
    },
];

/// An effect with its parameter values, checked against its row.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    spec: &'static EffectSpec,
    values: Vec<ParamValue>,
    input: Option<Box<Node>>,
    sound: Option<std::sync::Arc<crate::sound::Heard>>,
    outline: Option<std::sync::Arc<BezPath>>,
    totals: Vec<(usize, std::sync::Arc<Total>)>,
}

impl Effect {
    /// The effect called `name` with the parameters given by name; the
    /// others take their defaults.
    ///
    /// # Errors
    /// A diagnostic for an unknown effect or parameter, a value of the wrong
    /// type, or a number outside its range; its path names the parameter.
    pub fn new(name: &str, params: &[(&str, ParamValue)]) -> Result<Effect> {
        let spec = EFFECTS
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| Diagnostic::unknown("effect", name, EFFECTS.iter().map(|s| s.name)))?;
        let mut values: Vec<ParamValue> = spec.params.iter().map(|p| p.default).collect();
        for &(param, value) in params {
            let i = index(spec, param)?;
            values[i] = checked(&spec.params[i], value).map_err(|e| e.at(param))?;
        }
        Ok(Effect {
            spec,
            values,
            input: None,
            sound: None,
            outline: None,
            totals: Vec::new(),
        })
    }

    /// The effect reading `total` as the running total of its number
    /// parameter `param` over the layer's own time, so a value that
    /// changes acts only from then on: what particles have been born
    /// stays born.
    ///
    /// # Errors
    /// A diagnostic at `param` for an unknown parameter or one that is not
    /// a number.
    pub fn with_total(mut self, param: &str, total: Total) -> Result<Effect> {
        let i = index(self.spec, param)?;
        if self.spec.params[i].default.param_type() != ParamType::Number {
            return Err(Diagnostic::new(
                Kind::WrongType,
                "only a number parameter has a running total",
            )
            .at(param)
            .expected("a number parameter"));
        }
        self.totals.retain(|(j, _)| *j != i);
        self.totals.push((i, std::sync::Arc::new(total)));
        Ok(self)
    }

    /// The running totals it reads, each by its parameter's index.
    pub fn totals(&self) -> &[(usize, std::sync::Arc<Total>)] {
        &self.totals
    }

    /// The effect reading `node`'s drawing: a layer drawn in its own
    /// coordinates, which the renderer lays on those of the layer the
    /// effect is on.
    pub fn with_input(mut self, node: Node) -> Effect {
        self.input = Some(Box::new(node));
        self
    }

    /// The effect reading `heard`, the window of sound it asked for.
    pub fn with_sound(mut self, heard: crate::sound::Heard) -> Effect {
        self.sound = Some(std::sync::Arc::new(heard));
        self
    }

    /// The effect following `outline`, in the layer's own coordinates,
    /// in place of the line or circle its parameters give.
    pub fn with_outline(mut self, outline: BezPath) -> Effect {
        self.outline = Some(std::sync::Arc::new(outline));
        self
    }

    /// The outline it follows, if any.
    pub fn outline(&self) -> Option<&std::sync::Arc<BezPath>> {
        self.outline.as_ref()
    }

    /// The sound it reads, if any.
    pub fn sound(&self) -> Option<&std::sync::Arc<crate::sound::Heard>> {
        self.sound.as_ref()
    }

    /// The node whose drawing it reads, if any.
    pub fn input(&self) -> Option<&Node> {
        self.input.as_deref()
    }

    /// The effect's row.
    pub fn spec(&self) -> &'static EffectSpec {
        self.spec
    }

    /// The value of every parameter, in the order of the row's `params`.
    pub fn values(&self) -> &[ParamValue] {
        &self.values
    }

    /// Paints the effect over the content drawn so far in `ctx`'s layer, if
    /// it can without reading it.
    pub(crate) fn paint_over(&self, ctx: &mut EffectContext<'_>) {
        if let Apply::Pixels {
            over: Some(over), ..
        } = self.spec.apply
        {
            over(ctx, &self.values);
        }
    }

    /// Whether it needs the pixels its layer drew.
    pub fn reads_pixels(&self) -> bool {
        matches!(self.spec.apply, Apply::Pixels { over: None, .. })
    }

    /// How far what it draws reaches from what its layer drew, see
    /// [`Reach`]; a moving effect's is its move's.
    pub fn reach(&self) -> Reach {
        (self.spec.reach)(&self.values)
    }

    /// The layer's box once it has run: `bounds` grown on each side by as
    /// much as it draws beyond them ([`Reach::Grows`]).
    pub fn grown(&self, bounds: Rect) -> Rect {
        match self.reach() {
            Reach::Grows([left, top, right, bottom]) => Rect::new(
                bounds.x0 - f64::from(left),
                bounds.y0 - f64::from(top),
                bounds.x1 + f64::from(right),
                bounds.y1 + f64::from(bottom),
            ),
            _ => bounds,
        }
    }

    /// How it moves the content, if it is a moving one.
    pub fn movement(&self) -> Option<Move> {
        match self.spec.apply {
            Apply::Move(movement) => Some(movement(&self.values)),
            Apply::Pixels { .. } => None,
        }
    }

    /// Applies the effect to `pixels`, a layer's drawn in `space`: a move
    /// resamples them, any other changes them in place.
    pub fn apply(&self, pixels: &mut Pixels, space: &Space) {
        match self.spec.apply {
            Apply::Move(movement) => {
                let Move { transform, opacity } = movement(&self.values);
                let inside = space.transform * transform * space.transform.inverse();
                if inside != Affine::IDENTITY {
                    *pixels = pixels.transformed(inside);
                }
                if opacity < 1.0 {
                    for pixel in pixels.data_mut() {
                        *pixel = pixel.map(|c| c * opacity);
                    }
                }
            }
            Apply::Pixels { pixels: change, .. } => change(pixels, &self.values, space),
        }
    }
}

/// The index of `spec`'s parameter called `param`.
///
/// # Errors
/// A diagnostic at `param` listing the parameters, when it has none of
/// that name.
fn index(spec: &EffectSpec, param: &str) -> Result<usize> {
    spec.params
        .iter()
        .position(|p| p.name == param)
        .ok_or_else(|| {
            Diagnostic::unknown(
                "effect parameter",
                param,
                spec.params.iter().map(|p| p.name),
            )
            .at(param)
        })
}

/// `value` if it has the parameter's type and lies in its range.
fn checked(spec: &ParamSpec, value: ParamValue) -> Result<ParamValue> {
    let expected = spec.default.param_type();
    if value.param_type() != expected {
        return Err(Diagnostic::new(
            Kind::WrongType,
            format!(
                "`{}` takes a {expected:?}, not a {:?}",
                spec.name,
                value.param_type()
            ),
        )
        .expected(format!("{expected:?}").to_lowercase()));
    }
    match (value, spec.range) {
        (ParamValue::Number(n), Some((lo, hi))) if !(lo..=hi).contains(&n) => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("{n} is outside {lo}..={hi}"),
        )
        .expected(format!("a number from {lo} to {hi}"))),
        (ParamValue::Choice(i), _) if usize::from(i) >= spec.choices.len() => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("choice {i} of {}", spec.choices.len()),
        )
        .expected(format!("one of {}", spec.choices.join(", ")))),
        _ => Ok(value),
    }
}

/// What an effect may draw with: the render context, inside the isolated
/// layer it applies to, and the size of the frame.
pub struct EffectContext<'a> {
    pub(crate) ctx: &'a mut RenderContext,
    pub(crate) width: u16,
    pub(crate) height: u16,
    /// The layer's box on the frame's grid, when its content has one.
    pub(crate) bounds: Option<BezPath>,
}

impl EffectContext<'_> {
    /// Paints `color` over the whole frame, combined with the layer's content
    /// by `compose`. Leaves the context's transform, paint and blend mode as
    /// it found them.
    pub fn cover(&mut self, color: Color, compose: Compose) {
        let frame = Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height));
        self.paint(color, compose, |ctx| ctx.fill_rect(&frame));
    }

    /// Paints `color` over the layer's box, or the whole frame for a layer
    /// without one, as [`EffectContext::cover`] does.
    pub fn cover_box(&mut self, color: Color, compose: Compose) {
        match self.bounds.take() {
            Some(bounds) => {
                self.paint(color, compose, |ctx| ctx.fill_path(&bounds));
                self.bounds = Some(bounds);
            }
            None => self.cover(color, compose),
        }
    }

    /// Runs `fill` on the grid with `color` as the paint, combined by
    /// `compose`, then puts back the transform, paint and blend mode.
    fn paint(&mut self, color: Color, compose: Compose, fill: impl FnOnce(&mut RenderContext)) {
        let ctx = &mut *self.ctx;
        let (transform, paint, blend) = (*ctx.transform(), ctx.paint().clone(), ctx.blend_mode());
        ctx.set_transform(Affine::IDENTITY);
        ctx.set_paint(color.to_vello());
        ctx.set_blend_mode(BlendMode {
            mix: Mix::Normal,
            compose,
        });
        fill(ctx);
        ctx.set_transform(transform);
        ctx.set_paint(paint);
        ctx.set_blend_mode(blend);
    }
}

/// The `fill` effect: the content's colour moves towards `color` by
/// `opacity`, and its alpha stays.
///
/// Painting the colour with alpha `opacity` "source-atop" the content gives,
/// per premultiplied pixel, `opacity * color * a + (1 - opacity) * content`,
/// with alpha `a` unchanged: exactly that mix, done by the rasteriser's own
/// compositing rather than a pass over the pixels.
///
/// Inverted, the colour takes `1 - a` as its alpha, and the content moves
/// towards it by `opacity`: painting it with alpha `opacity` "xor" the
/// content, over the layer's box, gives `opacity * color * (1 - a) +
/// (1 - opacity) * content`, alpha `opacity * (1 - a) + (1 - opacity) * a`,
/// that mix again.
fn fill(ctx: &mut EffectContext<'_>, values: &[ParamValue]) {
    let [
        ParamValue::Color(color),
        ParamValue::Number(opacity),
        ParamValue::Choice(invert),
    ] = values
    else {
        unreachable!("`Effect::new` checks the fill parameters");
    };
    match invert {
        0 => ctx.cover(color.with_opacity(*opacity), Compose::SrcAtop),
        _ => ctx.cover_box(color.with_opacity(*opacity), Compose::Xor),
    }
}

/// The `fill` effect on pixels: the colour, at its alpha times `opacity`,
/// laid "source-atop" each pixel, or "xor" each pixel of the layer's box
/// when inverted, as [`fill`] paints it.
fn fill_pixels(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let [
        ParamValue::Color(color),
        ParamValue::Number(opacity),
        ParamValue::Choice(invert),
    ] = values
    else {
        unreachable!("`Effect::new` checks the fill parameters");
    };
    let share = color.a * opacity;
    let rgb = [color.r, color.g, color.b];
    if *invert == 0 {
        for pixel in pixels.data_mut() {
            let alpha = pixel[3];
            for c in 0..3 {
                pixel[c] = rgb[c] * share * alpha + pixel[c] * (1.0 - share);
            }
        }
        return;
    }
    let inside = box_test(space);
    let width = usize::from(pixels.width());
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        if !inside(i % width, i / width) {
            continue;
        }
        let clear = 1.0 - pixel[3];
        for c in 0..3 {
            pixel[c] = rgb[c] * share * clear + pixel[c] * (1.0 - share);
        }
        pixel[3] = share * clear + pixel[3] * (1.0 - share);
    }
}

/// Whether the centre of pixel `(x, y)` lies in the layer's box; every
/// pixel does for a layer without one.
fn box_test(space: &Space) -> impl Fn(usize, usize) -> bool {
    let back = space.transform.inverse();
    let bounds = space.bounds;
    move |x, y| bounds.is_none_or(|b| b.contains(back * Point::new(x as f64 + 0.5, y as f64 + 0.5)))
}

/// The `transform` effect: the content's anchor goes to `position`, scaled
/// and turned about it, as a shape group's transform places its items.
fn transform(values: &[ParamValue]) -> Move {
    let [
        ParamValue::Point(position),
        ParamValue::Point(anchor),
        ParamValue::Point(scale),
        ParamValue::Number(rotate),
        ParamValue::Number(opacity),
    ] = values
    else {
        unreachable!("`Effect::new` checks the transform parameters");
    };
    let point = |[x, y]: [f32; 2]| Vec2::new(f64::from(x), f64::from(y));
    Move {
        transform: Affine::translate(point(*position))
            * Affine::rotate(f64::from(*rotate).to_radians())
            * Affine::scale_non_uniform(f64::from(scale[0]), f64::from(scale[1]))
            * Affine::translate(-point(*anchor)),
        opacity: *opacity,
    }
}

/// The reach of an effect that changes pixels only where they are.
fn stays(_: &[ParamValue]) -> Reach {
    Reach::Within(0.0)
}

/// The reach of an effect that draws and reads within the layer's box.
fn in_box(_: &[ParamValue]) -> Reach {
    Reach::Bounds
}

/// The reach of an effect that may draw anywhere.
fn anywhere(_: &[ParamValue]) -> Reach {
    Reach::Anywhere
}

/// The number in `values[i]`.
fn number_at(values: &[ParamValue], i: usize) -> f32 {
    match values[i] {
        ParamValue::Number(n) => n,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// How far a Gaussian of standard deviation `sigma` reaches: four of
/// them, where less than a 30,000th of it is left.
fn gaussian_span(sigma: f32) -> f32 {
    4.0 * sigma
}

/// `displacement-map` moves a pixel at most its larger distance; wrapping
/// reads across the whole box.
fn displacement_reach(values: &[ParamValue]) -> Reach {
    match values[5] {
        ParamValue::Choice(1) => Reach::Bounds,
        _ => Reach::Within(number_at(values, 0).abs().max(number_at(values, 1).abs())),
    }
}

/// `turbulent-displace` moves a pixel up to `amount`/100 of `size` along
/// each axis; bulges and twists follow the noise's slope, which may be
/// steeper, so twice that.
fn turbulent_reach(values: &[ParamValue]) -> Reach {
    Reach::Within(2.0 * (number_at(values, 0) / 100.0 * number_at(values, 1)).abs())
}

/// `repeat-tile` grows the box by how far its tiles reach each way.
fn repeat_tile_reach(values: &[ParamValue]) -> Reach {
    let [right, left, down, up] = std::array::from_fn(|i| number_at(values, i));
    Reach::Grows([left, up, right, down])
}

/// `gaussian-blur` spreads by its Gaussian.
fn gaussian_reach(values: &[ParamValue]) -> Reach {
    Reach::Within(gaussian_span(
        number_at(values, 0) * blur::SIGMA_PER_BLURRINESS,
    ))
}

/// `box-blur` spreads by its radius and the pixel beyond it, once per
/// pass.
fn box_reach(values: &[ParamValue]) -> Reach {
    Reach::Within((number_at(values, 0) + 1.0) * number_at(values, 1).round().max(1.0))
}

/// `drop-shadow` falls `distance` away and spreads by its softness.
fn shadow_reach(values: &[ParamValue]) -> Reach {
    Reach::Within(
        number_at(values, 3) + gaussian_span(number_at(values, 4) * blur::SIGMA_PER_BLURRINESS),
    )
}

/// `glow` spreads by its radius, as a blurriness.
fn glow_reach(values: &[ParamValue]) -> Reach {
    Reach::Within(gaussian_span(
        number_at(values, 1) * blur::SIGMA_PER_BLURRINESS,
    ))
}

#[cfg(test)]
#[path = "../tests/unit/effect.rs"]
mod tests;
