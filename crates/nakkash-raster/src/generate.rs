//! Generators: effects that draw something of their own on the layer, a
//! gradient, a grid, noise, a beam, or cut it into blinds. Each is worked out at
//! every pixel in the layer's own coordinates, so it moves, turns and
//! scales with the layer.
//!
//! A generator draws where the layer is: where what the layer drew has
//! alpha, which for a solid, the usual canvas of a generator, is its box.

use vello_cpu::kurbo::{Affine, Point, Rect, Shape};

use crate::colour::{choice, number, rgb};
use crate::composite::Operation;
use crate::effect::{ParamValue, Space};
use crate::par;
use crate::pen::Pen;
use crate::pixels::Pixels;

/// The point in `values[i]`.
fn point(values: &[ParamValue], i: usize) -> [f32; 2] {
    match values[i] {
        ParamValue::Point(p) => p,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// The alpha of colour `values[i]`.
fn alpha(values: &[ParamValue], i: usize) -> f32 {
    match values[i] {
        ParamValue::Color(c) => c.a,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// Runs `paint` on every pixel the layer covers, with the pixel's centre
/// in the layer's coordinates (`back` maps the grid's to them), its
/// straight colour and its place in the grid (column, row); `paint` gives
/// the straight colour and alpha the pixel takes, the alpha as a share of
/// the layer's there.
fn each_in_layer(
    pixels: &mut Pixels,
    back: Affine,
    paint: impl Fn([f32; 2], [f32; 4], [u32; 2]) -> [f32; 4] + Sync,
) {
    let width = usize::from(pixels.width());
    par::each(pixels.data_mut(), |i, pixel| {
        let coverage = pixel[3];
        if coverage <= 0.0 {
            return;
        }
        let at = back * Point::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
        let straight = [
            pixel[0] / coverage,
            pixel[1] / coverage,
            pixel[2] / coverage,
            1.0,
        ];
        let place = [(i % width) as u32, (i / width) as u32];
        let [r, g, b, a] =
            paint([at.x as f32, at.y as f32], straight, place).map(|c| c.clamp(0.0, 1.0));
        let a = a * coverage;
        *pixel = [r * a, g * a, b * a, a];
    });
}

/// How `four-color` and `grid` draw over the layer, by name in
/// [`crate::composite::OPERATIONS`]; the first is the default.
pub(crate) const GENERATOR_MODES: &[&str] = &[
    "replace",
    "over",
    "add",
    "multiply",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "color-dodge",
    "color-burn",
    "darken",
    "lighten",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "stencil-alpha",
];

/// How `fractal-noise` draws over the layer, by name in
/// [`crate::composite::OPERATIONS`]; the first is the default.
pub(crate) const NOISE_MODES: &[&str] = &[
    "over",
    "replace",
    "add",
    "multiply",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "color-dodge",
    "color-burn",
    "darken",
    "lighten",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "stencil-alpha",
];

/// The operation `values[i]` picks from `modes`.
fn mode(modes: &[&str], values: &[ParamValue], i: usize) -> Operation {
    Operation::named(modes[choice(values, i)]).expect("every generator mode is an operation")
}

/// What a generator does with what the layer drew, at the pixel `at`: its
/// colour `over`, at `amount`, combined with the layer's straight colour
/// `under` by `operation`, as straight colour and a share of the layer's
/// alpha. `replace` draws it alone at `amount`; `over` lays it over the
/// layer by `amount`; the blend modes blend it with the layer, then lay
/// that over by `amount`.
fn composite(
    operation: Operation,
    under: [f32; 4],
    over: [f32; 3],
    amount: f32,
    at: [u32; 2],
) -> [f32; 4] {
    let [r, g, b, a] = operation.pixel(
        [under[0], under[1], under[2], 1.0],
        [over[0] * amount, over[1] * amount, over[2] * amount, amount],
        at,
    );
    match a > 0.0 {
        true => [r / a, g / a, b / a, a],
        false => [0.0; 4],
    }
}

/// `ramp`'s plan: a gradient from one colour at `start` to another
/// `direction` from it, along that line or around `start`, keeping `blend`
/// of the layer's own colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ramp {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// Where the ramp starts, in the layer's coordinates.
    pub start: [f32; 2],
    /// From `start` to where the ramp ends.
    pub direction: [f32; 2],
    /// The squared length of `direction`.
    pub length2: f32,
    /// The straight colour and alpha at `start`.
    pub from: [f32; 4],
    /// The straight colour and alpha at the end.
    pub to: [f32; 4],
    /// How much of the layer's own colour stays.
    pub blend: f32,
    /// Around `start`, rather than along the line.
    pub radial: bool,
}

/// [`Ramp`] from `ramp`'s values in `space`.
pub(crate) fn ramp_plan(values: &[ParamValue], space: &Space) -> Ramp {
    let (start, from, end, to) = (
        point(values, 0),
        rgb(values, 1),
        point(values, 2),
        rgb(values, 3),
    );
    let (from_a, to_a) = (alpha(values, 1), alpha(values, 3));
    let d = [end[0] - start[0], end[1] - start[1]];
    Ramp {
        back: space.transform.inverse(),
        start,
        direction: d,
        length2: d[0] * d[0] + d[1] * d[1],
        from: [from[0], from[1], from[2], from_a],
        to: [to[0], to[1], to[2], to_a],
        blend: number(values, 4),
        radial: choice(values, 5) == 1,
    }
}

/// `ramp`: a gradient from `start-color` at `start` to `end-color` at
/// `end`, along the line between them or around `start`; `blend` keeps
/// that much of the layer's own colour. See [`Ramp`].
pub(crate) fn ramp(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Ramp {
        back,
        start,
        direction: d,
        length2,
        from,
        to,
        blend,
        radial,
    } = ramp_plan(values, space);
    let (from_a, to_a) = (from[3], to[3]);
    each_in_layer(pixels, back, |[x, y], under, _| {
        let v = [x - start[0], y - start[1]];
        let t = match (radial, length2 > 0.0) {
            (_, false) => 1.0,
            (false, true) => (v[0] * d[0] + v[1] * d[1]) / length2,
            (true, true) => ((v[0] * v[0] + v[1] * v[1]) / length2).sqrt(),
        }
        .clamp(0.0, 1.0);
        let c: [f32; 3] = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t);
        let a = from_a + (to_a - from_a) * t;
        let mix = |i: usize| c[i] + (under[i] - c[i]) * blend;
        [mix(0), mix(1), mix(2), a + (1.0 - a) * blend]
    });
}

/// `four-color`'s plan: four colours at four points, each pixel the mean
/// of them weighted by the inverse of its distance to each to `power`,
/// combined with the layer by an operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FourColor {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// Where each colour is, in the layer's coordinates.
    pub points: [[f32; 2]; 4],
    /// The straight colours.
    pub colors: [[f32; 3]; 4],
    /// What the distances are raised to, negated: 2 × 100 / `blend`.
    pub power: f32,
    /// How much of the colours show.
    pub opacity: f32,
    /// How they combine with the layer.
    pub operation: Operation,
}

/// [`FourColor`] from `four-color`'s values in `space`.
pub(crate) fn four_color_plan(values: &[ParamValue], space: &Space) -> FourColor {
    FourColor {
        back: space.transform.inverse(),
        points: std::array::from_fn(|i| point(values, 2 * i)),
        colors: std::array::from_fn(|i| rgb(values, 2 * i + 1)),
        power: 2.0 * 100.0 / number(values, 8).max(1.0),
        opacity: number(values, 9),
        operation: mode(GENERATOR_MODES, values, 10),
    }
}

/// `four-color`: four colours at four points, each pixel the mean of them
/// weighted by the inverse of its distance to each to the power
/// 2 × 100 / `blend`, so a larger `blend` mixes them further. See
/// [`FourColor`].
pub(crate) fn four_color(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let FourColor {
        back,
        points,
        colors,
        power,
        opacity,
        operation,
    } = four_color_plan(values, space);
    each_in_layer(pixels, back, |[x, y], under, at| {
        let mut weights = [0.0f32; 4];
        for (w, p) in weights.iter_mut().zip(&points) {
            let d2 = (x - p[0]).powi(2) + (y - p[1]).powi(2);
            *w = match d2 > 1e-6 {
                true => d2.powf(-power / 2.0),
                false => f32::INFINITY,
            };
        }
        let c: [f32; 3] = match weights.iter().position(|w| w.is_infinite()) {
            Some(at) => colors[at],
            None => {
                let total: f32 = weights.iter().sum();
                std::array::from_fn(|i| {
                    weights
                        .iter()
                        .zip(&colors)
                        .map(|(w, c)| w * c[i])
                        .sum::<f32>()
                        / total
                })
            }
        };
        composite(operation, under, c, opacity, at)
    });
}

/// `grid`'s plan: lines `border` px wide around cells from `anchor`, or
/// the cells instead, combined with the layer by an operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLines {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// Where a corner of the cells is, in the layer's coordinates.
    pub anchor: [f32; 2],
    /// A cell's width and height, at least 0.001 px.
    pub cell: [f32; 2],
    /// How wide the lines are.
    pub border: f32,
    /// The lines' straight colour.
    pub color: [f32; 3],
    /// How much of the grid shows.
    pub opacity: f32,
    /// The cells rather than the lines.
    pub cells: bool,
    /// How the grid combines with the layer.
    pub operation: Operation,
}

