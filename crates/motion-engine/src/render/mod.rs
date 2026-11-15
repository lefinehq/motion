//! From an evaluated frame to the display list `motion-raster` draws
//! ([D34]): the one place that knows both scenes and pixels, so the scene
//! crates never touch pixels and the rasteriser never sees a scene.
//!
//! Every visible node becomes one layer under its canvas transform, with its
//! opacity and effects; what it draws becomes the layer's leaves, and an
//! image layer's leaf is its picture, which the caller decodes. A nested
//! composition is a layer clipped to the composition's canvas and isolated,
//! holding the composition's background and its own nodes at the time the
//! scene evaluated it. A collapsed one is neither clipped nor given its
//! background: its nodes draw straight into the canvas it sits in, under its
//! transform, and its effects run over that canvas in its coordinates.
//!
//! Frames blend gamma-encoded values, as the rasteriser does: the scene
//! format has no colour-space key yet, and linear-light blending ([D21]) is
//! refused by the rasteriser until it can draw it.
//!
//! [D21]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d21-linear-light-float-compositing
//! [D34]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

mod effect;
mod mask;
mod shape;
mod text;

use motion_format::key::{IMAGE, VIDEO};
use motion_format::property::Prop;
use motion_format::shorthand::{MatteMode, RectShape};
use motion_format::{ImageContent, Place, Result};
use motion_raster::kurbo::{Affine, Rect, RoundedRect, Shape};
use motion_raster::{
    Blending, Clip, Color, DrawStyle, FillRule, Frame, ImageDraw, Layer, Matte,
    MatteMode as RasterMatte, Node, Paint, PathDraw, Picture,
};
use motion_scene::{CompGraph, ContentState, FrameState, Node as GraphNode, NodeState, SceneGraph};

/// How closely curves are flattened into the outlines this module builds
/// from shapes, such as a rounded rectangle's corners, in pixels.
const TOLERANCE: f64 = 0.01;

/// Finds the picture an image layer's `image` names.
pub(crate) type Pictures<'a> = dyn Fn(&str) -> Result<Picture> + 'a;

/// Finds the frame a video layer's `video` shows at a time in seconds, or
/// `None` outside the clip.
pub(crate) type Clips<'a> = dyn Fn(&str, f64) -> Result<Option<Picture>> + 'a;

/// What drawing one frame reads besides the frame's state: the compiled
/// scene, and the pictures and clips its image and video layers name.
#[derive(Clone, Copy)]
pub(crate) struct Sources<'a> {
    /// The compiled scene.
    pub(crate) graph: &'a SceneGraph,
    /// The pictures of image layers.
    pub(crate) pictures: &'a Pictures<'a>,
    /// The frames of video layers.
    pub(crate) clips: &'a Clips<'a>,
}

/// The display list of `state`, one frame of `sources.graph`'s scene, drawn
/// over the root composition's background (a clear one draws nothing), with
/// the image layers' pictures and the video layers' frames from `sources`.
///
/// # Errors
/// A diagnostic for an effect the rasteriser has no row for, or a parameter
/// it rejects, at the effect's place in the layer's `fx`; or the one
/// `sources` returns, at the layer's `image` or `video`.
pub(crate) fn display_list(sources: Sources<'_>, state: &FrameState) -> Result<Frame> {
    let graph = sources.graph;
    let background = graph.root.background;
    Ok(Frame {
        background: opaque_enough(background).then(|| color(background)),
        blending: Blending::Gamma,
        nodes: nodes(sources, &graph.root.nodes, canvas(&graph.root), state)?,
    })
}

/// The layers of one canvas's nodes, back to front: `list` is the canvas's
/// compiled nodes, `area` the canvas, `state` their state at this frame.
fn nodes(
    sources: Sources<'_>,
    list: &[GraphNode],
    area: Rect,
    state: &FrameState,
) -> Result<Vec<Node>> {
    (0..state.nodes.len())
        .filter(|&i| state.nodes[i].visible())
        .filter_map(|i| matted(sources, list, area, state, i).transpose())
        .collect()
}

/// The canvas of `comp`, from its top left.
fn canvas(comp: &CompGraph) -> Rect {
    Rect::new(0.0, 0.0, comp.width, comp.height)
}

