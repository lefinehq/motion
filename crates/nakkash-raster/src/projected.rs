//! Projected layers ([D58]): a plane drawn flat in its own coordinates,
//! into a buffer of its own, then laid onto the canvas through a
//! projective map.
//!
//! - **The region** of the plane the buffer holds is the part the canvas
//!   sees, the frame's inverse image in front of the camera, cut to the
//!   box the layer draws in, effects included ([`Reach`]), and widened by
//!   how far its effects read.
//! - **The scale** of the buffer is the most the map magnifies that
//!   region, at most [`MAX_SCALE`], lowered so neither side passes
//!   [`MAX_SIDE`].
//! - **Laying** maps each pixel of the canvas back onto the plane. A pixel
//!   whose ray meets the plane behind the camera is left clear, so a plane
//!   crossing the camera is cut at it with no clipping of outlines. The
//!   others are sampled from a chain of halved copies of the buffer, by
//!   the footprint the pixel has on the plane: trilinearly, with up to
//!   [`MAX_ANISOTROPY`] probes along its long axis, so a plane seen nearly
//!   edge-on neither aliases nor blurs across.
//!
//! [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera

use std::borrow::Cow;
use std::cell::OnceCell;

use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use vello_cpu::Pixmap;
use vello_cpu::kurbo::{Affine, Point, Rect, Shape};

use crate::effect::{Effect, Reach};
use crate::homography::Homography;
use crate::list::{Clip, GlyphDraw, Layer, LayerMask, Node};
use crate::pixels::{Pixel, Pixels};
use crate::style::DrawStyle;

/// The most a projected layer's buffer magnifies its own units.
pub(crate) const MAX_SCALE: f64 = 4.0;

/// The longest side of a projected layer's buffer, in px.
pub(crate) const MAX_SIDE: f64 = 8192.0;

/// The clear px kept round a buffer, so its content's edges fade out
/// inside it as they are sampled, and kernels a px or two wide on its grid
/// (`sharpen`) have room.
const BORDER: f64 = 4.0;

/// How far past the frame the seen part of a plane is taken, in px of the
/// canvas: the reach of sampling at the frame's edges.
const MARGIN: f64 = 2.0;

/// The most probes along a footprint's long axis.
pub(crate) const MAX_ANISOTROPY: usize = 16;

/// Where the seen part of a plane that draws everywhere ends towards the
/// horizon: where a unit of it covers less than this much of the canvas,
/// in px². Beyond, 64 by 64 units fall within one px.
const FAR_AREA: f64 = 1.0 / 4096.0;

/// How far out a polygon is started for a plane that draws everywhere,
/// in its units, before the frame's inverse image cuts it.
const UNBOUNDED: f64 = 1e9;

/// Where a layer draws, in its own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Extent {
    /// Nowhere.
    Nothing,
    /// Within a box.
    Box(Rect),
    /// Anywhere on its plane.
    Everywhere,
}

impl Extent {
    /// Where either draws.
    fn union(self, other: Extent) -> Extent {
        match (self, other) {
            (Extent::Nothing, e) | (e, Extent::Nothing) => e,
            (Extent::Box(a), Extent::Box(b)) => Extent::Box(a.union(b)),
            _ => Extent::Everywhere,
        }
    }

    /// Where it draws within `rect`.
    fn within(self, rect: Rect) -> Extent {
        match self {
            Extent::Nothing => Extent::Nothing,
            Extent::Box(b) => {
                let cut = b.intersect(rect);
                match cut.width() >= 0.0 && cut.height() >= 0.0 {
                    true => Extent::Box(cut),
                    false => Extent::Nothing,
                }
            }
            Extent::Everywhere => Extent::Box(rect),
        }
    }

    /// Where it draws once moved by `affine`.
    fn moved(self, affine: Affine) -> Extent {
        match self {
            Extent::Box(b) => Extent::Box(affine.transform_rect_bbox(b)),
            e => e,
        }
    }