/// [`GridLines`] from `grid`'s values in `space`.
pub(crate) fn grid_plan(values: &[ParamValue], space: &Space) -> GridLines {
    let (anchor, corner) = (point(values, 0), point(values, 1));
    let (width, height, border) = (number(values, 2), number(values, 3), number(values, 4));
    let cell = match choice(values, 7) {
        0 => [(corner[0] - anchor[0]).abs(), (corner[1] - anchor[1]).abs()],
        1 => [width, width],
        _ => [width, height],
    }
    .map(|c| c.max(1e-3));
    GridLines {
        back: space.transform.inverse(),
        anchor,
        cell,
        border,
        color: rgb(values, 5),
        opacity: number(values, 6),
        cells: choice(values, 8) == 1,
        operation: mode(GENERATOR_MODES, values, 9),
    }
}

/// `grid`: lines `border` px wide around cells from `anchor`, each cell
/// reaching to `corner`, or `width` square, or `width` by `height`; with
/// `invert`, the cells instead of the lines. See [`GridLines`].
pub(crate) fn grid(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let GridLines {
        back,
        anchor,
        cell,
        border,
        color,
        opacity,
        cells,
        operation,
    } = grid_plan(values, space);
    // A pixel's share of a line, in px of the layer's own units.
    let line = |at: f32, step: f32| {
        let off = (at / step).rem_euclid(1.0) * step;
        let distance = off.min(step - off);
        (border / 2.0 - distance + 0.5).clamp(0.0, 1.0)
    };
    each_in_layer(pixels, back, |[x, y], under, at| {
        let on = line(x - anchor[0], cell[0]).max(line(y - anchor[1], cell[1]));
        let on = if cells { 1.0 - on } else { on };
        composite(operation, under, color, on * opacity, at)
    });
}

