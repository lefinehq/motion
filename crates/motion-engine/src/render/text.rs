//! A text layer as glyph runs: each run in its font, with its fill and
//! stroke in its paint order, and under animators each glyph with its own
//! transform, opacity, colours and stroke width.
//!
//! # Paint order and opacity
//!
//! `fill-over` strokes first and fills on top, so the stroke shows only
//! outside the glyph; `stroke-over` fills first. A glyph that fades fades as
//! one shape: when it draws both a fill and a stroke, they are drawn into a
//! buffer of their own that takes the opacity, so the stroke does not show
//! through the fill. A glyph with one paint folds its opacity into that
//! paint's colour, which draws the same pixels without the buffer.
//!
//! Across characters, `paint-across` either keeps each character's own
//! order, so a later character covers an earlier one's stroke and fill, or
//! draws one paint of every glyph before the other paint of any: every
//! stroke, then every fill (`fills-over`), or the reverse. Then each glyph
//! folds its opacity into each paint, as a glyph with one paint does.
//!
//! Glyphs next to each other in a run whose states are equal, such as the
//! glyphs of a line that an animator moves by line, are drawn as one run.
//!
//! # Faux bold and faux italic
//!
//! The scene format describes both as drawing effects on a face that lacks
//! the weight or the slant. Layout gives faux bold room
//! ([`FAUX_BOLD_TRACKING`](motion_text::FAUX_BOLD_TRACKING) after each
//! character, measured against stored layouts); the renderer draws both:
//!
//! - **Faux bold** strokes every glyph in its own fill colour, centred on the
//!   outline and with mitred corners, so the glyph grows by half the stroke
//!   width on every side. The width is a fraction of the size that shrinks as
//!   type grows, as Skia's synthetic bold does: 1/24 of the size up to 9 px,
//!   1/32 from 36 px, linear between ([`bold_width`]). A stroke on the text
//!   widens by the same amount, so it stays the same width outside the
//!   thickened glyph. When the fill is translucent, the fill and the
//!   thickening stroke are drawn into one buffer at full strength that
//!   takes the colour's alpha, so their overlap is not painted twice.
//! - **Faux italic** slants every glyph about its own origin on the baseline
//!   by [`ITALIC_SLANT_DEGREES`], the angle FreeType's synthetic oblique
//!   uses: tops lean right, the baseline stays put.

use motion_format::text::{CharacterStyle, PaintAcross, PaintOrder};
use motion_raster::kurbo::Affine;
use motion_raster::{
    Color, DrawStyle, Effect, FillRule, Glyph, GlyphDraw, Layer, Node, Paint, ParamValue,
    StrokeStyle,
};
use motion_scene::{GlyphState, TextNode};
use motion_text::GlyphRun;

use super::{color, moved, opaque_enough};

/// How far faux italic slants glyphs from upright, in degrees.
pub(crate) const ITALIC_SLANT_DEGREES: f64 = 12.0;

/// The width of the stroke that thickens glyphs of `size` px for faux bold:
/// 1/24 of the size at 9 px and below, 1/32 at 36 px and above, and linear
/// in between, so small type thickens relatively more.
pub(crate) fn bold_width(size: f64) -> f64 {
    const SMALL: (f64, f64) = (9.0, 1.0 / 24.0);
    const LARGE: (f64, f64) = (36.0, 1.0 / 32.0);
    let t = ((size - SMALL.0) / (LARGE.0 - SMALL.0)).clamp(0.0, 1.0);
    size * (SMALL.1 + t * (LARGE.1 - SMALL.1))
}

/// How each glyph of `run` in `style` is drawn about its origin: stretched
/// up by the run's stretch, its vertical scale over its horizontal one (the
/// layout has already set it at its horizontally scaled size), then slanted
/// for faux italic; `None` when neither changes it.
fn glyph_shape(run: &GlyphRun, style: &CharacterStyle) -> Option<Affine> {
    let stretch = (run.stretch != 1.0).then(|| Affine::scale_non_uniform(1.0, run.stretch));
    // Kurbo's skew moves x by `skew_x * y`; y grows downwards, so a negative
    // factor leans the glyphs' tops to the right.
    let slant = style
        .faux_italic
        .then(|| Affine::skew(-ITALIC_SLANT_DEGREES.to_radians().tan(), 0.0));
    match (slant, stretch) {
        (Some(slant), Some(stretch)) => Some(slant * stretch),
        (one, other) => one.or(other),
    }
}