/// The layer of node `i` of the canvas `area`, with its matte if it has
/// one, or `None` when it draws nothing.
fn matted(
    sources: Sources<'_>,
    list: &[GraphNode],
    area: Rect,
    state: &FrameState,
    i: usize,
) -> Result<Option<Node>> {
    let Some(Node::Layer(mut drawn)) = layer(sources, &list[i], area, &state.nodes[i])? else {
        return Ok(None);
    };
    // An adjusting layer applies its effects to what is below it, where it
    // draws.
    drawn.adjust |= list[i].adjust;
    // An effect that reads another layer reads its drawing in its own
    // coordinates, with its effects but not its transform, opacity or
    // matte; nothing outside its span.
    for (effect, input) in drawn.effects.iter_mut().zip(&list[i].inputs) {
        let Some(source) = *input else {
            continue;
        };
        let read = match state.nodes[source].shown {
            true => layer(sources, &list[source], area, &state.nodes[source])?,
            false => None,
        };
        let read = match read {
            Some(Node::Layer(read)) => Layer {
                transform: Affine::IDENTITY,
                opacity: 1.0,
                ..read
            },
            _ => Layer::new(Vec::new()),
        };
        *effect = effect.clone().with_input(Node::Layer(read));
    }
    let Some((source, mode)) = list[i].matte else {
        return Ok(Some(Node::Layer(drawn)));
    };
    // The matte layer draws into the mask whether or not it is hidden, and
    // nothing outside its span; it may be matted in turn.
    let matte = &state.nodes[source];
    let source = match matte.shown {
        true => matted(sources, list, area, state, source)?,
        false => None,
    };
    Ok(Some(Node::Layer(Layer {
        matte: Some(Matte {
            source: Box::new(source.unwrap_or_else(|| Node::Layer(Layer::new(Vec::new())))),
            mode: match mode {
                MatteMode::Alpha => RasterMatte::Alpha,
                MatteMode::AlphaInverted => RasterMatte::AlphaInverted,
                MatteMode::Luma => RasterMatte::Luma,
                MatteMode::LumaInverted => RasterMatte::LumaInverted,
            },
        }),
        ..drawn
    })))
}

/// The layer of one node of the canvas `area`, compiled as `compiled`,
/// without its matte, or `None` when it draws nothing.
fn layer(
    sources: Sources<'_>,
    compiled: &GraphNode,
    area: Rect,
    node: &NodeState,
) -> Result<Option<Node>> {
    let opacity = node.number(Prop::OPACITY) as f32;
    if opacity <= 0.0 {
        return Ok(None);
    }
    let mut clip = None;
    let mut isolate = false;
    let mut bounds = None;
    let children = match &node.content {
        ContentState::Rect(shape) => {
            bounds = Some(Rect::new(0.0, 0.0, shape.width, shape.height));
            vec![rect(shape, node.fill)]
        }
        ContentState::Shapes(draws) => shape::draws(draws),
        ContentState::Text { text, glyphs } => text::draws(text, glyphs),
        ContentState::Image(content) => {
            bounds = Some(content.frame);
            vec![image(content, sources.pictures).map_err(|e| e.under(Place::of_layer(&node.id)))?]
        }
        ContentState::Video { video, seconds } => {
            bounds = Some(video.frame);
            let frame = (sources.clips)(&video.path, *seconds)
                .map_err(|e| e.at(VIDEO).under(Place::of_layer(&node.id)))?;
            frame
                .map(|picture| {
                    Node::Image(ImageDraw {
                        picture,
                        rect: video.frame,
                    })
                })
                .into_iter()
                .collect()
        }
        ContentState::Comp(index, Some(inner)) if compiled.collapse => {
            let comp = &sources.graph.comps[*index];
            let children = nodes(sources, &comp.nodes, canvas(comp), inner)?;
            if children.is_empty() {
                return Ok(None);
            }
            let effects =
                effect::effects(&node.fx).map_err(|e| e.under(Place::of_layer(&node.id)))?;
            // Its own blend modes reach what is below unless its effects
            // need its drawing on its own, or an adjusting layer inside
            // needs only its composition below it.
            let isolate = !effects.is_empty() || comp.nodes.iter().any(|n| n.adjust);
            return Ok(Some(Node::Layer(Layer {
                opacity: opacity.min(1.0),
                effects,
                isolate,
                bounds: Some(area),
                time: node.seconds,
                ..Layer::new(moved(node.transform, children))
            })));
        }
        ContentState::Comp(index, Some(inner)) => {
            let comp = &sources.graph.comps[*index];
            let canvas = canvas(comp);
            clip = Some(Clip::Rect(canvas));
            bounds = Some(canvas);
            isolate = true;
            let mut children = Vec::new();
            if opaque_enough(comp.background) {
                children.push(fill(canvas.to_path(TOLERANCE), color(comp.background)));
            }
            children.extend(nodes(sources, &comp.nodes, canvas, inner)?);
            children
        }
        ContentState::Comp(_, None) | ContentState::Pivot => Vec::new(),
        ContentState::Filter => {
            let effects =
                effect::effects(&node.fx).map_err(|e| e.under(Place::of_layer(&node.id)))?;
            return Ok(Some(Node::Layer(Layer {
                transform: node.transform,
                opacity: opacity.min(1.0),
                masks: mask::masks(&node.mask),
                effects,
                adjust: true,
                time: node.seconds,
                ..Layer::new(Vec::new())
            })));
        }
    };
    if children.is_empty() {
        // Effects change what the layer drew, so without content there is
        // nothing for them to change.
        return Ok(None);
    }
    let effects = effect::effects(&node.fx).map_err(|e| e.under(Place::of_layer(&node.id)))?;
    Ok(Some(Node::Layer(Layer {
        transform: node.transform,
        opacity: opacity.min(1.0),
        clip,
        masks: mask::masks(&node.mask),
        effects,
        isolate,
        bounds,
        time: node.seconds,
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

/// An image node: its picture stretched over its box. The picture's errors
/// are placed at `image`, and name the file.
fn image(content: &ImageContent, pictures: &Pictures<'_>) -> Result<Node> {
    let picture = pictures(&content.path).map_err(|e| e.at(IMAGE))?;
    Ok(Node::Image(ImageDraw {
        picture,
        rect: content.frame,
    }))
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