/// `venetian-blinds`' plan: the layer cut into stripes, each hidden from
/// one side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VenetianBlinds {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// How much of each stripe is hidden.
    pub completion: f32,
    /// The sine and cosine of the stripes' direction.
    pub turn: [f32; 2],
    /// How wide each stripe is, at least 0.001 px.
    pub width: f32,
    /// How wide a stripe's soft edge is; 0 for a hard one.
    pub feather: f32,
}

/// [`VenetianBlinds`] from `venetian-blinds`' values in `space`.
pub(crate) fn venetian_blinds_plan(values: &[ParamValue], space: &Space) -> VenetianBlinds {
    let (sin, cos) = number(values, 1).to_radians().sin_cos();
    VenetianBlinds {
        back: space.transform.inverse(),
        completion: number(values, 0),
        turn: [sin, cos],
        width: number(values, 2).max(1e-3),
        feather: number(values, 3),
    }
}

/// `venetian-blinds`: the layer cut into stripes `width` px wide, running
/// across `direction` (0° horizontal stripes), each hidden from one side
/// by `completion`, with a soft edge `feather` px wide. See
/// [`VenetianBlinds`].
pub(crate) fn venetian_blinds(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let VenetianBlinds {
        back,
        completion,
        turn: [sin, cos],
        width,
        feather,
    } = venetian_blinds_plan(values, space);
    each_in_layer(pixels, back, |[x, y], under, _| {
        let along = (x * sin + y * cos).rem_euclid(width);
        let edge = completion * width;
        let shown = match feather > 0.0 {
            true => ((along - edge) / feather + 0.5).clamp(0.0, 1.0),
            false => f32::from(u8::from(along >= edge)),
        };
        [under[0], under[1], under[2], shown]
    });
}

