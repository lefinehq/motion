//! From an evaluated frame to the display list `motion-raster` draws
//! ([D34]): the one place that knows both scenes and pixels, so the scene
//! crates never touch pixels and the rasteriser never sees a scene.
//!
//! Every visible node becomes one layer under its canvas transform, with its
//! opacity and effects; what it draws becomes the layer's leaves. A nested
//! composition is a layer clipped to the composition's canvas and isolated,
//! holding the composition's background and its own nodes at the time the
//! scene evaluated it.
//!
//! Frames blend gamma-encoded values, as the rasteriser does: the scene
//! format has no colour-space key yet, and linear-light blending ([D21]) is
//! refused by the rasteriser until it can draw it.
//!
//! [D21]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d21-linear-light-float-compositing
//! [D34]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

mod effect;
mod shape;
mod text;

use motion_format::Result;
use motion_format::property::Prop;
use motion_format::shorthand::RectShape;
use motion_raster::kurbo::{Affine, Rect, RoundedRect, Shape};
use motion_raster::{
    Blending, Clip, Color, DrawStyle, FillRule, Frame, Layer, Node, Paint, PathDraw,
};
use motion_scene::{ContentState, FrameState, NodeState, SceneGraph};

/// How closely curves are flattened into the outlines this module builds
/// from shapes, such as a rounded rectangle's corners, in pixels.
const TOLERANCE: f64 = 0.01;

/// The display list of `state`, one frame of `graph`'s scene, drawn over
/// `background` (straight RGBA from 0 to 1; a clear one draws nothing).
///
/// # Errors
/// A diagnostic for an effect the rasteriser has no row for, or a parameter
/// it rejects, at the effect's place in the layer's `fx`.
pub(crate) fn display_list(
    graph: &SceneGraph,
    background: [f64; 4],
    state: &FrameState,
) -> Result<Frame> {
    Ok(Frame {
        background: opaque_enough(background).then(|| color(background)),
        blending: Blending::Gamma,
        nodes: nodes(graph, state)?,
    })
}

/// The layers of one canvas's nodes, back to front.
fn nodes(graph: &SceneGraph, state: &FrameState) -> Result<Vec<Node>> {
    state
        .nodes
        .iter()
        .filter(|node| node.visible())
        .filter_map(|node| layer(graph, node).transpose())
        .collect()
}

/// The layer of one visible node, or `None` when it draws nothing.
fn layer(graph: &SceneGraph, node: &NodeState) -> Result<Option<Node>> {
    let opacity = node.number(Prop::OPACITY) as f32;
    if opacity <= 0.0 {
        return Ok(None);
    }
    let mut clip = None;
    let mut isolate = false;
    let children = match &node.content {
        ContentState::Rect(shape) => vec![rect(shape, node.fill)],
        ContentState::Shapes(draws) => shape::draws(draws),
        ContentState::Text { text, glyphs } => text::draws(text, glyphs),
        ContentState::Comp(index, Some(inner)) => {
            let comp = &graph.comps[*index];
            let canvas = Rect::new(0.0, 0.0, comp.width, comp.height);
            clip = Some(Clip::Rect(canvas));
            isolate = true;
            let mut children = Vec::new();
            if opaque_enough(comp.background) {
                children.push(fill(canvas.to_path(TOLERANCE), color(comp.background)));
            }
            children.extend(nodes(graph, inner)?);
            children
        }
        ContentState::Comp(_, None) | ContentState::Pivot => Vec::new(),
    };
    if children.is_empty() {
        // Effects change what the layer drew, so without content there is
        // nothing for them to change.
        return Ok(None);
    }
    let effects = effect::effects(&node.fx).map_err(|e| e.under(node.id.as_str()))?;
    Ok(Some(Node::Layer(Layer {
        transform: node.transform,
        opacity: opacity.min(1.0),
        clip,
        effects,
        isolate,
        ..Layer::new(children)
    })))
}

/// A rect node: its box, with rounded corners, filled with its colour.
fn rect(shape: &RectShape, rgba: [f64; 4]) -> Node {
    let (width, height) = (shape.width, shape.height);
    // Corners larger than half the shorter side would overlap; they meet instead.
    let radius = shape.radius.clamp(0.0, width.min(height) / 2.0);
    let outline = RoundedRect::new(0.0, 0.0, width, height, radius).to_path(TOLERANCE);
    fill(outline, color(rgba))
}

/// `path` filled with `color` by the non-zero rule.
fn fill(path: motion_raster::kurbo::BezPath, color: Color) -> Node {
    Node::Path(PathDraw {
        path,
        paint: Paint::Solid(color),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// A scene colour, straight RGBA from 0 to 1, as the rasteriser's.
fn color([r, g, b, a]: [f64; 4]) -> Color {
    Color::rgba(r as f32, g as f32, b as f32, a as f32)
}

/// Whether a colour leaves any mark: its alpha is above zero.
fn opaque_enough(rgba: [f64; 4]) -> bool {
    rgba[3] > 0.0
}

/// Wraps `children` in a layer that only moves them, or returns them as they
/// are under the identity.
fn moved(transform: Affine, children: Vec<Node>) -> Vec<Node> {
    if transform == Affine::IDENTITY {
        return children;
    }
    vec![Node::Layer(Layer {
        transform,
        ..Layer::new(children)
    })]
}

#[cfg(test)]
#[path = "../../tests/unit/render/mod.rs"]
mod tests;