    /// Where it draws in the coordinates `map` takes it to: the box of its
    /// corners when all are in front, everywhere when the plane crosses
    /// the camera.
    fn projected(self, map: &Homography) -> Extent {
        let Extent::Box(b) = self else {
            return self;
        };
        let corners = [(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)]
            .map(|(x, y)| map.project(Point::new(x, y)));
        match corners {
            [Some(a), Some(b), Some(c), Some(d)] => {
                Extent::Box(Rect::from_points(a, b).union_pt(c).union_pt(d))
            }
            _ => Extent::Everywhere,
        }
    }
}

/// Where `layer` draws in its own coordinates: its box, or its children's,
/// grown and moved by its effects in order and cut to its clip, as its
/// effects grow it.
pub(crate) fn extent(layer: &Layer) -> Extent {
    let content = match layer.bounds {
        Some(bounds) => Extent::Box(bounds),
        None => nodes_extent(&layer.children),
    };
    let drawn = after_effects(content, &layer.effects, layer.bounds);
    match layer.shown_clip() {
        Some(clip) => drawn.within(clip_box(&clip)),
        None => drawn,
    }
}

/// Where `extent` draws once `effects` have run on it, for a layer whose
/// box is `bounds`; an effect that grows the box leaves the grown box to
/// the effects after it.
fn after_effects(extent: Extent, effects: &[Effect], bounds: Option<Rect>) -> Extent {
    let (drawn, _) = effects
        .iter()
        .fold((extent, bounds), |(drawn, bounds), effect| {
            if let Some(movement) = effect.movement() {
                return (drawn.moved(movement.transform), bounds);
            }
            let grown = bounds.map(|b| effect.grown(b));
            let drawn = match effect.reach() {
                Reach::Within(by) => match drawn {
                    Extent::Box(b) => Extent::Box(b.inflate(f64::from(by), f64::from(by))),
                    e => e,
                },
                Reach::Bounds | Reach::Grows(_) => match grown {
                    Some(grown) => drawn.union(Extent::Box(grown)),
                    None => Extent::Everywhere,
                },
                Reach::Anywhere => Extent::Everywhere,
            };
            (drawn, grown)
        });
    drawn
}

/// The box of a clip's outline.
fn clip_box(clip: &Clip) -> Rect {
    match clip {
        Clip::Rect(rect) => *rect,
        Clip::Path(path, _) => path.bounding_box(),
    }
}

/// Where `nodes` draw in their parent's coordinates. A filter layer among
/// them grows what the nodes before it drew, as its effects do.
pub(crate) fn nodes_extent(nodes: &[Node]) -> Extent {
    nodes
        .iter()
        .fold(Extent::Nothing, |drawn, node| match node {
            Node::Layer(layer) if layer.adjust => {
                after_effects(drawn, &layer.effects, layer.bounds).union(drawn)
            }
            Node::Layer(layer) => {
                let own = extent(layer);
                let placed = match &layer.projection {
                    Some(projection) => std::iter::once(&projection.matrix)
                        .chain(&projection.samples)
                        .fold(Extent::Nothing, |all, map| all.union(own.projected(map))),
                    None => {
                        let moves = std::iter::once(&Affine::IDENTITY).chain(&layer.motion);
                        moves.fold(Extent::Nothing, |all, step| {
                            all.union(own.moved(*step * layer.transform))
                        })
                    }
                };
                drawn.union(placed)
            }
            Node::Path(draw) => {
                let reach = match &draw.style {
                    DrawStyle::Fill(_) => 0.0,
                    DrawStyle::Stroke(stroke) => stroke_reach(stroke.width, stroke.miter_limit),
                };
                drawn.union(Extent::Box(draw.path.bounding_box().inflate(reach, reach)))
            }
            Node::Glyphs(draw) => drawn.union(glyphs_extent(draw)),
            Node::Image(draw) => drawn.union(Extent::Box(draw.rect)),
        })
}

/// How far a stroke of `width` reaches past its outline: half its width,
/// as far as a miter join may reach or a square cap's corner does.
fn stroke_reach(width: f64, miter_limit: f64) -> f64 {
    width.abs() / 2.0 * miter_limit.max(std::f64::consts::SQRT_2)
}