/// `linear-wipe`'s plan: the layer hidden behind an edge that crosses its
/// box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearWipe {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// The layer's box; `None` for the frame's, as the layer sees it.
    pub bounds: Option<Rect>,
    /// How far the edge has crossed the box.
    pub completion: f32,
    /// The sine and cosine of the way the edge travels, clockwise from up.
    pub turn: [f32; 2],
    /// How wide the edge's fade is, in px.
    pub feather: f32,
}

impl LinearWipe {
    /// How far along the wipe's direction a point of the layer lies.
    pub fn along(&self, [x, y]: [f32; 2]) -> f32 {
        let [sin, cos] = self.turn;
        x * sin - y * cos
    }

    /// Where the edge is along the wipe's direction, on a grid of
    /// `width` by `height` pixels.
    pub fn edge(&self, width: u16, height: u16) -> f32 {
        let bounds = self.bounds.unwrap_or_else(|| {
            let frame = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
            (self.back * frame.to_path(0.1)).bounding_box()
        });
        let corners = [
            [bounds.x0, bounds.y0],
            [bounds.x1, bounds.y0],
            [bounds.x0, bounds.y1],
            [bounds.x1, bounds.y1],
        ]
        .map(|p| self.along(p.map(|c| c as f32)));
        let (from, to) = corners
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &c| {
                (lo.min(c), hi.max(c))
            });
        from - self.feather / 2.0 + self.completion * (to - from + self.feather)
    }
}

/// [`LinearWipe`] from `linear-wipe`'s values in `space`.
pub(crate) fn linear_wipe_plan(values: &[ParamValue], space: &Space) -> LinearWipe {
    let (sin, cos) = number(values, 1).to_radians().sin_cos();
    LinearWipe {
        back: space.transform.inverse(),
        bounds: space.bounds,
        completion: number(values, 0),
        turn: [sin, cos],
        feather: number(values, 2),
    }
}

/// `linear-wipe`: the layer hidden behind an edge that crosses its box
/// along `angle` (0° up, turning clockwise, so 90° runs left to right),
/// `completion` of the way. The edge fades over `feather` px, and its
/// travel takes the feather in, so 0 shows the whole box and 1 hides it.
/// See [`LinearWipe`].
pub(crate) fn linear_wipe(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let wipe = linear_wipe_plan(values, space);
    let edge = wipe.edge(pixels.width(), pixels.height());
    let feather = wipe.feather;
    each_in_layer(pixels, wipe.back, |at, under, _| {
        [
            under[0],
            under[1],
            under[2],
            shown(wipe.along(at) - edge, feather),
        ]
    });
}

/// How much of the layer shows `beyond` px past a wipe's edge, on the side
/// it moves towards, with the edge faded over `feather` px about it; a
/// hard edge still covers a pixel in part.
fn shown(beyond: f32, feather: f32) -> f32 {
    (beyond / feather.max(1.0) + 0.5).clamp(0.0, 1.0)
}

/// `iris-wipe`'s plan: a polygon hole in the layer, its edge faded
/// inwards.
#[derive(Debug, Clone, PartialEq)]
pub struct IrisWipe {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// The hole's corners in order, in the layer's coordinates; `None`
    /// when its radii leave no hole.
    pub outline: Option<Vec<[f32; 2]>>,
    /// How wide the edge's fade is, inwards, in px; 0 for a hard edge.
    pub feather: f32,
}

/// [`IrisWipe`] from `iris-wipe`'s values in `space`.
pub(crate) fn iris_wipe_plan(values: &[ParamValue], space: &Space) -> IrisWipe {
    let center = point(values, 0);
    let corners = number(values, 1).round().max(3.0) as usize;
    let (outer, inner) = (number(values, 2), number(values, 3));
    let rotation = number(values, 4).to_radians();
    let star = choice(values, 6) == 1;
    let outline: Vec<[f32; 2]> = (0..corners)
        .map(|i| {
            let r = if star && i % 2 == 1 { inner } else { outer };
            let turn = rotation + i as f32 / corners as f32 * std::f32::consts::TAU;
            [center[0] + r * turn.sin(), center[1] - r * turn.cos()]
        })
        .collect();
    let hides_nothing = outer <= 0.0 && (!star || inner <= 0.0);
    IrisWipe {
        back: space.transform.inverse(),
        outline: (!hides_nothing).then_some(outline),
        feather: number(values, 5),
    }
}

