//! The canvas a vector reader draws on: the drawing model every reader
//! shares, whatever its file format, and the one place that turns it into
//! the rasteriser's display list.
//!
//! A reader walks its file's drawing operators with its own graphics state
//! and calls the canvas for what is painted: [`Canvas::fill`],
//! [`Canvas::stroke`] and [`Canvas::picture`] paint, [`Canvas::push_clip`]
//! and [`Canvas::push_group`] open a clip or a transparency group that
//! later paints land in, and [`Canvas::pop`] closes the last one opened.
//! What a reader cannot draw it names with [`Canvas::unsupported`], so the
//! import reports it. Every coordinate is in the drawing's own space: the
//! page's units (points for PDF), y down, origin at the page's top-left
//! corner.

use std::collections::BTreeSet;

use nakkash_raster::kurbo::{Affine, BezPath, Rect};
use nakkash_raster::{
    BlendMode, Clip, DrawStyle, FillRule, ImageDraw, Layer, Matte, Node, Paint, PathDraw, Picture,
    StrokeStyle,
};

/// A transparency group: what is painted while it is open is composited on
/// its own, then laid on what is below with these.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// How opaque the whole group is, 0 to 1.
    pub opacity: f32,
    /// How it combines with what is below it.
    pub blend: BlendMode,
    /// A soft mask: a drawing, in the drawing's own space, whose alpha or
    /// brightness says how much of the group shows at each point.
    pub mask: Option<Matte>,
}

impl Group {
    /// A group that only isolates what is painted in it.
    pub fn new() -> Group {
        Group {
            opacity: 1.0,
            blend: BlendMode::Normal,
            mask: None,
        }
    }
}

impl Default for Group {
    fn default() -> Group {
        Group::new()
    }
}

/// What a vector reader paints, as display-list nodes in the drawing's own
/// space, with what it could not draw.
#[derive(Debug, Default)]
pub struct Canvas {
    /// The clips and groups open, innermost last, each with the nodes
    /// painted in it so far; the first is the drawing itself.
    open: Vec<Open>,
    /// What the reader could not draw, as reports name it.
    unsupported: BTreeSet<String>,
}

/// A clip or a group being painted into.
#[derive(Debug)]
struct Open {
    /// What closes it.
    kind: Opened,
    /// What is painted in it, back to front.
    nodes: Vec<Node>,
}

/// What an [`Open`] is.
#[derive(Debug)]
enum Opened {
    /// The drawing itself, which never closes.
    Root,
    /// A clip: nothing painted in it shows outside it.
    Clip(Clip),
    /// A transparency group.
    Group(Group),
}

impl Canvas {
    /// An empty canvas.
    pub fn new() -> Canvas {
        Canvas {
            open: vec![Open {
                kind: Opened::Root,
                nodes: Vec::new(),
            }],
            unsupported: BTreeSet::new(),
        }
    }

    /// Fills `path`, given in coordinates `transform` maps to the drawing's,
    /// with `paint`, placed in the drawing's coordinates.
    pub fn fill(&mut self, path: &BezPath, transform: Affine, paint: Paint, rule: FillRule) {
        self.add(Node::Path(PathDraw {
            path: transform * path.clone(),
            paint,
            style: DrawStyle::Fill(rule),
        }));
    }

    /// Strokes `path` with `style`, both in coordinates `transform` maps
    /// to the drawing's: the line's width and dashes are measured there, so
    /// a squashing transform squashes the line too. `paint` is placed in the
    /// drawing's coordinates.
    pub fn stroke(&mut self, path: &BezPath, transform: Affine, paint: Paint, style: &StrokeStyle) {
        // A transform that scales evenly keeps a line a line: the outline and
        // its measures are moved into the drawing's space as they are.
        if let Some(scale) = even_scale(transform) {
            let mut style = style.clone();
            style.width *= scale;
            if let Some(dashes) = &mut style.dashes {
                dashes.offset *= scale;
                dashes
                    .pattern
                    .iter_mut()
                    .for_each(|length| *length *= scale);
            }
            return self.add(Node::Path(PathDraw {
                path: transform * path.clone(),
                paint,
                style: DrawStyle::Stroke(style),
            }));
        }
        // Otherwise it is stroked where it is given and the result moved.
        let paint = match paint {
            Paint::Gradient(mut gradient) => {
                gradient.transform = transform.inverse() * gradient.transform;
                Paint::Gradient(gradient)
            }
            solid => solid,
        };
        self.add(Node::Layer(Layer {
            transform,
            ..Layer::new(vec![Node::Path(PathDraw {
                path: path.clone(),
                paint,
                style: DrawStyle::Stroke(style.clone()),
            })])
        }));
    }

