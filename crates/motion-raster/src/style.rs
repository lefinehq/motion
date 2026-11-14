//! How a leaf is painted: its colour ([`Paint`]), and whether its outline is
//! filled or stroked ([`DrawStyle`]). Caps, joins and fill rules are looked up
//! by name in tables, so an importer maps names, never branches on them.

use motion_format::{Diagnostic, Result};
use vello_cpu::kurbo;
use vello_cpu::peniko;

/// A colour in gamma-encoded sRGB, each channel 0 to 1, alpha not premultiplied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha: 0 is transparent, 1 opaque.
    pub a: f32,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
    /// Opaque black.
    pub const BLACK: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);

    /// A colour from its four channels.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    /// The same colour with its alpha multiplied by `opacity`.
    pub fn with_opacity(self, opacity: f32) -> Color {
        Color {
            a: self.a * opacity,
            ..self
        }
    }

    pub(crate) fn to_vello(self) -> peniko::Color {
        peniko::Color::new([self.r, self.g, self.b, self.a])
    }
}

/// What a leaf is painted with.
///
/// Only flat colour exists today; gradients become variants here, and the
/// renderer maps each variant to a vello paint in one place. A picture is a
/// leaf of its own, [`ImageDraw`](crate::ImageDraw), placed by its rectangle.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// One colour everywhere.
    Solid(Color),
}

impl Paint {
    pub(crate) fn to_vello(&self) -> peniko::Color {
        match self {
            Paint::Solid(color) => color.to_vello(),
        }
    }
}

/// Which parts of a self-overlapping outline count as inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    /// Inside where the outline winds around a point a non-zero number of times.
    NonZero,
    /// Inside where it winds an odd number of times.
    EvenOdd,
}

/// The fill rules by name.
pub const FILL_RULES: &[(&str, FillRule, peniko::Fill)] = &[
    ("nonzero", FillRule::NonZero, peniko::Fill::NonZero),
    ("evenodd", FillRule::EvenOdd, peniko::Fill::EvenOdd),
];

/// The shape drawn at the open ends of a stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    /// Flat, ending exactly at the end point.
    Butt,
    /// A half circle around the end point.
    Round,
    /// A half square around the end point.
    Square,
}

/// The caps by name.
pub const CAPS: &[(&str, Cap, kurbo::Cap)] = &[
    ("butt", Cap::Butt, kurbo::Cap::Butt),
    ("round", Cap::Round, kurbo::Cap::Round),
    ("square", Cap::Square, kurbo::Cap::Square),
];

/// The shape drawn where two segments of a stroke meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// A sharp corner, cut off past the miter limit.
    Miter,
    /// A rounded corner.
    Round,
    /// A corner cut straight across.
    Bevel,
}

/// The joins by name.
pub const JOINS: &[(&str, Join, kurbo::Join)] = &[
    ("miter", Join::Miter, kurbo::Join::Miter),
    ("round", Join::Round, kurbo::Join::Round),
    ("bevel", Join::Bevel, kurbo::Join::Bevel),
];

/// Finds the row of `value` in a table of `(name, value, vello value)`.
fn row<'t, T: PartialEq, V>(
    table: &'t [(&'static str, T, V)],
    value: &T,
) -> &'t (&'static str, T, V) {
    table
        .iter()
        .find(|(_, v, _)| v == value)
        .expect("every variant has a row in its table")
}

/// Looks `name` up in a table, or reports it with the names the table holds.
fn named<T: Copy, V>(table: &[(&'static str, T, V)], what: &str, name: &str) -> Result<T> {
    table
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, v, _)| *v)
        .ok_or_else(|| Diagnostic::unknown(what, name, table.iter().map(|(n, _, _)| *n)))
}

impl FillRule {
    /// The rule called `name` in [`FILL_RULES`].
    ///
    /// # Errors
    /// An unknown-name diagnostic listing the rules.
    pub fn named(name: &str) -> Result<FillRule> {
        named(FILL_RULES, "fill rule", name)
    }

    /// The rule's name.
    pub fn name(self) -> &'static str {
        row(FILL_RULES, &self).0
    }

    pub(crate) fn to_vello(self) -> peniko::Fill {
        row(FILL_RULES, &self).2
    }
}

impl Cap {
    /// The cap called `name` in [`CAPS`].
    ///
    /// # Errors
    /// An unknown-name diagnostic listing the caps.
    pub fn named(name: &str) -> Result<Cap> {
        named(CAPS, "cap", name)
    }

    /// The cap's name.
    pub fn name(self) -> &'static str {
        row(CAPS, &self).0
    }

    fn to_vello(self) -> kurbo::Cap {
        row(CAPS, &self).2
    }
}

impl Join {
    /// The join called `name` in [`JOINS`].
    ///
    /// # Errors
    /// An unknown-name diagnostic listing the joins.
    pub fn named(name: &str) -> Result<Join> {
        named(JOINS, "join", name)
    }

    /// The join's name.
    pub fn name(self) -> &'static str {
        row(JOINS, &self).0
    }

    fn to_vello(self) -> kurbo::Join {
        row(JOINS, &self).2
    }
}

/// How an outline is stroked.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    /// The stroke's width in the leaf's units.
    pub width: f64,
    /// The shape of open ends.
    pub cap: Cap,
    /// The shape of corners.
    pub join: Join,
    /// How far a miter corner may reach, in stroke widths, before it is bevelled.
    pub miter_limit: f64,
    /// Dashes, if the stroke is dashed.
    pub dashes: Option<Dashes>,
}

impl StrokeStyle {
    /// A solid stroke of `width` with butt caps and miter joins limited at 4.
    pub fn new(width: f64) -> StrokeStyle {
        StrokeStyle {
            width,
            cap: Cap::Butt,
            join: Join::Miter,
            miter_limit: 4.0,
            dashes: None,
        }
    }

    pub(crate) fn to_vello(&self) -> kurbo::Stroke {
        let stroke = kurbo::Stroke::new(self.width)
            .with_caps(self.cap.to_vello())
            .with_join(self.join.to_vello())
            .with_miter_limit(self.miter_limit);
        match &self.dashes {
            None => stroke,
            Some(dashes) => stroke.with_dashes(dashes.offset, dashes.pattern.iter().copied()),
        }
    }
}

/// A dash pattern: lengths of dash, gap, dash, gap and so on, starting `offset` in.
#[derive(Debug, Clone, PartialEq)]
pub struct Dashes {
    /// How far into the pattern the stroke starts.
    pub offset: f64,
    /// Alternating dash and gap lengths.
    pub pattern: Vec<f64>,
}

/// Whether a leaf's outline is filled or stroked.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawStyle {
    /// Filled, with this rule.
    Fill(FillRule),
    /// Stroked, with this style.
    Stroke(StrokeStyle),
}

#[cfg(test)]
#[path = "../tests/unit/style.rs"]
mod tests;