/// Every glyph run of `text`, back to front. `glyphs` holds one state per
/// glyph in the layout's run order, or none when no animator moves them.
pub(super) fn draws(text: &TextNode, glyphs: &[GlyphState]) -> Vec<Node> {
    match text.content.whole.paint_across {
        PaintAcross::Each => pass(text, glyphs, None),
        PaintAcross::FillsOver => [
            pass(text, glyphs, Some(Paints::Stroke)),
            pass(text, glyphs, Some(Paints::Fill)),
        ]
        .concat(),
        PaintAcross::StrokesOver => [
            pass(text, glyphs, Some(Paints::Fill)),
            pass(text, glyphs, Some(Paints::Stroke)),
        ]
        .concat(),
    }
}

/// One of a glyph's two paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Paints {
    /// The fill, with faux bold's thickening.
    Fill,
    /// The stroke.
    Stroke,
}

/// Every glyph run of `text` with only the paint `only`, or both in each
/// glyph's own order.
fn pass(text: &TextNode, glyphs: &[GlyphState], only: Option<Paints>) -> Vec<Node> {
    debug_assert!(
        glyphs.is_empty()
            || glyphs.len()
                == text
                    .layout
                    .runs
                    .iter()
                    .map(|r| r.glyphs.len())
                    .sum::<usize>()
    );
    let mut out = Vec::new();
    let mut first = 0;
    for (r, run) in text.layout.runs.iter().enumerate() {
        let style = text.style_of(r);
        let face = Face::new(run, style, only);
        if glyphs.is_empty() {
            face.draw(&run.glyphs, &rest(style), &mut out);
            continue;
        }
        let states = &glyphs[first..first + run.glyphs.len()];
        first += run.glyphs.len();
        let mut at = 0;
        for same in states.chunk_by(|a, b| a == b) {
            face.draw(&run.glyphs[at..at + same.len()], &same[0], &mut out);
            at += same.len();
        }
    }
    out
}

/// The Gaussian blurs of a glyph blurred `[x, y]` across and down, in the
/// text layer's coordinates, each the `blurriness` of the `gaussian-blur`
/// effect: none, one both ways when the two agree, else one each way. A
/// negative amount blurs nothing.
fn blurs([x, y]: [f64; 2]) -> Vec<Effect> {
    let (x, y) = (x.max(0.0), y.max(0.0));
    let blur = |amount: f64, way: u8| {
        Effect::new(
            "gaussian-blur",
            &[
                ("blurriness", ParamValue::Number(amount as f32)),
                ("dimensions", ParamValue::Choice(way)),
            ],
        )
        .expect("the gaussian blur takes a blurriness and a way")
    };
    match (x > 0.0, y > 0.0) {
        (true, true) if x == y => vec![blur(x, 0)],
        (across, down) => [(across, x, 1), (down, y, 2)]
            .into_iter()
            .filter(|(on, ..)| *on)
            .map(|(_, amount, way)| blur(amount, way))
            .collect(),
    }
}

/// A glyph's state where no animator moves it: as its run's style sets it.
fn rest(style: &CharacterStyle) -> GlyphState {
    GlyphState {
        transform: Affine::IDENTITY,
        opacity: 1.0,
        fill: style.fill,
        stroke: style.stroke,
        stroke_width: style.stroke_width,
        blur: [0.0, 0.0],
    }
}

/// What every glyph of one run shares: its font and how its style draws.
struct Face<'a> {
    run: &'a GlyphRun,
    order: PaintOrder,
    /// The faux bold stroke's width, zero without faux bold.
    bold: f64,
    /// How each glyph is drawn about its origin: stretched up by its run's
    /// vertical scale, then slanted for faux italic; `None` when neither.
    shape: Option<Affine>,
    /// The one paint it draws, or both.
    only: Option<Paints>,
}

/// One paint of a glyph, and the drawing operations it is made of.
struct Ink {
    paint: Paints,
    color: [f64; 4],
    styles: Vec<DrawStyle>,
}