/// Where a run of glyphs draws: each glyph's outline box, at the run's
/// size and the font's position on its axes, placed as the rasteriser
/// places it. Everywhere when the font cannot be read.
fn glyphs_extent(draw: &GlyphDraw) -> Extent {
    let Ok(font) = FontRef::from_index(draw.font.data.data(), draw.font.index) else {
        return Extent::Everywhere;
    };
    let coords: Vec<NormalizedCoord> = draw
        .coords
        .iter()
        .map(|&c| NormalizedCoord::from_bits(c))
        .collect();
    let metrics = font.glyph_metrics(Size::new(draw.size), LocationRef::new(&coords));
    // Outlines are y-up; the glyph's own transform turns them about its origin.
    let own = draw.glyph_transform.unwrap_or(Affine::IDENTITY) * Affine::FLIP_Y;
    let reach = match &draw.style {
        DrawStyle::Fill(_) => 0.0,
        DrawStyle::Stroke(stroke) => stroke_reach(stroke.width, stroke.miter_limit),
    };
    draw.glyphs.iter().fold(Extent::Nothing, |drawn, glyph| {
        match metrics.bounds(GlyphId::new(glyph.id)) {
            Some(b) => {
                let outline = Rect::new(
                    f64::from(b.x_min),
                    f64::from(b.y_min),
                    f64::from(b.x_max),
                    f64::from(b.y_max),
                );
                let placed = Affine::translate((f64::from(glyph.x), f64::from(glyph.y))) * own;
                let at = placed.transform_rect_bbox(outline).inflate(reach, reach);
                drawn.union(Extent::Box(at))
            }
            // An empty glyph, such as a space, draws nothing.
            None => drawn,
        }
    })
}

/// How far the drawing of `layer` reads from each point it shows, in its
/// own units, as its masks and effects run on its buffer: `None` when
/// some effect reads anywhere in its box.
pub(crate) fn read_reach(layer: &Layer) -> Option<f64> {
    let masks = mask_reach(&layer.masks);
    // Moves before the first effect that reads pixels apply as the
    // content is drawn, and read nothing.
    let mut on_pixels = layer.effects.iter().skip_while(|e| e.movement().is_some());
    on_pixels.try_fold(masks, |sum, effect| match effect.movement() {
        Some(_) => None,
        None => match effect.reach() {
            Reach::Within(by) => Some(sum + f64::from(by)),
            Reach::Bounds | Reach::Grows(_) | Reach::Anywhere => None,
        },
    })
}

/// How far `masks` read from each point they cut, in the layer's units:
/// the most any feathers or moves its edge.
pub(crate) fn mask_reach(masks: &[LayerMask]) -> f64 {
    masks
        .iter()
        .map(|m| f64::from(m.feather[0].max(m.feather[1]) + m.expand.abs()))
        .fold(0.0, f64::max)
}

/// Where and how big a projected layer's buffer is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Plan {
    /// The buffer's width in px.
    pub(crate) width: u16,
    /// The buffer's height in px.
    pub(crate) height: u16,
    /// From the layer's own coordinates to px of the buffer.
    pub(crate) local: Affine,
    /// The part of the layer's plane the buffer holds, in its own
    /// coordinates, without the buffer's clear border.
    pub(crate) region: Rect,
    /// For each map, the px of the canvas its seen part lands on, as
    /// `[x0, y0, x1, y1)`; `None` where it lands on none.
    landed: Vec<Option<[usize; 4]>>,
    /// A point of the layer's region the canvas sees under the first map,
    /// the middle of what it sees.
    pub(crate) seen: Point,
}

