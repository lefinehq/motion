//! What a node of the display list draws, as a 128-bit identity ([D90]).
//!
//! Two nodes with the same identity draw the same pixels at the same
//! transform, on the same grid, with the same options; so a frame draws
//! each identity once and hands out copies, whichever node asked. The
//! identity covers everything a node holds:
//!
//! - every field, numbers by their bits, so `0.0` and `-0.0`, or two NaNs
//!   with other payloads, are different identities (a miss, never a wrong
//!   hit);
//! - pictures by a hash of their pixels, made once per picture; video
//!   frames are pictures;
//! - fonts by their data's id, which no other font data ever has, and the
//!   face's index;
//! - children, effect inputs and matte sources by their own identities.
//!
//! The marks of shared children ([`Shared`](crate::Shared)) are left out:
//! they say nothing of what is drawn. Nothing is keyed by where a node sits
//! in memory: an address is used only to find, within one frame whose
//! nodes are all borrowed and alive, an identity already worked out.
//!
//! Each type is hashed by destructuring it whole, so a field added to the
//! display list does not compile until its identity covers it.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::collections::HashMap;
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher13};
use vello_cpu::kurbo::{Affine, BezPath, PathEl, Point, Rect};

use crate::homography::Homography;
use crate::list::{
    Clip, GlyphDraw, ImageDraw, Layer, LayerMask, Matte, Node, PathDraw, Projection,
};
use crate::style::{Color, DrawStyle, Gradient, GradientShape, Paint, StrokeStyle};

/// A node's identity: equal for nodes that draw the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Id(pub(crate) u128);

/// Hashes the parts of a node into its identity: SipHash 1-3 with 128
/// bits out and a fixed key, so an identity is the same in every run and
/// on every machine.
pub(crate) struct Hasher(SipHasher13);

impl std::hash::Hasher for Hasher {
    fn finish(&self) -> u64 {
        self.0.finish()
    }

    fn write(&mut self, bytes: &[u8]) {
        std::hash::Hasher::write(&mut self.0, bytes);
    }
}

impl Hasher {
    /// A hasher for a part of kind `tag`, so parts of different kinds that
    /// hold the same numbers have different identities.
    pub(crate) fn new(tag: &str) -> Hasher {
        let mut hasher = Hasher(SipHasher13::new_with_keys(
            0x6e61_6b6b_6173_6800,
            0x6964_656e_7469_7479,
        ));
        hasher.text(tag);
        hasher
    }

    /// The identity of what was written.
    pub(crate) fn id(&self) -> Id {
        Id(self.0.finish128().as_u128())
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        std::hash::Hasher::write(&mut self.0, bytes);
    }