    /// Paints `picture` with its pixels' rectangle, from (0, 0) to its
    /// width and height in pixels with its first row at the top, mapped to
    /// the drawing by `transform`.
    pub fn picture(&mut self, picture: Picture, transform: Affine) {
        let rect = Rect::new(
            0.0,
            0.0,
            f64::from(picture.width()),
            f64::from(picture.height()),
        );
        self.add(Node::Layer(Layer {
            transform,
            ..Layer::new(vec![Node::Image(ImageDraw { picture, rect })])
        }));
    }

    /// Paints `node`, built in the drawing's coordinates, as it is: for what
    /// the other paints do not cover, such as a pattern's cells.
    pub fn node(&mut self, node: Node) {
        self.add(node);
    }

    /// Opens a clip: until it is popped, nothing painted shows outside
    /// `path`, given in the drawing's coordinates, as `rule` fills it.
    pub fn push_clip(&mut self, path: BezPath, rule: FillRule) {
        self.open.push(Open {
            kind: Opened::Clip(Clip::Path(path, rule)),
            nodes: Vec::new(),
        });
    }

    /// Opens a transparency group: until it is popped, what is painted is
    /// composited on its own, then laid on what is below as `group` says.
    pub fn push_group(&mut self, group: Group) {
        self.open.push(Open {
            kind: Opened::Group(group),
            nodes: Vec::new(),
        });
    }

    /// Closes the clip or group opened last. `false`, changing nothing,
    /// when none is open.
    pub fn pop(&mut self) -> bool {
        if self.open.len() < 2 {
            return false;
        }
        let Some(Open { kind, nodes }) = self.open.pop() else {
            return false;
        };
        if let Some(node) = closed(kind, nodes) {
            self.add(node);
        }
        true
    }

    /// How many clips and groups are open.
    pub fn depth(&self) -> usize {
        self.open.len() - 1
    }

    /// Notes something the reader found and could not draw, as the
    /// import's report says it (`text in a font the file does not embed`).
    /// Each is noted once, however often it is met.
    pub fn unsupported(&mut self, what: impl Into<String>) {
        self.unsupported.insert(what.into());
    }

    /// What was painted, back to front, every clip and group still open
    /// closed, and what could not be drawn, sorted.
    pub fn finish(mut self) -> (Vec<Node>, Vec<String>) {
        while self.pop() {}
        let nodes = self.open.pop().map(|open| open.nodes).unwrap_or_default();
        (nodes, self.unsupported.into_iter().collect())
    }

    /// Adds `node` to the clip or group open innermost.
    fn add(&mut self, node: Node) {
        if let Some(open) = self.open.last_mut() {
            open.nodes.push(node);
        }
    }
}

/// The node a closed clip or group of `nodes` becomes; `None` when nothing
/// was painted in it.
fn closed(kind: Opened, nodes: Vec<Node>) -> Option<Node> {
    if nodes.is_empty() {
        return None;
    }
    Some(Node::Layer(match kind {
        Opened::Root => Layer::new(nodes),
        Opened::Clip(clip) => Layer {
            clip: Some(clip),
            ..Layer::new(nodes)
        },
        Opened::Group(group) => {
            // A group that changes nothing about how it lands, and holds
            // nothing that blends with what is below, is only its nodes.
            let plain = group.opacity >= 1.0
                && group.blend == BlendMode::Normal
                && group.mask.is_none()
                && !blends(&nodes);
            if plain {
                return Some(Node::Layer(Layer::new(nodes)));
            }
            Layer {
                opacity: group.opacity.clamp(0.0, 1.0),
                blend: group.blend,
                matte: group.mask,
                isolate: true,
                ..Layer::new(nodes)
            }
        }
    }))
}

/// Whether any of `nodes`, or the nodes of layers among them that do not
/// isolate them, blends other than normally.
fn blends(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| match node {
        Node::Layer(layer) => {
            layer.blend != BlendMode::Normal || (!layer.isolate && blends(&layer.children))
        }
        _ => false,
    })
}

/// How much `transform` scales every direction by, when it scales all of
/// them alike (turns and mirrors allowed); `None` when it stretches one
/// more than another.
fn even_scale(transform: Affine) -> Option<f64> {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let (x, y) = (a.hypot(b), c.hypot(d));
    let square = (a * c + b * d).abs() <= 1e-9 * x.max(y).max(1.0);
    ((x - y).abs() <= 1e-9 * x.max(y).max(1.0) && square).then_some(x)
}

#[cfg(test)]
#[path = "../tests/unit/canvas.rs"]
mod tests;