/// `iris-wipe`: a hole in the layer around `center`, a polygon of
/// `points` corners (rounded) `outer-radius` px from it, its first corner
/// straight up and turned `rotation` degrees clockwise; with `inner` on,
/// every other corner sits `inner-radius` px from it instead, a star. The
/// hole's edge fades over `feather` px inwards, so a radius of 0 hides
/// nothing. See [`IrisWipe`].
pub(crate) fn iris_wipe(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let IrisWipe {
        back,
        outline,
        feather,
    } = iris_wipe_plan(values, space);
    let Some(outline) = outline else {
        return;
    };
    each_in_layer(pixels, back, |at, under, _| {
        let outside = signed_distance(&outline, at);
        let shown = match feather > 0.0 {
            true => (outside / feather + 1.0).clamp(0.0, 1.0),
            false => (outside + 0.5).clamp(0.0, 1.0),
        };
        [under[0], under[1], under[2], shown]
    });
}

/// How far `at` lies outside the closed polygon `outline`, in px: negative
/// inside.
fn signed_distance(outline: &[[f32; 2]], [x, y]: [f32; 2]) -> f32 {
    let mut nearest = f32::INFINITY;
    let mut inside = false;
    for (i, &[ax, ay]) in outline.iter().enumerate() {
        let [bx, by] = outline[(i + 1) % outline.len()];
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0.0, 1.0);
        let (px, py) = (ax + t * dx - x, ay + t * dy - y);
        nearest = nearest.min(px * px + py * py);
        // Even-odd crossings of the ray to the right.
        if (ay > y) != (by > y) && x < ax + (y - ay) / (by - ay) * dx {
            inside = !inside;
        }
    }
    let distance = nearest.sqrt();
    if inside { -distance } else { distance }
}

/// The layer's px across one noise cell at a `scale` of 1.
pub const NOISE_CELL: f32 = 100.0;

/// How the octaves of fractal noise sum, by `fractal`'s choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fractal {
    /// Every octave's level as it is.
    Basic,
    /// Each octave folded about mid grey, and the sum's creases rounded
    /// off.
    TurbulentSmooth,
    /// Each octave folded about mid grey.
    TurbulentBasic,
    /// Each octave folded about mid grey, and the sum's square root.
    TurbulentSharp,
    /// Each octave dragged across by the level of those before it.
    Smeary,
    /// Each octave curled by the level of those before it.
    Swirly,
}

/// How value noise blends between lattice values, by `noise`'s choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseKind {
    /// A quintic ease: smooth, with smooth slopes.
    Spline,
    /// A cubic ease.
    SoftLinear,
    /// Straight lines.
    Linear,
    /// No blend: each cell takes its corner's value.
    Block,
}

/// How noise levels outside 0 to 1 come back, by `overflow`'s choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Held to 0 to 1.
    Clip,
    /// Through a logistic curve about mid grey.
    SoftClamp,
    /// Folded back and forth into 0 to 1.
    WrapBack,
}

/// `fractal-noise`'s plan: grey value noise summed over octaves, shaped,
/// and combined with the layer by an operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FractalNoise {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// How far levels spread from mid grey.
    pub contrast: f32,
    /// What is added to every level.
    pub brightness: f32,
    /// How far the noise turns, in radians.
    pub rotation: f32,
    /// A cell's width and height in the layer's px.
    pub scale: [f32; 2],
    /// Where the noise is centred, in the layer's coordinates.
    pub offset: [f32; 2],
    /// How many octaves it sums, at least 1; the last weighs by its
    /// fraction.
    pub complexity: f32,
    /// Each octave's weight against the one before.
    pub influence: f32,
    /// Each octave's size against the one before, at least 0.01.
    pub scaling: f32,
    /// How far each octave turns, in radians.
    pub sub_rotation: f32,
    /// How far each octave moves, in the layer's px.
    pub sub_offset: [f32; 2],
    /// Where the noise is in its third dimension, in cells.
    pub evolution: f32,
    /// Which noise it is: the first octave's seed, each next one's one
    /// more.
    pub seed: u32,
    /// How the octaves sum.
    pub fractal: Fractal,
    /// How lattice values blend.
    pub kind: NoiseKind,
    /// How levels outside 0 to 1 come back.
    pub overflow: Overflow,
    /// Whether the level is turned over before contrast and brightness.
    pub invert: bool,
    /// How much of the noise shows.
    pub opacity: f32,
    /// How the noise combines with the layer.
    pub operation: Operation,
}