    pub(crate) fn text(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    pub(crate) fn u8(&mut self, n: u8) {
        std::hash::Hasher::write_u8(&mut self.0, n);
    }

    pub(crate) fn u32(&mut self, n: u32) {
        std::hash::Hasher::write_u32(&mut self.0, n);
    }

    pub(crate) fn u64(&mut self, n: u64) {
        std::hash::Hasher::write_u64(&mut self.0, n);
    }

    pub(crate) fn i16(&mut self, n: i16) {
        std::hash::Hasher::write_i16(&mut self.0, n);
    }

    pub(crate) fn bool(&mut self, b: bool) {
        self.u8(u8::from(b));
    }

    pub(crate) fn f32(&mut self, x: f32) {
        self.u32(x.to_bits());
    }

    pub(crate) fn f64(&mut self, x: f64) {
        self.u64(x.to_bits());
    }

    pub(crate) fn id_of(&mut self, id: Id) {
        std::hash::Hasher::write_u128(&mut self.0, id.0);
    }

    /// Whether a part is there, then the part.
    pub(crate) fn option<T>(&mut self, part: Option<&T>, each: impl FnOnce(&mut Hasher, &T)) {
        self.bool(part.is_some());
        if let Some(part) = part {
            each(self, part);
        }
    }

    /// How many parts, then each.
    pub(crate) fn all<T>(&mut self, parts: &[T], mut each: impl FnMut(&mut Hasher, &T)) {
        self.u64(parts.len() as u64);
        for part in parts {
            each(self, part);
        }
    }

    /// An enum without fields, by its own `Hash`.
    pub(crate) fn plain(&mut self, value: impl Hash) {
        value.hash(self);
    }

    pub(crate) fn point(&mut self, p: Point) {
        let Point { x, y } = p;
        self.f64(x);
        self.f64(y);
    }

    pub(crate) fn rect(&mut self, r: Rect) {
        let Rect { x0, y0, x1, y1 } = r;
        for v in [x0, y0, x1, y1] {
            self.f64(v);
        }
    }

    pub(crate) fn affine(&mut self, a: Affine) {
        for v in a.as_coeffs() {
            self.f64(v);
        }
    }

    pub(crate) fn path(&mut self, path: &BezPath) {
        self.u64(path.elements().len() as u64);
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    self.u8(0);
                    self.point(p);
                }
                PathEl::LineTo(p) => {
                    self.u8(1);
                    self.point(p);
                }
                PathEl::QuadTo(a, b) => {
                    self.u8(2);
                    self.point(a);
                    self.point(b);
                }
                PathEl::CurveTo(a, b, c) => {
                    self.u8(3);
                    self.point(a);
                    self.point(b);
                    self.point(c);
                }
                PathEl::ClosePath => self.u8(4),
            }
        }
    }

    pub(crate) fn color(&mut self, color: Color) {
        let Color { r, g, b, a } = color;
        for v in [r, g, b, a] {
            self.f32(v);
        }
    }

    pub(crate) fn homography(&mut self, map: &Homography) {
        for row in map.rows() {
            for v in row {
                self.f64(v);
            }
        }
    }

    fn paint(&mut self, paint: &Paint) {
        match paint {
            Paint::Solid(color) => {
                self.u8(0);
                self.color(*color);
            }
            Paint::Gradient(gradient) => {
                self.u8(1);
                let Gradient {
                    shape,
                    stops,
                    transform,
                } = &**gradient;
                match *shape {
                    GradientShape::Linear { start, end } => {
                        self.u8(0);
                        self.point(start);
                        self.point(end);
                    }
                    GradientShape::Radial {
                        start,
                        start_radius,
                        end,
                        end_radius,
                    } => {
                        self.u8(1);
                        self.point(start);
                        self.f32(start_radius);
                        self.point(end);
                        self.f32(end_radius);
                    }
                }
                self.all(stops, |h, stop| {
                    let crate::style::Stop { offset, color } = *stop;
                    h.f32(offset);
                    h.color(color);
                });
                self.affine(*transform);
            }
        }
    }

    fn style(&mut self, style: &DrawStyle) {
        match style {
            DrawStyle::Fill(rule) => {
                self.u8(0);
                self.plain(rule);
            }
            DrawStyle::Stroke(stroke) => {
                self.u8(1);
                let StrokeStyle {
                    width,
                    cap,
                    join,
                    miter_limit,
                    dashes,
                } = stroke;
                self.f64(*width);
                self.plain(cap);
                self.plain(join);
                self.f64(*miter_limit);
                self.option(dashes.as_ref(), |h, dashes| {
                    let crate::style::Dashes { offset, pattern } = dashes;
                    h.f64(*offset);
                    h.all(pattern, |h, v| h.f64(*v));
                });
            }
        }
    }
}

/// The identities of one frame's nodes, worked out once each.
///
/// Within the frame every node is borrowed and alive, so a node's address
/// finds its identity without hashing it again; the map is dropped with
/// the frame and never outlives the nodes it was made from.
#[derive(Debug, Default)]
pub(crate) struct Identities {
    nodes: HashMap<usize, Id>,
}

impl Identities {
    /// The identity of the list `nodes`, drawn together back to front.
    pub(crate) fn list(&mut self, nodes: &[Node]) -> Id {
        let ids: Vec<Id> = nodes.iter().map(|node| self.node(node)).collect();
        let mut h = Hasher::new("list");
        h.all(&ids, |h, id| h.id_of(*id));
        h.id()
    }

