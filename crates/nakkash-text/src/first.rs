//! Where box text's first baseline sits: the height of the face's
//! lowercase `d` below the box's top, as the original sets it
//! ([evidence](https://github.com/lefinehq/nakkash/blob/main/docs/design/aep-evidence.md#box-texts-first-baseline)).
//! The `d` is what a Latin face's lowercase ascenders reach, so the first
//! line's ascenders touch the top while its capitals and accents may rise
//! above it. A face whose `d` is missing, or too short to be an ascender
//! (a symbol face's), sits a font ascent down instead.

use kurbo::{BezPath, Shape};
use skrifa::instance::Size;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

use crate::font::Face;

/// The shortest `d`, as a share of the font's ascent, still taken as a
/// lowercase ascender. A Latin `d` reaches about 0.7 of an em against an
/// ascent of about 0.9 to 1.1; a symbol face's `d` may be far shorter, and
/// would put the line at the box's top.
const SHORTEST_D: f64 = 0.3;

/// The tallest `d` taken, in ems: a taller one is a broken outline.
const TALLEST_D: f64 = 4.0;

/// How far below a box's top `face` puts the first baseline, in ems of the
/// size it is set at: the top of its `d`'s outline at the face's position
/// on its axes, in whole thousandths of an em, else its font ascent.
pub(crate) fn below_top(face: &Face) -> f64 {
    let Ok(font) = FontRef::from_index(face.data.data.data(), face.data.index) else {
        return 0.0;
    };
    let location = font.axes().location(face.variations.iter().copied());
    let metrics = font.metrics(Size::unscaled(), &location);
    let em = f64::from(metrics.units_per_em.max(1));
    let ascent = f64::from(metrics.ascent) / em;
    let d = font
        .charmap()
        .map('d')
        .and_then(|id| font.outline_glyphs().get(id))
        .and_then(|glyph| {
            let mut pen = Path(BezPath::new());
            glyph
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), &location),
                    &mut pen,
                )
                .ok()?;
            let top = pen.0.bounding_box().y1;
            Some((top / em * 1000.0).round() / 1000.0)
        });
    match d {
        Some(d) if is_ascender(d, ascent) => d,
        _ => ascent,
    }
}

/// Whether a `d` `d` ems tall, in a face whose ascent is `ascent` ems, is a
/// lowercase ascender: above zero, under [`TALLEST_D`], and at least
/// [`SHORTEST_D`] of an ascent that can be read.
pub(crate) fn is_ascender(d: f64, ascent: f64) -> bool {
    let readable = ascent.is_finite() && ascent > 0.0;
    d.is_finite() && d > 0.0 && d < TALLEST_D && (!readable || d >= SHORTEST_D * ascent)
}

/// A pen that keeps the outline as a path, whose curves bound it exactly.
struct Path(BezPath);

impl OutlinePen for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn close(&mut self) {
        self.0.close_path();
    }
}

#[cfg(test)]
#[path = "../tests/unit/first.rs"]
mod tests;