/// [`FractalNoise`] from `fractal-noise`'s values in `space`.
pub(crate) fn fractal_noise_plan(values: &[ParamValue], space: &Space) -> FractalNoise {
    let scale = number(values, 3);
    FractalNoise {
        back: space.transform.inverse(),
        contrast: number(values, 0),
        brightness: number(values, 1),
        rotation: number(values, 2).to_radians(),
        scale: [
            scale * number(values, 4) * NOISE_CELL,
            scale * number(values, 5) * NOISE_CELL,
        ]
        .map(|s| s.max(1e-3)),
        offset: point(values, 6),
        complexity: number(values, 7).max(1.0),
        influence: number(values, 8),
        scaling: number(values, 9).max(0.01),
        sub_rotation: number(values, 10).to_radians(),
        sub_offset: point(values, 11),
        evolution: number(values, 12) / 360.0,
        seed: number(values, 13).max(0.0) as u32,
        fractal: match choice(values, 15) {
            0 => Fractal::Basic,
            1 => Fractal::TurbulentSmooth,
            2 => Fractal::TurbulentBasic,
            3 => Fractal::TurbulentSharp,
            4 => Fractal::Smeary,
            _ => Fractal::Swirly,
        },
        kind: match choice(values, 16) {
            0 => NoiseKind::Spline,
            1 => NoiseKind::SoftLinear,
            2 => NoiseKind::Linear,
            _ => NoiseKind::Block,
        },
        overflow: match choice(values, 17) {
            0 => Overflow::Clip,
            1 => Overflow::SoftClamp,
            _ => Overflow::WrapBack,
        },
        invert: choice(values, 18) == 1,
        opacity: number(values, 14),
        operation: mode(NOISE_MODES, values, 19),
    }
}

/// `fractal-noise`: grey value noise in `complexity` octaves, each
/// `sub-scaling` the size of the one before, turned by `sub-rotation`,
/// moved by `sub-offset` and weighed by `sub-influence`; `evolution` moves
/// through a third dimension, one turn a cell. `invert` turns the level
/// over first; contrast and brightness then apply around mid grey, and
/// `overflow` brings the level into 0 to 1. See [`FractalNoise`].
pub(crate) fn fractal_noise(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let noise = fractal_noise_plan(values, space);
    let (opacity, operation) = (noise.opacity, noise.operation);
    each_in_layer(pixels, noise.back, |point, under, at| {
        let v = noise.at(point);
        composite(operation, under, [v; 3], opacity, at)
    });
}

impl FractalNoise {
    /// How many octaves it sums.
    pub fn octaves(&self) -> u32 {
        self.complexity.ceil() as u32
    }