    /// The identity of `node`, worked out bottom-up and kept for the frame.
    pub(crate) fn node(&mut self, node: &Node) -> Id {
        let address = std::ptr::from_ref(node) as usize;
        if let Some(&id) = self.nodes.get(&address) {
            return id;
        }
        let id = self.made(node);
        self.nodes.insert(address, id);
        id
    }

    fn made(&mut self, node: &Node) -> Id {
        match node {
            Node::Layer(layer) => self.layer(layer),
            Node::Path(path) => {
                let PathDraw { path, paint, style } = path;
                let mut h = Hasher::new("path");
                h.path(path);
                h.paint(paint);
                h.style(style);
                h.id()
            }
            Node::Glyphs(glyphs) => {
                let GlyphDraw {
                    font,
                    size,
                    glyphs,
                    glyph_transform,
                    coords,
                    paint,
                    style,
                } = glyphs;
                let mut h = Hasher::new("glyphs");
                // The font's data by its id, which no other data has.
                h.u64(font.data.id());
                h.u32(font.index);
                h.f32(*size);
                h.all(glyphs, |h, glyph| {
                    let crate::list::Glyph { id, x, y } = *glyph;
                    h.u32(id);
                    h.f32(x);
                    h.f32(y);
                });
                h.option(glyph_transform.as_ref(), |h, t| h.affine(*t));
                h.all(coords, |h, c| h.i16(*c));
                h.paint(paint);
                h.style(style);
                h.id()
            }
            Node::Image(image) => {
                let ImageDraw { picture, rect } = image;
                let mut h = Hasher::new("image");
                h.id_of(picture.content_id());
                h.rect(*rect);
                h.id()
            }
        }
    }

    fn layer(&mut self, layer: &Layer) -> Id {
        let Layer {
            transform,
            opacity,
            blend,
            clip,
            masks,
            effects,
            children,
            isolate,
            matte,
            adjust,
            bounds,
            time,
            motion,
            projection,
            // A mark of sharing, not of content.
            shared: _,
        } = layer;
        // The parts that are nodes first, each to its own identity.
        let children = self.list(children);
        let inputs: Vec<Option<Id>> = effects
            .iter()
            .map(|effect| effect.input().map(|input| self.node(input)))
            .collect();
        let matte = matte
            .as_ref()
            .map(|Matte { source, mode }| (self.node(source), *mode));
        let mut h = Hasher::new("layer");
        h.affine(*transform);
        h.f32(*opacity);
        h.plain(blend);
        h.option(clip.as_ref(), |h, clip| match clip {
            Clip::Rect(rect) => {
                h.u8(0);
                h.rect(*rect);
            }
            Clip::Path(path, rule) => {
                h.u8(1);
                h.path(path);
                h.plain(rule);
            }
        });
        h.all(masks, |h, mask| {
            let LayerMask {
                path,
                mode,
                inverted,
                opacity,
                feather,
                expand,
            } = mask;
            h.path(path);
            h.plain(mode);
            h.bool(*inverted);
            h.f32(*opacity);
            h.f32(feather[0]);
            h.f32(feather[1]);
            h.f32(*expand);
        });
        h.u64(effects.len() as u64);
        for (effect, input) in effects.iter().zip(&inputs) {
            effect.ident(&mut h, *input);
        }
        h.id_of(children);
        h.bool(*isolate);
        h.option(matte.as_ref(), |h, (source, mode)| {
            h.id_of(*source);
            h.plain(mode);
        });
        h.bool(*adjust);
        h.option(bounds.as_ref(), |h, b| h.rect(*b));
        h.f64(*time);
        h.all(motion, |h, m| h.affine(*m));
        h.option(projection.as_ref(), |h, projection| {
            let Projection { matrix, samples } = projection;
            h.homography(matrix);
            h.all(samples, |h, s| h.homography(s));
        });
        h.id()
    }
}

#[cfg(test)]
#[path = "../tests/unit/identity.rs"]
mod tests;
