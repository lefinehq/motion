//! A shape layer's draws as outlines filled or stroked under each paint's
//! group transform. Paint kinds are joined by name to a table of what each
//! one draws, as geometry kinds are to their outlines in `motion-scene`, and
//! settings such as a fill's `rule` or a stroke's `cap` are looked up by name
//! in the rasteriser's tables, so neither side lists the other's names.

use motion_format::item::props;
use motion_raster::{Cap, Color, DrawStyle, FillRule, Join, Node, Paint, PathDraw, StrokeStyle};
use motion_scene::ShapeDraw;

use super::{color, moved};

/// Draws one paint's outline, or nothing when the paint leaves no mark.
type Painter = fn(&ShapeDraw) -> Option<PathDraw>;

/// What each paint kind draws, by name; every paint kind has a row, which a
/// test checks against `motion_format::item::SHAPES`.
pub(super) const PAINTS: [(&str, Painter); 2] = [("fill", fill), ("stroke", stroke)];

/// Every draw, back to front, each under its group transform.
pub(super) fn draws(draws: &[ShapeDraw]) -> Vec<Node> {
    draws
        .iter()
        .filter_map(|draw| {
            let (_, paint) = PAINTS
                .iter()
                .find(|(name, _)| *name == draw.paint.kind.name)
                .expect("every paint kind has a row");
            let leaf = paint(draw)?;
            // A path is stroked in the space it is drawn in, so a stroke under
            // a non-uniform scale keeps its shape: the transform stays a layer
            // rather than being applied to the outline.
            Some(moved(draw.transform, vec![Node::Path(leaf)]))
        })
        .flatten()
        .collect()
}

/// The paint's colour with its own and its groups' opacity, or `None` when
/// that leaves nothing to see. A paint is one drawing operation, so opacity
/// folds into its colour exactly.
fn ink(draw: &ShapeDraw, rgba: [f64; 4], opacity: f64) -> Option<Paint> {
    let color: Color = color(rgba).with_opacity((opacity * draw.opacity) as f32);
    (color.a > 0.0).then_some(Paint::Solid(color))
}

fn fill(draw: &ShapeDraw) -> Option<PathDraw> {
    let paint = &draw.paint;
    Some(PathDraw {
        path: draw.outline.clone(),
        paint: ink(
            draw,
            paint.color(props::FILL_COLOR),
            paint.number(props::FILL_OPACITY),
        )?,
        style: DrawStyle::Fill(
            FillRule::named(paint.setting("rule")).expect("the scene's rules are the rasteriser's"),
        ),
    })
}

fn stroke(draw: &ShapeDraw) -> Option<PathDraw> {
    let paint = &draw.paint;
    let width = paint.number(props::STROKE_WIDTH);
    if width <= 0.0 {
        return None;
    }
    Some(PathDraw {
        path: draw.outline.clone(),
        paint: ink(
            draw,
            paint.color(props::STROKE_COLOR),
            paint.number(props::STROKE_OPACITY),
        )?,
        style: DrawStyle::Stroke(StrokeStyle {
            cap: Cap::named(paint.setting("cap")).expect("the scene's caps are the rasteriser's"),
            join: Join::named(paint.setting("join"))
                .expect("the scene's joins are the rasteriser's"),
            miter_limit: paint.number(props::STROKE_MITER),
            ..StrokeStyle::new(width)
        }),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/render/shape.rs"]
mod tests;
