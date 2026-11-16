//! A pen for effects that draw lines of their own: strokes in the layer's
//! coordinates, drawn into the frame's grid by their distance to each
//! pixel, faded over a share of their half thickness at their edges,
//! coloured from an inside colour along their middle to an outside colour
//! at their edges, and kept to the layer's box. The sound effects draw
//! their bands and waves with it, the beam its beam.

use vello_cpu::kurbo::{Affine, Point, Rect};

use crate::effect::Space;
use crate::pixels::Pixels;

/// Draws into the frame's grid: strokes and dots in the layer's
/// coordinates, mapped by `transform`, kept to the layer's box.
pub(crate) struct Pen<'a> {
    pixels: &'a mut Pixels,
    transform: Affine,
    back: Affine,
    bounds: Option<Rect>,
    /// How many pixels of the grid a px of the layer is.
    scale: f64,
    /// Half the thickness, in pixels of the grid: where the edge is half
    /// covered.
    half: f64,
    /// The share of the half thickness that fades, 0 to 1.
    softness: f64,
    inside: [f32; 4],
    outside: [f32; 4],
}

impl<'a> Pen<'a> {
    /// A pen over `pixels`, laid out in `space`, drawing lines `thickness`
    /// px of the layer thick, faded over `softness` of their half
    /// thickness, from the first colour along their middle to the second at
    /// their edges.
    pub(crate) fn new(
        pixels: &'a mut Pixels,
        space: &Space,
        thickness: f64,
        softness: f64,
        [inside, outside]: [[f32; 4]; 2],
    ) -> Pen<'a> {
        let scale = space.transform.determinant().abs().sqrt();
        Pen {
            pixels,
            transform: space.transform,
            back: space.transform.inverse(),
            bounds: space.bounds,
            scale,
            half: thickness * scale / 2.0,
            softness: softness.clamp(0.0, 1.0),
            inside,
            outside,
        }
    }

    /// How far an edge `half` pixels out fades, centred on it: the
    /// softness's share of the thickness, and at least a pixel, so edges
    /// are smooth.
    fn fade(&self, half: f64) -> f64 {
        (half * self.softness * 2.0).max(1.0)
    }

    /// The colour `across` of the way from a stroke's middle to its edge,
    /// turned `hue` degrees.
    fn color(&self, across: f64, hue: f64) -> [f32; 4] {
        let t = across.clamp(0.0, 1.0) as f32;
        let mixed: [f32; 4] =
            std::array::from_fn(|i| self.inside[i] + (self.outside[i] - self.inside[i]) * t);
        match hue == 0.0 {
            true => mixed,
            false => turned(mixed, hue),
        }
    }

    /// Lays `color` with `cover` over the grid's pixel `(x, y)`.
    fn lay(&mut self, x: i64, y: i64, color: [f32; 4], cover: f32) {
        if let Some(bounds) = self.bounds {
            let at = self.back * Point::new(x as f64 + 0.5, y as f64 + 0.5);
            if !bounds.contains(at) {
                return;
            }
        }
        let width = i64::from(self.pixels.width());
        let a = color[3] * cover;
        if a <= 0.0 {
            return;
        }
        let pixel = &mut self.pixels.data_mut()[(y * width + x) as usize];
        for c in 0..3 {
            pixel[c] = color[c] * a + pixel[c] * (1.0 - a);
        }
        pixel[3] = a + pixel[3] * (1.0 - a);
    }

    /// A stroke from `from` to `to`, both in the layer's coordinates, with
    /// round ends, or ending flat at them when `square`.
    pub(crate) fn stroke(&mut self, from: Point, to: Point, square: bool, hue: f64) {
        let half = self.half;
        self.tapered(from, to, [half, half], square, hue);
    }

    /// A stroke from `from` to `to` whose thickness runs from `thickness[0]`
    /// px of the layer at `from` to `thickness[1]` at `to`, ending flat at
    /// both.
    pub(crate) fn beam(&mut self, from: Point, to: Point, thickness: [f64; 2]) {
        let halves = thickness.map(|t| t * self.scale / 2.0);
        self.tapered(from, to, halves, true, 0.0);
    }

    /// A stroke from `from` to `to` whose half thickness in pixels of the
    /// grid runs from `halves[0]` to `halves[1]`, with round ends, or
    /// ending flat at them when `square`.
    fn tapered(&mut self, from: Point, to: Point, halves: [f64; 2], square: bool, hue: f64) {
        let (a, b) = (self.transform * from, self.transform * to);
        let widest = halves[0].max(halves[1]);
        let reach = widest + self.fade(widest) / 2.0;
        let (x0, x1) = (
            (a.x.min(b.x) - reach).floor() as i64,
            (a.x.max(b.x) + reach).ceil() as i64,
        );
        let (y0, y1) = (
            (a.y.min(b.y) - reach).floor() as i64,
            (a.y.max(b.y) + reach).ceil() as i64,
        );
        let (width, height) = (
            i64::from(self.pixels.width()),
            i64::from(self.pixels.height()),
        );
        let ab = b - a;
        let length2 = ab.hypot2();
        for y in y0.max(0)..y1.min(height) {
            for x in x0.max(0)..x1.min(width) {
                let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
                let t = match length2 > 0.0 {
                    true => ((p - a).dot(ab) / length2).clamp(0.0, 1.0),
                    false => 0.0,
                };
                let half = halves[0] + (halves[1] - halves[0]) * t;
                let fade = self.fade(half);
                let near = a + ab * t;
                let distance = match square && length2 > 0.0 {
                    // Butt ends: a box from `from` to `to`, its edges
                    // across the stroke at its ends.
                    true => {
                        let length = length2.sqrt();
                        let along = (p - a).dot(ab) / length;
                        let past = (along - length).max(-along);
                        (p - near).hypot().max(past + half)
                    }
                    false => (p - near).hypot(),
                };
                // A box filter a pixel wide over a hard edge, widened by
                // the softness: half covered at `half`.
                let cover = ((half - distance) / fade + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let edge = half + fade / 2.0;
                let color = self.color(distance / edge.max(f64::MIN_POSITIVE), hue);
                self.lay(x, y, color, cover as f32);
            }
        }
    }
}

/// `color` with its hue turned `degrees`, keeping its luma: a rotation
/// about the grey axis.
fn turned(color: [f32; 4], degrees: f64) -> [f32; 4] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    let third: f32 = 1.0 / 3.0;
    let root = third.sqrt();
    let k = |i: usize, j: usize| {
        let base = match i == j {
            true => cos + (1.0 - cos) * third,
            false => (1.0 - cos) * third,
        };
        let turn = match (i + 3 - j) % 3 {
            1 => root * sin,
            2 => -root * sin,
            _ => 0.0,
        };
        base + turn
    };
    let [r, g, b, a] = color;
    let c = [r, g, b];
    let out: [f32; 3] =
        std::array::from_fn(|i| (k(i, 0) * c[0] + k(i, 1) * c[1] + k(i, 2) * c[2]).clamp(0.0, 1.0));
    [out[0], out[1], out[2], a]
}