impl Plan {
    /// The buffer of a layer that draws within `extent` and reads `reach`
    /// from each point it shows (`None`: anywhere in `extent`), laid
    /// through each of `maps`, from its own coordinates to the px of a
    /// `width` by `height` canvas; `None` when none of it can show.
    pub(crate) fn new(
        extent: Extent,
        reach: Option<f64>,
        maps: &[Homography],
        width: u16,
        height: u16,
    ) -> Option<Plan> {
        let start = match extent {
            Extent::Nothing => return None,
            Extent::Box(b) => b,
            Extent::Everywhere => Rect::new(-UNBOUNDED, -UNBOUNDED, UNBOUNDED, UNBOUNDED),
        };
        let start = vec![
            Point::new(start.x0, start.y0),
            Point::new(start.x1, start.y0),
            Point::new(start.x1, start.y1),
            Point::new(start.x0, start.y1),
        ];
        let (w, h) = (f64::from(width), f64::from(height));
        let seen: Vec<Vec<Point>> = maps
            .iter()
            .map(|map| {
                let mut polygon = seen_part(start.clone(), map, w, h);
                if extent == Extent::Everywhere {
                    polygon = towards_horizon(polygon, map);
                }
                polygon
            })
            .collect();
        let seen_box = seen
            .iter()
            .filter(|p| !p.is_empty())
            .map(|p| polygon_box(p))
            .reduce(|a, b| a.union(b))?;
        let region = match (reach, extent) {
            (Some(reach), Extent::Box(b)) => seen_box.inflate(reach, reach).intersect(b),
            (Some(reach), _) => seen_box.inflate(reach, reach),
            (None, Extent::Box(b)) => b,
            (None, _) => seen_box,
        };
        if !(region.width() > 0.0 && region.height() > 0.0) {
            return None;
        }
        let most = maps
            .iter()
            .zip(&seen)
            .flat_map(|(map, polygon)| probes(polygon).map(|p| map.magnification(p)))
            .fold(0.0, f64::max);
        let room = MAX_SIDE - 2.0 * BORDER - 1.0;
        let scale = most
            .min(MAX_SCALE)
            .min(room / region.width())
            .min(room / region.height());
        if !(scale.is_finite() && scale > 0.0) {
            return None;
        }
        // The region starts on a whole buffer px, so a map that only moves
        // by whole px lays the buffer's px exactly on the canvas's.
        let snap = |v: f64| (v * scale).floor() / scale;
        let region = Rect::new(snap(region.x0), snap(region.y0), region.x1, region.y1);
        let side = |length: f64| (length * scale + 2.0 * BORDER).ceil() as u16;
        let local = Affine::translate((BORDER, BORDER))
            * Affine::scale(scale)
            * Affine::translate((-region.x0, -region.y0));
        let landed = maps
            .iter()
            .zip(&seen)
            .map(|(map, polygon)| landed(polygon, map, width, height))
            .collect();
        let first = seen.iter().find(|p| !p.is_empty())?;
        let middle = first.iter().fold(Point::ZERO, |sum, p| sum + p.to_vec2());
        Some(Plan {
            width: side(region.width()),
            height: side(region.height()),
            local,
            region,
            landed,
            seen: Point::new(middle.x / first.len() as f64, middle.y / first.len() as f64),
        })
    }
}

/// The part of the convex `polygon` of a plane that `map` shows in a
/// `width` by `height` frame, widened by [`MARGIN`]: the points in front
/// of the camera whose image lies in the frame. Each bound is linear in
/// the plane's coordinates once multiplied by `w`, so it is a half-plane.
fn seen_part(polygon: Vec<Point>, map: &Homography, width: f64, height: f64) -> Vec<Point> {
    let [x, y, w] = map.rows();
    let combine = |a: [f64; 3], k: f64, b: [f64; 3]| std::array::from_fn(|i| a[i] + k * b[i]);
    let negate = |a: [f64; 3]| a.map(|v| -v);
    let sides = [
        // In front.
        w,
        // x ≥ −margin·w, x ≤ (width + margin)·w, and the same for y.
        combine(x, MARGIN, w),
        combine(negate(x), width + MARGIN, w),
        combine(y, MARGIN, w),
        combine(negate(y), height + MARGIN, w),
    ];
    sides
        .into_iter()
        .fold(polygon, |polygon, side| clipped(&polygon, side))
}

/// The seen part of a plane that draws everywhere, cut where a unit of it
/// covers less than [`FAR_AREA`] of the canvas: where `w` passes the depth
/// at which `|det J| = |det M| / w³` falls to it.
fn towards_horizon(polygon: Vec<Point>, map: &Homography) -> Vec<Point> {
    let far = (map.determinant().abs() / FAR_AREA).cbrt();
    let [_, _, w] = map.rows();
    clipped(&polygon, [-w[0], -w[1], far - w[2]])
}