impl<'a> Face<'a> {
    fn new(run: &'a GlyphRun, style: &CharacterStyle, only: Option<Paints>) -> Face<'a> {
        Face {
            run,
            only,
            order: style.paint,
            bold: match style.faux_bold {
                true => bold_width(run.size),
                false => 0.0,
            },
            shape: glyph_shape(run, style),
        }
    }

    /// Draws `glyphs`, which share `state`, into `out`: blurred in a
    /// buffer of their own when the state blurs them.
    fn draw(&self, glyphs: &[motion_text::Glyph], state: &GlyphState, out: &mut Vec<Node>) {
        if state.opacity <= 0.0 || glyphs.is_empty() {
            return;
        }
        let effects = blurs(state.blur);
        if !effects.is_empty() {
            let mut drawn = Vec::new();
            self.draw_sharp(glyphs, state, &mut drawn);
            out.push(Node::Layer(Layer {
                effects,
                ..Layer::new(drawn)
            }));
            return;
        }
        self.draw_sharp(glyphs, state, out);
    }

    /// Draws `glyphs`, which share `state`, into `out`, unblurred.
    fn draw_sharp(&self, glyphs: &[motion_text::Glyph], state: &GlyphState, out: &mut Vec<Node>) {
        let inks = self.inks(state);
        match inks.len() {
            0 => {}
            1 => out.extend(moved(
                state.transform,
                self.ink(&inks[0], glyphs, state.opacity),
            )),
            _ if state.opacity >= 1.0 => {
                let nodes = inks.iter().flat_map(|ink| self.ink(ink, glyphs, 1.0));
                out.extend(moved(state.transform, nodes.collect()));
            }
            // Fill and stroke overlap: each glyph fades as one shape, in a
            // buffer of its own, so neither shows through the other.
            _ => out.extend(glyphs.iter().map(|glyph| {
                let one = std::slice::from_ref(glyph);
                Node::Layer(Layer {
                    transform: state.transform,
                    opacity: state.opacity as f32,
                    ..Layer::new(
                        inks.iter()
                            .flat_map(|ink| self.ink(ink, one, 1.0))
                            .collect(),
                    )
                })
            })),
        }
    }

    /// The paints `state` draws, bottom first; a clear paint is left out.
    fn inks(&self, state: &GlyphState) -> Vec<Ink> {
        let mut fill = vec![DrawStyle::Fill(FillRule::NonZero)];
        if self.bold > 0.0 {
            fill.push(DrawStyle::Stroke(StrokeStyle::new(self.bold)));
        }
        let fill = opaque_enough(state.fill).then_some(Ink {
            paint: Paints::Fill,
            color: state.fill,
            styles: fill,
        });
        let stroke = (opaque_enough(state.stroke) && state.stroke_width > 0.0).then(|| Ink {
            paint: Paints::Stroke,
            color: state.stroke,
            styles: vec![DrawStyle::Stroke(StrokeStyle::new(
                state.stroke_width + self.bold,
            ))],
        });
        let (bottom, top) = match self.order {
            PaintOrder::FillOver => (stroke, fill),
            PaintOrder::StrokeOver => (fill, stroke),
        };
        bottom
            .into_iter()
            .chain(top)
            .filter(|ink| self.only.is_none_or(|only| ink.paint == only))
            .collect()
    }

    /// The leaves of one paint over `glyphs`, at `opacity`.
    fn ink(&self, ink: &Ink, glyphs: &[motion_text::Glyph], opacity: f64) -> Vec<Node> {
        let rgba = color(ink.color).with_opacity(opacity as f32);
        if ink.styles.len() == 1 || rgba.a >= 1.0 {
            return ink
                .styles
                .iter()
                .map(|style| self.leaf(glyphs, Paint::Solid(rgba), style.clone()))
                .collect();
        }
        // The fill and its faux bold stroke overlap along the outline; drawn
        // at full strength into one buffer, the union takes the alpha once.
        let solid = Paint::Solid(Color { a: 1.0, ..rgba });
        vec![Node::Layer(Layer {
            opacity: rgba.a,
            ..Layer::new(
                ink.styles
                    .iter()
                    .map(|style| self.leaf(glyphs, solid.clone(), style.clone()))
                    .collect(),
            )
        })]
    }

    fn leaf(&self, glyphs: &[motion_text::Glyph], paint: Paint, style: DrawStyle) -> Node {
        Node::Glyphs(GlyphDraw {
            font: self.run.font.clone(),
            size: self.run.size as f32,
            glyphs: glyphs
                .iter()
                .map(|g| Glyph {
                    id: g.id,
                    x: g.x as f32,
                    y: g.y as f32,
                })
                .collect(),
            glyph_transform: self.shape,
            coords: self.run.coords.clone(),
            paint,
            style,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/text.rs"]
mod tests;