    /// The level at `[x, y]`, in the layer's coordinates.
    pub fn at(&self, [x, y]: [f32; 2]) -> f32 {
        let (sin, cos) = self.rotation.sin_cos();
        let (dx, dy) = (x - self.offset[0], y - self.offset[1]);
        let mut p = [
            (dx * cos + dy * sin) / self.scale[0],
            (-dx * sin + dy * cos) / self.scale[1],
        ];
        let (sub_sin, sub_cos) = self.sub_rotation.sin_cos();
        let turbulent = matches!(
            self.fractal,
            Fractal::TurbulentSmooth | Fractal::TurbulentBasic | Fractal::TurbulentSharp
        );
        let warp = Warp::of(self.fractal);
        let (mut sum, mut total, mut weight) = (0.0f32, 0.0f32, 1.0f32);
        let octaves = self.octaves();
        for octave in 0..octaves {
            // The last octave weighs by the fraction of complexity left.
            let share = (self.complexity - octave as f32).min(1.0);
            let n = value_noise(
                [p[0], p[1], self.evolution],
                self.seed.wrapping_add(octave),
                self.kind as usize,
            );
            let n = if turbulent { (2.0 * n - 1.0).abs() } else { n };
            sum += n * weight * share;
            total += weight * share;
            weight *= self.influence;
            let mut scaled = [p[0] / self.scaling, p[1] / self.scaling];
            // A warping type moves the next octave by the level so far.
            if let Some(warp) = warp {
                let [dx, dy] = warp.by(sum / total.max(f32::EPSILON));
                scaled = [scaled[0] + dx, scaled[1] + dy];
            }
            p = [
                scaled[0] * sub_cos + scaled[1] * sub_sin + self.sub_offset[0] / NOISE_CELL,
                -scaled[0] * sub_sin + scaled[1] * sub_cos + self.sub_offset[1] / NOISE_CELL,
            ];
        }
        let mut v = if total > 0.0 { sum / total } else { 0.5 };
        if self.fractal == Fractal::TurbulentSmooth {
            // Turbulent smooth: the creases rounded off.
            v = v * v * (3.0 - 2.0 * v);
        } else if self.fractal == Fractal::TurbulentSharp {
            v = v.sqrt();
        }
        // Inverted before the contrast and brightness, which then shape the
        // inverted level: a dark, contrasted field keeps its bright lines.
        let v = if self.invert { 1.0 - v } else { v };
        let v = (v - 0.5) * self.contrast + 0.5 + self.brightness;
        match self.overflow {
            Overflow::SoftClamp => 1.0 / (1.0 + (-4.0 * (v - 0.5)).exp()),
            Overflow::WrapBack => {
                let t = v.rem_euclid(2.0);
                if t > 1.0 { 2.0 - t } else { t }
            }
            Overflow::Clip => v.clamp(0.0, 1.0),
        }
    }
}

/// How a warping fractal type moves each octave after the first: by the
/// level of the octaves before it, in cells of the next octave, so its
/// detail follows the shapes of the larger ones.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Warp {
    /// `smeary`: along the noise's own x axis, by up to one cell either
    /// way, which drags the detail into streaks across.
    Smear,
    /// `swirly`: half a cell in a direction that turns once around as the
    /// level runs from 0 to 1, which curls the detail around the larger
    /// shapes.
    Swirl,
}

impl Warp {
    /// The warp of the fractal type `fractal`, if it warps.
    fn of(fractal: Fractal) -> Option<Warp> {
        match fractal {
            Fractal::Smeary => Some(Warp::Smear),
            Fractal::Swirly => Some(Warp::Swirl),
            _ => None,
        }
    }

    /// How far it moves the next octave, at `level` so far, 0 to 1.
    fn by(self, level: f32) -> [f32; 2] {
        match self {
            Warp::Smear => [2.0 * level - 1.0, 0.0],
            Warp::Swirl => {
                let (sin, cos) = (std::f32::consts::TAU * level).sin_cos();
                [0.5 * cos, 0.5 * sin]
            }
        }
    }
}