/// The part of the convex `polygon` where `side · (x, y, 1) ≥ 0`.
fn clipped(polygon: &[Point], side: [f64; 3]) -> Vec<Point> {
    let at = |p: Point| side[0] * p.x + side[1] * p.y + side[2];
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (i, &p) in polygon.iter().enumerate() {
        let q = polygon[(i + 1) % polygon.len()];
        let (a, b) = (at(p), at(q));
        if a >= 0.0 {
            out.push(p);
        }
        if (a >= 0.0) != (b >= 0.0) {
            out.push(p.lerp(q, a / (a - b)));
        }
    }
    out
}

/// The box of a polygon's points.
fn polygon_box(polygon: &[Point]) -> Rect {
    polygon
        .iter()
        .fold(Rect::from_points(polygon[0], polygon[0]), |b, &p| {
            b.union_pt(p)
        })
}

/// Where a polygon's magnification is read: its corners, the middles of
/// its sides and its centre.
fn probes(polygon: &[Point]) -> impl Iterator<Item = Point> + '_ {
    let n = polygon.len();
    let centre = match n {
        0 => None,
        _ => {
            let sum = polygon.iter().fold(Point::ZERO, |s, p| s + p.to_vec2());
            Some(Point::new(sum.x / n as f64, sum.y / n as f64))
        }
    };
    polygon
        .iter()
        .enumerate()
        .flat_map(move |(i, &p)| [p, p.midpoint(polygon[(i + 1) % n])])
        .chain(centre)
}

/// The px of a `width` by `height` canvas the seen `polygon` lands on
/// through `map`, a px wider each way for sampling's reach.
fn landed(polygon: &[Point], map: &Homography, width: u16, height: u16) -> Option<[usize; 4]> {
    let corners: Vec<Point> = polygon.iter().filter_map(|&p| map.project(p)).collect();
    if corners.is_empty() {
        return None;
    }
    let b = polygon_box(&corners).inflate(1.0, 1.0);
    let x = |v: f64| v.clamp(0.0, f64::from(width)) as usize;
    let y = |v: f64| v.clamp(0.0, f64::from(height)) as usize;
    let px = [
        x(b.x0.floor()),
        y(b.y0.floor()),
        x(b.x1.ceil()),
        y(b.y1.ceil()),
    ];
    (px[0] < px[2] && px[1] < px[3]).then_some(px)
}

/// A drawn buffer's px, premultiplied RGBA at 8 bits a channel, row by
/// row: the buffer as the rasteriser drew it, or one of its halved
/// copies. Eight bits keep the drawing exact and a quarter the size of
/// floats, and laying a plane is bound by reading them.
struct Texture<'a> {
    width: usize,
    height: usize,
    data: Cow<'a, [u8]>,
}

impl Texture<'_> {
    /// The px at `x`, `y`, clear beyond the edges.
    fn get(&self, x: i64, y: i64) -> [u8; 4] {
        match (usize::try_from(x), usize::try_from(y)) {
            (Ok(x), Ok(y)) if x < self.width && y < self.height => {
                let i = (y * self.width + x) * 4;
                [
                    self.data[i],
                    self.data[i + 1],
                    self.data[i + 2],
                    self.data[i + 3],
                ]
            }
            _ => [0; 4],
        }
    }

    /// The colour at `(x, y)`, px centres at half steps, blended from the
    /// four nearest px; clear beyond the edges. As [`Pixels::sample`], in
    /// 0 to 1.
    #[inline(always)]
    fn bilinear(&self, x: f64, y: f64) -> Pixel {
        let (x, y) = (x - 0.5, y - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let inside =
            x0 >= 0.0 && y0 >= 0.0 && x0 + 1.0 < self.width as f64 && y0 + 1.0 < self.height as f64;
        let four: [[u8; 4]; 4] = match inside {
            true => {
                let i = (y0 as usize * self.width + x0 as usize) * 4;
                let j = i + self.width * 4;
                let (top, bottom) = (&self.data[i..i + 8], &self.data[j..j + 8]);
                [
                    [top[0], top[1], top[2], top[3]],
                    [top[4], top[5], top[6], top[7]],
                    [bottom[0], bottom[1], bottom[2], bottom[3]],
                    [bottom[4], bottom[5], bottom[6], bottom[7]],
                ]
            }
            false => {
                let (x0, y0) = (x0 as i64, y0 as i64);
                [
                    self.get(x0, y0),
                    self.get(x0 + 1, y0),
                    self.get(x0, y0 + 1),
                    self.get(x0 + 1, y0 + 1),
                ]
            }
        };
        let k = 1.0 / 255.0;
        let weights = [
            (1.0 - fx) * (1.0 - fy) * k,
            fx * (1.0 - fy) * k,
            (1.0 - fx) * fy * k,
            fx * fy * k,
        ];
        std::array::from_fn(|c| {
            f32::from(four[0][c]) * weights[0]
                + f32::from(four[1][c]) * weights[1]
                + f32::from(four[2][c]) * weights[2]
                + f32::from(four[3][c]) * weights[3]
        })
    }

    /// The texture at half the size each way, rounded up, each px the
    /// mean of the four it covers, clear beyond the edges.
    fn halved(&self) -> Texture<'static> {
        let (hw, hh) = (self.width.div_ceil(2), self.height.div_ceil(2));
        let mut data = vec![0u8; hw * hh * 4];
        let row = |y: usize| match y < self.height {
            true => &self.data[y * self.width * 4..(y + 1) * self.width * 4],
            false => &[][..],
        };
        // A px of a row, as a u32, clear beyond its end.
        let at = |line: &[u8], x: usize| match line.get(x * 4..x * 4 + 4) {
            Some(p) => u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
            None => 0,
        };
        for (r, line) in data.chunks_exact_mut(hw * 4).enumerate() {
            let (top, bottom) = (row(r * 2), row(r * 2 + 1));
            for (col, px) in line.chunks_exact_mut(4).enumerate() {
                let x = col * 2;
                // All four in 16-bit lanes: at most 4·255 each.
                let sum = spread(at(top, x))
                    + spread(at(top, x + 1))
                    + spread(at(bottom, x))
                    + spread(at(bottom, x + 1));
                for (c, v) in px.iter_mut().enumerate() {
                    *v = ((((sum >> (16 * c)) & 0xffff) + 2) / 4) as u8;
                }
            }
        }
        Texture {
            width: hw,
            height: hh,
            data: Cow::Owned(data),
        }
    }
}

/// The four 8-bit channels of a px, `[r, g, b, a]` from the low byte,
/// spread over the four 16-bit lanes of a `u64`.
fn spread(px: u32) -> u64 {
    let p = u64::from(px);
    (p & 0xff) | ((p & 0xff00) << 8) | ((p & 0x00ff_0000) << 16) | ((p & 0xff00_0000) << 24)
}

/// A buffer and its halved copies, each half the size of the one before,
/// down to a px. A copy is made the first time a footprint needs it, so a
/// plane seen at its own size or larger makes none.
struct Chain<'a> {
    base: Texture<'a>,
    halves: Vec<OnceCell<Texture<'static>>>,
}

impl<'a> Chain<'a> {
    /// The chain of the drawn `buffer`.
    fn new(buffer: &'a Pixmap) -> Chain<'a> {
        let base = Texture {
            width: usize::from(buffer.width()),
            height: usize::from(buffer.height()),
            data: Cow::Borrowed(buffer.data_as_u8_slice()),
        };
        let longest = buffer.width().max(buffer.height());
        // Halving `n` times takes the longest side to a px.
        let levels = (u16::BITS - longest.saturating_sub(1).leading_zeros()) as usize;
        Chain {
            base,
            halves: (0..levels).map(|_| OnceCell::new()).collect(),
        }
    }

    /// The copy at `level`, 0 for the buffer itself, at most the last.
    fn level(&self, level: usize) -> &Texture<'_> {
        match level.min(self.halves.len()) {
            0 => &self.base,
            n => self.halves[n - 1].get_or_init(|| self.level(n - 1).halved()),
        }
    }

    /// The colour at buffer point `b` seen over a footprint whose width
    /// in buffer px is `2^octave`: between the two copies whose px are
    /// nearest that size.
    #[inline(always)]
    fn trilinear(&self, b: Point, octave: f64) -> Pixel {
        let octave = octave.max(0.0);
        let below = octave.floor();
        let t = (octave - below) as f32;
        let last = self.halves.len();
        let at = |level: usize| {
            let k = 1.0 / (1u64 << level) as f64;
            self.level(level).bilinear(b.x * k, b.y * k)
        };
        let below = below as usize;
        if below >= last {
            return at(last);
        }
        let lo = at(below);
        if t <= 0.0 {
            return lo;
        }
        let hi = at(below + 1);
        std::array::from_fn(|c| lo[c] + (hi[c] - lo[c]) * t)
    }
}