/// Value noise at `p`, 0 to 1, between lattice values blended as `kind`
/// says: 0 `spline`, 1 `soft-linear`, 2 `linear` or 3 `block`.
pub(crate) fn value_noise(p: [f32; 3], seed: u32, kind: usize) -> f32 {
    let cell = p.map(f32::floor);
    let f: [f32; 3] = std::array::from_fn(|i| p[i] - cell[i]);
    let at = cell.map(|c| c as i32);
    if kind == 3 {
        return lattice(at, seed);
    }
    let ease = |t: f32| match kind {
        0 => t * t * t * (t * (t * 6.0 - 15.0) + 10.0),
        1 => t * t * (3.0 - 2.0 * t),
        _ => t,
    };
    let w = f.map(ease);
    // Wrapping, as a release build adds: a cell far out saturates at the
    // largest i32, and its neighbour wraps round.
    let corner = |dx: i32, dy: i32, dz: i32| {
        lattice(
            [
                at[0].wrapping_add(dx),
                at[1].wrapping_add(dy),
                at[2].wrapping_add(dz),
            ],
            seed,
        )
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let plane = |dz: i32| {
        lerp(
            lerp(corner(0, 0, dz), corner(1, 0, dz), w[0]),
            lerp(corner(0, 1, dz), corner(1, 1, dz), w[0]),
            w[1],
        )
    };
    lerp(plane(0), plane(1), w[2])
}

/// A lattice point's value, 0 to 1, hashed from its coordinates and seed.
fn lattice([x, y, z]: [i32; 3], seed: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9)
        ^ (x as u32).wrapping_mul(0x85EB_CA6B)
        ^ (y as u32).wrapping_mul(0xC2B2_AE35)
        ^ (z as u32).wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// `beam`'s plan: a straight beam along a line, in place of the layer or
/// over it, kept to the layer's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beam {
    /// From the layer's own coordinates to pixels of the grid.
    pub transform: Affine,
    /// The layer's box, in its own coordinates, where the beam draws;
    /// `None` for everywhere.
    pub bounds: Option<Rect>,
    /// Whether the beam draws in place of the layer, rather than over it.
    pub alone: bool,
    /// The beam's stroke; `None` when it has not left `start` or has
    /// passed `end`.
    pub stroke: Option<BeamStroke>,
}

/// The stroke a [`Beam`] draws: thickness running along it, ends flat,
/// edges faded, coloured from its middle to its edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeamStroke {
    /// Where its back is, in the layer's coordinates.
    pub from: Point,
    /// Where its front is, in the layer's coordinates.
    pub to: Point,
    /// How thick it is at `from` and at `to`, in px of the layer.
    pub thickness: [f64; 2],
    /// How much of its half thickness fades at its edges, 0 to 1.
    pub softness: f64,
    /// Its colour along its middle and at its edges, straight RGBA.
    pub colors: [[f32; 4]; 2],
}

/// [`Beam`] from `beam`'s values in `space`.
pub(crate) fn beam_plan(values: &[ParamValue], space: &Space) -> Beam {
    let (start, end) = (point(values, 0), point(values, 1));
    let (length, time) = (number(values, 2), number(values, 3));
    let front = (time * (1.0 + length)).clamp(0.0, 1.0);
    let back = (time * (1.0 + length) - length).clamp(0.0, 1.0);
    let along = |k: f32| {
        let k = f64::from(k);
        let at = |i: usize| f64::from(start[i]) + (f64::from(end[i]) - f64::from(start[i])) * k;
        Point::new(at(0), at(1))
    };
    let (from, to) = (number(values, 4), number(values, 5));
    let thickness = |k: f32| f64::from(from + (to - from) * k);
    let color = |i: usize| {
        let [r, g, b] = rgb(values, i);
        [r, g, b, alpha(values, i)]
    };
    Beam {
        transform: space.transform,
        bounds: space.bounds,
        alone: choice(values, 9) == 0,
        stroke: (front > back).then(|| BeamStroke {
            from: along(back),
            to: along(front),
            thickness: [thickness(back), thickness(front)],
            softness: f64::from(number(values, 6)),
            colors: [color(7), color(8)],
        }),
    }
}

/// `beam`: a straight beam along the line from `start` to `end`, `length`
/// of the line long, its front `time × (1 + length)` of the way along: at
/// `time` 0 it has not left `start`, at 1 its back has reached `end`, and
/// with `length` 1 it spans the whole line at `time` 0.5. Its thickness runs
/// from `start-thickness` at `start` to `end-thickness` at `end`, its ends
/// are flat, and its edges fade over `softness` of its half thickness, from
/// `inside-color` along its middle to `outside-color` at its edges. It
/// draws in place of the layer, or over it with `composite` on, kept to
/// the layer's box. See [`Beam`].
pub(crate) fn beam(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Beam { alone, stroke, .. } = beam_plan(values, space);
    if alone {
        pixels.data_mut().fill([0.0; 4]);
    }
    let Some(stroke) = stroke else {
        return;
    };
    let [back, front] = stroke.thickness;
    let mut pen = Pen::new(
        pixels,
        space,
        back.max(front),
        stroke.softness,
        stroke.colors,
    );
    pen.beam(stroke.from, stroke.to, stroke.thickness);
}

#[cfg(test)]
#[path = "../tests/unit/generate.rs"]
mod tests;