/// The base-2 logarithm of `x`, which is positive and finite: its
/// exponent, and the mantissa's from a cubic fit to `log2` over one octave,
/// within 0.001 of it and meeting it at both ends. That is far finer than
/// blending two copies needs, and the library's `log2` costs a fifth of
/// the time of laying a px.
fn log2(x: f64) -> f64 {
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as f64 - 1023.0;
    // The mantissa less one, 0 to 1.
    let m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000) - 1.0;
    // log2(1 + m) ≈ m·(a + m·(b + m·c)), with a + b + c = 1.
    let (a, b) = (1.422_9, -0.582_1);
    exponent + m * (a + m * (b + m * (1.0 - a - b)))
}

/// `buffer`, drawn as `plan` says, laid onto a `width` by `height` canvas
/// through each of `maps` and averaged.
pub(crate) fn lay(
    buffer: &Pixmap,
    plan: &Plan,
    maps: &[Homography],
    width: u16,
    height: u16,
) -> Pixels {
    let chain = Chain::new(buffer);
    let mut out = Pixels::transparent(width, height);
    let share = 1.0 / maps.len() as f32;
    for (map, landed) in maps.iter().zip(&plan.landed) {
        let (Some(landed), Some(back)) = (landed, map.inverse()) else {
            continue;
        };
        let to_buffer = Homography::from(plan.local) * back;
        lay_one(&chain, &to_buffer, *landed, share, &mut out);
    }
    out
}

/// Adds `share` of the buffer seen through `to_buffer`, from px of the
/// canvas to px of the buffer, to the px of `out` within `landed`.
fn lay_one(
    chain: &Chain<'_>,
    to_buffer: &Homography,
    [x0, y0, x1, y1]: [usize; 4],
    share: f32,
    out: &mut Pixels,
) {
    let width = usize::from(out.width());
    let data = out.data_mut();
    // In tiles, so the px of the buffer a tile reads stay in cache while
    // it is laid: a row of the canvas crosses many rows of a plane seen in
    // perspective.
    let tiles = (y0..y1)
        .step_by(TILE)
        .flat_map(|ty| (x0..x1).step_by(TILE).map(move |tx| (tx, ty)));
    for (tx, ty) in tiles {
        lay_tile(
            chain,
            to_buffer,
            [tx, ty, (tx + TILE).min(x1), (ty + TILE).min(y1)],
            share,
            width,
            data,
        );
    }
}

/// The side of the tiles a plane is laid in, in px of the canvas.
const TILE: usize = 32;

/// [`lay_one`] over the px `[x0, y0, x1, y1)` of `data`, a canvas `width`
/// px wide.
fn lay_tile(
    chain: &Chain<'_>,
    to_buffer: &Homography,
    [x0, y0, x1, y1]: [usize; 4],
    share: f32,
    width: usize,
    data: &mut [Pixel],
) {
    let [g0, g1, g2] = to_buffer.rows();
    let (bw, bh) = (chain.base.width as f64, chain.base.height as f64);
    for y in y0..y1 {
        let fy = y as f64 + 0.5;
        let fx = x0 as f64 + 0.5;
        // Stepped along the row: each px adds the matrix's first column.
        let mut q = [
            g0[0] * fx + g0[1] * fy + g0[2],
            g1[0] * fx + g1[1] * fy + g1[2],
            g2[0] * fx + g2[1] * fy + g2[2],
        ];
        for x in x0..x1 {
            let [qx, qy, w] = q;
            q = [q[0] + g0[0], q[1] + g1[0], q[2] + g2[0]];
            // The ray meets the plane behind the camera, or at infinity.
            if w <= f64::MIN_POSITIVE {
                continue;
            }
            let inverse = 1.0 / w;
            let b = Point::new(qx * inverse, qy * inverse);
            // The footprint of the px on the buffer, from the Jacobian of
            // the map back.
            let foot = Footprint::new([
                (g0[0] - b.x * g2[0]) * inverse,
                (g0[1] - b.x * g2[1]) * inverse,
                (g1[0] - b.y * g2[0]) * inverse,
                (g1[1] - b.y * g2[1]) * inverse,
            ]);
            let inside = b.x >= 0.0 && b.y >= 0.0 && b.x <= bw && b.y <= bh;
            if !inside {
                let reach = foot.long() + 1.0;
                if !(b.x > -reach && b.y > -reach && b.x < bw + reach && b.y < bh + reach) {
                    continue;
                }
            }
            let pixel = foot.sample(chain, b);
            let total = &mut data[y * width + x];
            for c in 0..4 {
                total[c] += pixel[c] * share;
            }
        }
    }
}

/// The footprint of a px of the canvas on the buffer: the ellipse the
/// Jacobian `[a, b, c, d]` (rows) of the map back makes of the px, whose
/// axes are the square roots of the eigenvalues of `J·Jᵀ`.
struct Footprint {
    jacobian: [f64; 4],
    /// The squared length of its long axis, in buffer px.
    long2: f64,
    /// The squared length of its short axis.
    short2: f64,
}

impl Footprint {
    fn new(jacobian: [f64; 4]) -> Footprint {
        let [a, b, c, d] = jacobian;
        let (p, q, r) = (a * a + b * b, a * c + b * d, c * c + d * d);
        let half = (p + r) / 2.0;
        let root = (((p - r) / 2.0).powi(2) + q * q).sqrt();
        Footprint {
            jacobian,
            long2: half + root,
            short2: (half - root).max(0.0),
        }
    }

    /// The length of its long axis.
    fn long(&self) -> f64 {
        self.long2.sqrt()
    }

    /// The colour over it at buffer point `b`: probes spaced along its
    /// long axis, as many as it is longer than wide (rounded, up to
    /// [`MAX_ANISOTROPY`]), each read trilinearly over the width it
    /// covers. One no wider than a buffer px reads the buffer alone.
    fn sample(&self, chain: &Chain<'_>, b: Point) -> Pixel {
        if self.long2 <= 1.0 {
            return chain.base.bilinear(b.x, b.y);
        }
        // Fewer than 1.5 times as long as wide: one probe, so without
        // a square root.
        let probes = match self.long2 < 2.25 * self.short2 {
            true => 1,
            false => (self.long2 / self.short2.max(f64::MIN_POSITIVE))
                .sqrt()
                .round()
                .min(MAX_ANISOTROPY as f64) as usize,
        };
        let n = probes as f64;
        // log2 of the larger of a probe's share of the long axis and the
        // short axis, from their squares.
        let octave = log2((self.long2 / (n * n)).max(self.short2)) / 2.0;
        if probes == 1 {
            return chain.trilinear(b, octave);
        }
        let long = self.long();
        let axis = long_axis(self.jacobian, self.long2);
        let step = long / n;
        let mut sum = [0.0f32; 4];
        for k in 0..probes {
            let t = (k as f64 + 0.5 - n / 2.0) * step;
            let p = chain.trilinear(b + axis * t, octave);
            for c in 0..4 {
                sum[c] += p[c];
            }
        }
        sum.map(|c| c / probes as f32)
    }
}

/// The unit direction, on the buffer, along which a px's footprint is
/// longest: the eigenvector of `J·Jᵀ` for `long2`, its long axis squared.
fn long_axis([a, b, c, d]: [f64; 4], long2: f64) -> vello_cpu::kurbo::Vec2 {
    use vello_cpu::kurbo::Vec2;
    let (p, q, r) = (a * a + b * b, a * c + b * d, c * c + d * d);
    let l = long2;
    let one = Vec2::new(q, l - p);
    let other = Vec2::new(l - r, q);
    let v = match one.hypot2() >= other.hypot2() {
        true => one,
        false => other,
    };
    match v.hypot() > 0.0 {
        true => v / v.hypot(),
        false if p >= r => Vec2::new(1.0, 0.0),
        false => Vec2::new(0.0, 1.0),
    }
}

#[cfg(test)]
#[path = "../tests/unit/projected.rs"]
mod tests;
