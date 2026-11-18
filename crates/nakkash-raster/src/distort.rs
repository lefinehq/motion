//! Distortions: effects that move a layer's pixels about. Each works out,
//! for every pixel, the point of the layer's own coordinates it shows, and
//! reads the layer's drawing there, between pixels.
//!
//! Those that tile, wrap or pin at the layer's edges use its box
//! ([`Space::bounds`]); a layer without one, such as shapes or text, uses
//! the frame's box seen from the layer.

use vello_cpu::kurbo::{Affine, Point, Rect, Shape, Vec2};

use crate::channel::{Level, Reading};
use crate::colour::{choice, number};
use crate::effect::{ParamValue, Space};
use crate::generate::value_noise;
use crate::par;
use crate::pixels::Pixels;

/// The point in `values[i]`.
fn point(values: &[ParamValue], i: usize) -> [f32; 2] {
    match values[i] {
        ParamValue::Point(p) => p,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// The layer's box in its own coordinates: its own, or the frame's.
pub(crate) fn layer_box(pixels: &Pixels, space: &Space) -> Rect {
    distortion_box(space, pixels.width(), pixels.height())
}

/// The box a distortion tiles, wraps or pins at on a grid of `width` by
/// `height` pixels in `space`, in the layer's own coordinates: the layer's
/// own box, or, for a layer without one, the frame's box seen from the
/// layer.
pub fn distortion_box(space: &Space, width: u16, height: u16) -> Rect {
    space.bounds.unwrap_or_else(|| {
        let frame = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
        (space.transform.inverse() * frame.to_path(0.1)).bounding_box()
    })
}

/// A distortion's plan: what its values come to. Points and lengths stay
/// in the layer's own coordinates, as the CPU reads them; the box they
/// meet is [`distortion_box`] of the pixels it runs on.
#[derive(Debug, Clone, PartialEq)]
pub enum Distortion {
    /// `displacement-map`.
    DisplacementMap(DisplacementMap),
    /// `turbulent-displace`.
    TurbulentDisplace(TurbulentDisplace),
    /// `polar-coordinates`.
    PolarCoordinates(PolarCoordinates),
    /// `lens-distortion`.
    LensDistortion(LensDistortion),
    /// `repeat-tile`.
    RepeatTile(RepeatTile),
    /// `motion-tile`.
    MotionTile(MotionTile),
    /// `wrap`.
    Wrap(Wrap),
    /// `mirror`.
    Mirror(Mirror),
    /// `bezier-warp`.
    BezierWarp(BezierWarp),
}

/// `motion-tile`'s plan: the box shrunk to `tile` of itself and repeated
/// around `center`, over `output` of the box about its middle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionTile {
    /// Where a tile is centred, in the layer's coordinates.
    pub center: [f32; 2],
    /// A tile's width and height, as shares of the box's.
    pub tile: [f32; 2],
    /// How far the tiles reach, as shares of the box, about its middle.
    pub output: [f32; 2],
    /// What every other column (or row) shifts by, in turns.
    pub phase: f32,
    /// Whether every other tile flips.
    pub mirror: bool,
    /// Whether the phase shifts rows rather than columns.
    pub rows: bool,
}

/// `wrap`'s plan: the content moved so the box's middle lands on `to`,
/// wrapping around the box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wrap {
    /// Where the box's middle lands, in the layer's coordinates.
    pub to: [f32; 2],
    /// How much of the layer as it was is kept, 0 to 1; none at 0 or less.
    pub blend: f32,
}

/// `mirror`'s plan: the side of a line its normal faces shows the other
/// side reflected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mirror {
    /// A point of the line, in the layer's coordinates.
    pub center: [f32; 2],
    /// The sine of the angle of the line's normal from the x axis.
    pub sin: f32,
    /// Its cosine.
    pub cos: f32,
}

/// `polar-coordinates`' plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarCoordinates {
    /// How far each pixel goes towards where the conversion takes it.
    pub amount: f32,
    /// Whether rings unroll into rows (`polar-to-rect`), rather than rows
    /// rolling into rings.
    pub unroll: bool,
}

/// The span of the box a lens's field of view covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LensField {
    /// Its width.
    Horizontal,
    /// Its height.
    Vertical,
    /// Its diagonal.
    Diagonal,
}

/// `lens-distortion`'s plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensDistortion {
    /// The field of view, in degrees, at most just short of a half turn.
    pub degrees: f64,
    /// Where the lens's axis meets the layer.
    pub center: [f32; 2],
    /// Whether the lens is removed, rather than added.
    pub remove: bool,
    /// The span the field of view covers.
    pub field: LensField,
    /// How many times its width and height the box may grow to; 1 when it
    /// keeps its size.
    pub grow: f64,
}

impl LensDistortion {
    /// Its lens's focal length over `bounds`, in px of the layer; `None`
    /// when it bends nothing.
    pub fn focal(&self, bounds: Rect) -> Option<f64> {
        Lens::new(self, bounds).map(|lens| lens.focal)
    }

    /// The box it draws in, once run on `bounds`: `bounds`, or grown to
    /// hold what the lens draws of it, up to `grow` times its size.
    pub fn shown(&self, bounds: Rect) -> Rect {
        lens_box_of(bounds, self)
    }
}

/// How `repeat-tile` lays its tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tiling {
    /// Every tile is the box.
    Repeat,
    /// Every other column flips across, every other row upside down.
    Unfold,
    /// Every other row shifts by half a tile.
    Brick,
    /// None: the box only grows.
    Empty,
}

/// `repeat-tile`'s plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepeatTile {
    /// How far the tiles reach right, left, down and up of the box.
    pub reach: [f32; 4],
    /// How they lie.
    pub tiling: Tiling,
}

/// The displacements of `turbulent-displace`, in the order of its choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turbulence {
    /// Each axis by its own noise.
    Turbulent,
    /// The same, its finer octaves at half weight.
    TurbulentSmoother,
    /// Out along the noise's slope.
    Bulge,
    /// The same, smoother.
    BulgeSmoother,
    /// Across the slope, round the noise's hills.
    Twist,
    /// The same, smoother.
    TwistSmoother,
    /// Along x only.
    Horizontal,
    /// Along y only.
    Vertical,
    /// Both axes by one noise.
    Cross,
}

impl Turbulence {
    /// Whether its finer octaves weigh half.
    pub fn smoother(self) -> bool {
        matches!(
            self,
            Turbulence::TurbulentSmoother | Turbulence::BulgeSmoother | Turbulence::TwistSmoother
        )
    }
}

/// Which edges of the box `turbulent-displace` pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pinning {
    /// Every edge.
    All,
    /// The left and right edges.
    LeftRight,
    /// The top and bottom edges.
    TopBottom,
    /// None.
    Free,
}

/// `turbulent-displace`'s plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbulentDisplace {
    /// How far a pixel is read from, as a share of `size`.
    pub amount: f32,
    /// The noise's cell, in px of the layer, at least 1.
    pub size: f32,
    /// Where the noise's lattice starts.
    pub offset: [f32; 2],
    /// How many octaves, at least 1; the last weighs its fraction.
    pub complexity: f32,
    /// The noise's third coordinate, in turns.
    pub evolution: f32,
    /// The noise's seed.
    pub seed: u32,
    /// How the noise moves pixels.
    pub turbulence: Turbulence,
    /// Which edges stay put.
    pub pinning: Pinning,
}

impl TurbulentDisplace {
    /// How many octaves of noise it sums.
    pub fn octaves(&self) -> u32 {
        self.complexity.ceil() as u32
    }
}

/// A level a displacement reads from its map, 0 to 1, 0.5 moving nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapLevel {
    /// The red of the map's straight colour.
    Red,
    /// Its green.
    Green,
    /// Its blue.
    Blue,
    /// Its alpha.
    Alpha,
    /// Its luma, by Rec. 601 weights.
    Luminance,
    /// Its hue.
    Hue,
    /// The mean of its largest and smallest colour.
    Lightness,
    /// Its saturation, as the hue-lightness-saturation model measures it.
    Saturation,
    /// The most one way.
    Full,
    /// Half of that.
    Half,
    /// Nothing.
    Off,
}

/// `displacement-map`'s plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplacementMap {
    /// The most a pixel moves along x and y, in px of the layer.
    pub most: [f32; 2],
    /// What moves it along x and along y.
    pub from: [MapLevel; 2],
    /// How the map's box lies on the layer's.
    pub lay: Lay,
    /// Whether what moves past the box comes back on its other side.
    pub wrap: bool,
}

/// `bezier-warp`'s plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BezierWarp {
    /// How far each of its twelve points moves from where it rests, in px
    /// of the layer: round the box clockwise from its top-left corner, the
    /// corners and the handles a third of the way along each side.
    pub moves: [[f32; 2]; 12],
}

/// Redraws every pixel from the point of the layer `from` gives for the
/// layer point at its centre, or transparent where it gives none.
fn remap(pixels: &mut Pixels, space: &Space, from: impl Fn([f32; 2]) -> Option<[f32; 2]> + Sync) {
    let source = pixels.clone();
    let to: Affine = space.transform;
    let back = to.inverse();
    let width = usize::from(pixels.width());
    par::each(pixels.data_mut(), |i, pixel| {
        let at = back * Point::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
        *pixel = match from([at.x as f32, at.y as f32]) {
            Some([x, y]) => {
                let there = to * Point::new(f64::from(x), f64::from(y));
                source.sample(there.x, there.y)
            }
            None => [0.0; 4],
        };
    });
}

/// [`MotionTile`] from `motion-tile`'s values.
pub(crate) fn motion_tile_plan(values: &[ParamValue]) -> MotionTile {
    let [tile_w, tile_h, out_w, out_h] = std::array::from_fn(|i| number(values, i + 1));
    MotionTile {
        center: point(values, 0),
        tile: [tile_w, tile_h],
        output: [out_w, out_h],
        phase: number(values, 5) / 360.0,
        mirror: choice(values, 6) == 1,
        rows: choice(values, 7) == 1,
    }
}

/// `motion-tile`: the layer's box shrunk to `tile-width` by `tile-height`
/// of itself and repeated around `center`, drawn over `output-width` by
/// `output-height` of the box about its middle; with `mirror`, every other
/// tile flips, and `phase` shifts every other column (or row, with `shift`
/// `horizontal`) by that share of a turn.
pub(crate) fn motion_tile(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let MotionTile {
        center,
        tile: [tile_w, tile_h],
        output: [out_w, out_h],
        phase,
        mirror,
        rows,
    } = motion_tile_plan(values);
    let bounds = layer_box(pixels, space);
    let (origin, size) = (
        [bounds.x0 as f32, bounds.y0 as f32],
        [bounds.width() as f32, bounds.height() as f32],
    );
    let tile = [size[0] * tile_w, size[1] * tile_h].map(|t| t.max(1e-3));
    let output = [size[0] * out_w, size[1] * out_h];
    let middle = [origin[0] + size[0] / 2.0, origin[1] + size[1] / 2.0];
    remap(pixels, space, |[x, y]| {
        if (x - middle[0]).abs() > output[0] / 2.0 || (y - middle[1]).abs() > output[1] / 2.0 {
            return None;
        }
        let mut u = [
            (x - center[0]) / tile[0] + 0.5,
            (y - center[1]) / tile[1] + 0.5,
        ];
        // Every other column (or row) shifts by the phase.
        let (across, along) = if rows { (1, 0) } else { (0, 1) };
        if u[across].floor().rem_euclid(2.0) == 1.0 {
            u[along] += phase;
        }
        let at: [f32; 2] = std::array::from_fn(|i| {
            let f = u[i].rem_euclid(1.0);
            let flipped = mirror && u[i].floor().rem_euclid(2.0) == 1.0;
            origin[i] + (if flipped { 1.0 - f } else { f }) * size[i]
        });
        Some(at)
    });
}

/// [`Wrap`] from `wrap`'s values.
pub(crate) fn wrap_plan(values: &[ParamValue]) -> Wrap {
    Wrap {
        to: point(values, 0),
        blend: number(values, 1),
    }
}

/// `wrap`: the layer's content moved so the middle of its box lands on
/// `to`, wrapping around the box; `blend` keeps that much of the layer.
pub(crate) fn wrap(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Wrap { to, blend } = wrap_plan(values);
    let bounds = layer_box(pixels, space);
    let (origin, size) = (
        [bounds.x0 as f32, bounds.y0 as f32],
        [bounds.width() as f32, bounds.height() as f32],
    );
    let original = pixels.clone();
    let shift = [
        to[0] - origin[0] - size[0] / 2.0,
        to[1] - origin[1] - size[1] / 2.0,
    ];
    remap(pixels, space, |[x, y]| {
        let at = [x - shift[0], y - shift[1]];
        Some(std::array::from_fn(|i| match size[i] > 0.0 {
            true => origin[i] + (at[i] - origin[i]).rem_euclid(size[i]),
            false => at[i],
        }))
    });
    if blend > 0.0 {
        pixels.mix(&original, |_| blend);
    }
}

/// [`Mirror`] from `mirror`'s values.
pub(crate) fn mirror_plan(values: &[ParamValue]) -> Mirror {
    let (sin, cos) = number(values, 1).to_radians().sin_cos();
    Mirror {
        center: point(values, 0),
        sin,
        cos,
    }
}

/// `mirror`: the side of the line through `center` that `angle` faces
/// shows the other side reflected across the line. At 0° the line is
/// upright and the right side shows the left; the angle turns it
/// clockwise, so at 90° the bottom shows the top. Nothing shows outside
/// the layer's box, where the layer has nothing to reflect into.
pub(crate) fn mirror(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Mirror { center, sin, cos } = mirror_plan(values);
    let bounds = layer_box(pixels, space);
    remap(pixels, space, |[x, y]| {
        if !bounds.contains(Point::new(f64::from(x), f64::from(y))) {
            return None;
        }
        // How far the point lies on the reflected side of the line.
        let beyond = (x - center[0]) * cos + (y - center[1]) * sin;
        Some(match beyond > 0.0 {
            true => [x - 2.0 * beyond * cos, y - 2.0 * beyond * sin],
            false => [x, y],
        })
    });
}

/// [`PolarCoordinates`] from `polar-coordinates`' values.
pub(crate) fn polar_coordinates_plan(values: &[ParamValue]) -> PolarCoordinates {
    PolarCoordinates {
        amount: number(values, 0),
        unroll: choice(values, 1) == 1,
    }
}

/// `polar-coordinates`: with `rect-to-polar`, the point at angle φ
/// counterclockwise from straight up of the box's middle, `r` px from it,
/// shows the layer's point φ/2π of the way across the box and √2·`r` px
/// down from its top: rows become rings, the top row the middle, and the
/// columns run round them from straight up. The radius the box's height
/// maps to is half the diagonal of a square that tall. `polar-to-rect`
/// is the inverse, rings unrolled into rows. Each pixel goes
/// `interpolation` of the way from where it is towards where the
/// conversion takes it, and nothing shows outside the box.
pub(crate) fn polar_coordinates(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let PolarCoordinates { amount, unroll } = polar_coordinates_plan(values);
    let bounds = layer_box(pixels, space);
    let (left, top) = (bounds.x0 as f32, bounds.y0 as f32);
    let (width, height) = (bounds.width() as f32, bounds.height() as f32);
    let middle = [left + width / 2.0, top + height / 2.0];
    let turn = std::f32::consts::TAU;
    remap(pixels, space, |[x, y]| {
        if !bounds.contains(Point::new(f64::from(x), f64::from(y))) {
            return None;
        }
        let converted = match unroll {
            false => {
                let (dx, dy) = (x - middle[0], y - middle[1]);
                let angle = (-dx).atan2(-dy).rem_euclid(turn);
                [
                    left + width * angle / turn,
                    top + std::f32::consts::SQRT_2 * dx.hypot(dy),
                ]
            }
            true => {
                let angle = turn * (x - left) / width.max(1e-3);
                let radius = (y - top) / std::f32::consts::SQRT_2;
                let (sin, cos) = angle.sin_cos();
                [middle[0] - radius * sin, middle[1] - radius * cos]
            }
        };
        Some([
            x + (converted[0] - x) * amount,
            y + (converted[1] - y) * amount,
        ])
    });
}

/// The lens of `lens-distortion`: a lens that draws a ray at angle φ from
/// its axis `focal` · sin φ from the axis, where a pinhole draws it
/// `focal` · tan φ away. Its focal length makes the field of view span the
/// width, height or diagonal of the layer's box as `field` says, from one
/// end to the other.
struct Lens {
    /// Where its axis meets the layer.
    center: Point,
    /// Its focal length, in px of the layer.
    focal: f64,
}

impl Lens {
    /// The lens of the plan, over `bounds`; `None` when it bends nothing:
    /// a field of view of 0, or a box with no span.
    fn new(plan: &LensDistortion, bounds: Rect) -> Option<Lens> {
        let degrees = plan.degrees;
        let half_span = match plan.field {
            LensField::Horizontal => bounds.width() / 2.0,
            LensField::Vertical => bounds.height() / 2.0,
            LensField::Diagonal => bounds.width().hypot(bounds.height()) / 2.0,
        };
        if degrees <= 0.0 || half_span <= 0.0 {
            return None;
        }
        let [x, y] = plan.center;
        Some(Lens {
            center: Point::new(f64::from(x), f64::from(y)),
            focal: half_span / (degrees / 2.0).to_radians().tan(),
        })
    }

    /// How far from the axis the lens draws what a pinhole draws `r` px
    /// from it: always nearer.
    fn bent(&self, r: f64) -> f64 {
        r / (1.0 + (r / self.focal).powi(2)).sqrt()
    }

    /// How far from the axis a pinhole draws what the lens draws `r` px
    /// from it; `None` from the focal length out, which the lens draws
    /// from rays at a right angle to its axis or wider.
    fn unbent(&self, r: f64) -> Option<f64> {
        let left = 1.0 - (r / self.focal).powi(2);
        (left > 0.0).then(|| r / left.sqrt())
    }

    /// Where the point `p` of what it reads shows once drawn, removing
    /// the lens or adding it; `None` where it goes beyond every distance.
    fn moved(&self, p: Point, remove: bool) -> Option<Point> {
        let d = p - self.center;
        let r = d.hypot();
        if r <= 0.0 {
            return Some(p);
        }
        let to = match remove {
            true => self.unbent(r)?,
            false => self.bent(r),
        };
        Some(self.center + d * (to / r))
    }

    /// The point read for the drawn point `p`, removing the lens or adding
    /// it: the inverse of [`Lens::moved`].
    fn read(&self, p: Point, remove: bool) -> Option<Point> {
        let d = p - self.center;
        let r = d.hypot();
        if r <= 0.0 {
            return Some(p);
        }
        let from = match remove {
            true => self.bent(r),
            false => self.unbent(r)?,
        };
        Some(self.center + d * (from / r))
    }
}

/// The layer's box once `lens-distortion` has run on `bounds`: as it was
/// with `grow` `off`; else grown to hold all the lens draws of the box,
/// but no more than twice (`2x`) or four times (`4x`) its width and
/// height about its middle.
pub(crate) fn lens_box(bounds: Rect, values: &[ParamValue]) -> Rect {
    lens_box_of(bounds, &lens_distortion_plan(values))
}

/// [`lens_box`] of a plan.
fn lens_box_of(bounds: Rect, plan: &LensDistortion) -> Rect {
    if plan.grow == 1.0 {
        return bounds;
    }
    let most = plan.grow;
    let Some(lens) = Lens::new(plan, bounds) else {
        return bounds;
    };
    let remove = plan.remove;
    let cap = Rect::from_center_size(bounds.center(), bounds.size() * most);
    // The image of the box's outline holds the image of the box: the lens
    // moves every point along its ray from the axis, keeping their order.
    let corners = [
        Point::new(bounds.x0, bounds.y0),
        Point::new(bounds.x1, bounds.y0),
        Point::new(bounds.x1, bounds.y1),
        Point::new(bounds.x0, bounds.y1),
    ];
    const STEPS: usize = 256;
    let mut held = bounds;
    for side in 0..4 {
        let (from, to) = (corners[side], corners[(side + 1) % 4]);
        for step in 0..STEPS {
            let p = from.lerp(to, step as f64 / STEPS as f64);
            let shown = match lens.moved(p, remove) {
                Some(shown) => shown,
                // Out to infinity along its ray: as far as the cap.
                None => ray_exit(lens.center, p - lens.center, cap).unwrap_or(p),
            };
            held = held.union_pt(shown);
        }
    }
    held.intersect(cap)
}

/// Where the ray from `from` along `along` leaves `rect`, if it starts in
/// it.
fn ray_exit(from: Point, along: Vec2, rect: Rect) -> Option<Point> {
    if !rect.contains(from) {
        return None;
    }
    let reach = |p: f64, d: f64, lo: f64, hi: f64| match d {
        d if d > 0.0 => (hi - p) / d,
        d if d < 0.0 => (lo - p) / d,
        _ => f64::INFINITY,
    };
    let t = reach(from.x, along.x, rect.x0, rect.x1).min(reach(from.y, along.y, rect.y0, rect.y1));
    t.is_finite().then(|| from + along * t)
}

/// [`LensDistortion`] from `lens-distortion`'s values.
pub(crate) fn lens_distortion_plan(values: &[ParamValue]) -> LensDistortion {
    LensDistortion {
        // Just short of a half turn, where the focal length would be 0.
        degrees: f64::from(number(values, 0)).min(179.9),
        center: point(values, 1),
        remove: choice(values, 2) == 1,
        field: match choice(values, 3) {
            0 => LensField::Horizontal,
            1 => LensField::Vertical,
            _ => LensField::Diagonal,
        },
        grow: match choice(values, 4) {
            1 => 2.0,
            2 => 4.0,
            _ => 1.0,
        },
    }
}

/// `lens-distortion`: the layer bent about `center` as a lens whose field
/// of view, `field-of-view` degrees, spans the box's width, height or
/// diagonal (`field`) draws it, or, with `distortion` `remove`, unbent as a
/// pinhole would draw what the lens drew ([`Lens`]). What it draws stays
/// in the box, or the box `grow` lets it reach ([`lens_box`]), and it reads
/// only the box.
pub(crate) fn lens_distortion(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let plan = lens_distortion_plan(values);
    let bounds = layer_box(pixels, space);
    let Some(lens) = Lens::new(&plan, bounds) else {
        return;
    };
    let remove = plan.remove;
    let shown = lens_box_of(bounds, &plan);
    remap(pixels, space, |[x, y]| {
        let p = Point::new(f64::from(x), f64::from(y));
        if !shown.contains(p) {
            return None;
        }
        let at = lens.read(p, remove)?;
        bounds.contains(at).then_some([at.x as f32, at.y as f32])
    });
}

/// Where each of `bezier-warp`'s twelve points rests, in fractions of the
/// layer's box: the corners, and the handles a third of the way along
/// each side, so that at rest each side is straight and the warp changes
/// nothing. In the order of its parameters: round the box clockwise from
/// the top-left corner.
const WARP_REST: [[f64; 2]; 12] = [
    [0.0, 0.0],
    [1.0 / 3.0, 0.0],
    [2.0 / 3.0, 0.0],
    [1.0, 0.0],
    [1.0, 1.0 / 3.0],
    [1.0, 2.0 / 3.0],
    [1.0, 1.0],
    [2.0 / 3.0, 1.0],
    [1.0 / 3.0, 1.0],
    [0.0, 1.0],
    [0.0, 2.0 / 3.0],
    [0.0, 1.0 / 3.0],
];

/// The surface `bezier-warp` lays the layer's box on: a patch bounded by
/// four cubic curves, each side of the box bent through its two handles,
/// filled between them by blending the opposite sides (a Coons patch).
struct Patch {
    /// The top side from the top-left corner, the right side from the
    /// top-right one, the bottom side from the bottom-left one and the
    /// left side from the top-left one, each as its four control points.
    top: [Point; 4],
    right: [Point; 4],
    bottom: [Point; 4],
    left: [Point; 4],
}

impl Patch {
    /// The patch of the values over `bounds`: each point rests where
    /// [`WARP_REST`] puts it and moves by its value.
    fn new(plan: &BezierWarp, bounds: Rect) -> Patch {
        let p: [Point; 12] = std::array::from_fn(|i| {
            let [dx, dy] = plan.moves[i];
            let [fx, fy] = WARP_REST[i];
            Point::new(
                bounds.x0 + fx * bounds.width() + f64::from(dx),
                bounds.y0 + fy * bounds.height() + f64::from(dy),
            )
        });
        Patch {
            top: [p[0], p[1], p[2], p[3]],
            right: [p[3], p[4], p[5], p[6]],
            bottom: [p[9], p[8], p[7], p[6]],
            left: [p[0], p[11], p[10], p[9]],
        }
    }

    /// The point at `u` across and `v` down, 0 to 1 each way; beyond them
    /// the blend runs on as the same polynomials.
    fn at(&self, u: f64, v: f64) -> Point {
        let curve = |c: &[Point; 4], t: f64| {
            let s = 1.0 - t;
            let w = [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t];
            c.iter()
                .zip(w)
                .fold(Vec2::ZERO, |sum, (p, w)| sum + p.to_vec2() * w)
        };
        let corners = (self.top[0].to_vec2() * (1.0 - u) + self.top[3].to_vec2() * u) * (1.0 - v)
            + (self.bottom[0].to_vec2() * (1.0 - u) + self.bottom[3].to_vec2() * u) * v;
        let across = curve(&self.top, u) * (1.0 - v) + curve(&self.bottom, u) * v;
        let down = curve(&self.left, v) * (1.0 - u) + curve(&self.right, v) * u;
        (across + down - corners).to_point()
    }
}

/// How many pieces `bezier-warp` cuts each side of its patch into: enough
/// that a piece drawn flat strays from the curved patch by well under a
/// pixel for patches of a few thousand px.
const WARP_PIECES: usize = 128;

/// [`BezierWarp`] from `bezier-warp`'s values.
pub(crate) fn bezier_warp_plan(values: &[ParamValue]) -> BezierWarp {
    BezierWarp {
        moves: std::array::from_fn(|i| point(values, i)),
    }
}

/// `bezier-warp`: the layer's box laid on the [`Patch`] its twelve points
/// bound, each point moved by its value from where it rests on the box
/// ([`WARP_REST`]): what lies `u` across and `v` down the box shows `u`
/// across and `v` down the patch. Where the patch folds over itself, the
/// part nearer its bottom right shows. Its outline is smoothed over a
/// pixel.
pub(crate) fn bezier_warp(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let bounds = layer_box(pixels, space);
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return;
    }
    let patch = Patch::new(&bezier_warp_plan(values), bounds);
    let to = space.transform;
    // The patch on the grid, with its derivatives.
    let on_grid = |u: f64, v: f64| to * patch.at(u, v);
    let speed = |u: f64, v: f64| {
        let h = 1e-3;
        (
            (on_grid(u + h, v) - on_grid(u - h, v)).hypot() / (2.0 * h),
            (on_grid(u, v + h) - on_grid(u, v - h)).hypot() / (2.0 * h),
        )
    };
    // A margin past each side, a pixel and a half wide on the grid, so the
    // outline's smoothing has room outside it.
    let slowest = (0..=16)
        .flat_map(|i| {
            let t = i as f64 / 16.0;
            [speed(0.0, t), speed(1.0, t), speed(t, 0.0), speed(t, 1.0)]
        })
        .fold((f64::INFINITY, f64::INFINITY), |(a, b), (u, v)| {
            (a.min(u), b.min(v))
        });
    let margin = |s: f64| (1.5 / s.max(1e-6)).min(0.25);
    let (mu, mv) = (margin(slowest.0), margin(slowest.1));
    let n = WARP_PIECES;
    let uv = |i: usize, j: usize| {
        (
            -mu + (1.0 + 2.0 * mu) * i as f64 / n as f64,
            -mv + (1.0 + 2.0 * mv) * j as f64 / n as f64,
        )
    };
    let corners: Vec<Point> = (0..=n)
        .flat_map(|j| (0..=n).map(move |i| (i, j)))
        .map(|(i, j)| {
            let (u, v) = uv(i, j);
            on_grid(u, v)
        })
        .collect();
    // Which point of the patch each pixel centre shows, piece by piece.
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let mut shows: Vec<Option<(f64, f64)>> = vec![None; width * height];
    for j in 0..n {
        for i in 0..n {
            let at = |di: usize, dj: usize| {
                let (u, v) = uv(i + di, j + dj);
                (corners[(j + dj) * (n + 1) + i + di], u, v)
            };
            let quad = [at(0, 0), at(1, 0), at(1, 1), at(0, 1)];
            for tri in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
                cover(&mut shows, width, height, tri);
            }
        }
    }
    let half = [
        to.as_coeffs()[0].hypot(to.as_coeffs()[1]),
        to.as_coeffs()[2].hypot(to.as_coeffs()[3]),
    ]
    .map(|s| 0.5 / s.max(1e-6));
    let inset = Rect::new(
        bounds.x0 + half[0].min(bounds.width() / 2.0),
        bounds.y0 + half[1].min(bounds.height() / 2.0),
        bounds.x1 - half[0].min(bounds.width() / 2.0),
        bounds.y1 - half[1].min(bounds.height() / 2.0),
    );
    let source = pixels.clone();
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        *pixel = match shows[i] {
            None => [0.0; 4],
            Some((u, v)) => {
                // How far inside the outline the pixel centre lies, in px.
                let (su, sv) = speed(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                let inside = (u.min(1.0 - u) * su).min(v.min(1.0 - v) * sv);
                let kept = (0.5 + inside).clamp(0.0, 1.0) as f32;
                // Read half a pixel inside the box at most, so its edge is
                // smoothed once, by the outline.
                let at = to
                    * Point::new(
                        (bounds.x0 + u * bounds.width()).clamp(inset.x0, inset.x1),
                        (bounds.y0 + v * bounds.height()).clamp(inset.y0, inset.y1),
                    );
                source.sample(at.x, at.y).map(|c| c * kept)
            }
        };
    }
}

/// Marks each pixel whose centre lies in the triangle `tri` of grid points
/// with the patch point there, `u` and `v` interpolated across it.
fn cover(
    shows: &mut [Option<(f64, f64)>],
    width: usize,
    height: usize,
    tri: [(Point, f64, f64); 3],
) {
    let [(a, ua, va), (b, ub, vb), (c, uc, vc)] = tri;
    let area = (b - a).cross(c - a);
    if area.abs() < 1e-12 {
        return;
    }
    let lo = |f: fn(&Point) -> f64| [a, b, c].iter().map(f).fold(f64::INFINITY, f64::min);
    let hi = |f: fn(&Point) -> f64| [a, b, c].iter().map(f).fold(f64::NEG_INFINITY, f64::max);
    let x0 = (lo(|p| p.x) - 0.5).ceil().max(0.0) as usize;
    let y0 = (lo(|p| p.y) - 0.5).ceil().max(0.0) as usize;
    let x1 = ((hi(|p| p.x) - 0.5).floor() + 1.0).clamp(0.0, width as f64) as usize;
    let y1 = ((hi(|p| p.y) - 0.5).floor() + 1.0).clamp(0.0, height as f64) as usize;
    // A hair of overlap so neighbouring triangles leave no gap.
    let eps = -1e-9;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
            let wa = (b - p).cross(c - p) / area;
            let wb = (c - p).cross(a - p) / area;
            let wc = 1.0 - wa - wb;
            if wa >= eps && wb >= eps && wc >= eps {
                shows[y * width + x] =
                    Some((wa * ua + wb * ub + wc * uc, wa * va + wb * vb + wc * vc));
            }
        }
    }
}

/// [`RepeatTile`] from `repeat-tile`'s values.
pub(crate) fn repeat_tile_plan(values: &[ParamValue]) -> RepeatTile {
    RepeatTile {
        reach: std::array::from_fn(|i| number(values, i)),
        tiling: match choice(values, 4) {
            1 => Tiling::Unfold,
            2 => Tiling::Brick,
            3 => Tiling::Empty,
            _ => Tiling::Repeat,
        },
    }
}

/// `repeat-tile`: the layer's box repeated around itself, as far as
/// `right`, `left`, `down` and `up` reach beyond it. With `repeat` every
/// tile is the box; with `unfold` every other column of tiles flips
/// across and every other row upside down, so neighbours meet at their
/// edges; with `brick` every other row shifts by half a tile; with `none`
/// the box only grows, for the effects after it, and nothing shows
/// outside the box.
pub(crate) fn repeat_tile(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let RepeatTile {
        reach: [right, left, down, up],
        tiling,
    } = repeat_tile_plan(values);
    let bounds = layer_box(pixels, space);
    let reach = Rect::new(
        bounds.x0 - f64::from(left),
        bounds.y0 - f64::from(up),
        bounds.x1 + f64::from(right),
        bounds.y1 + f64::from(down),
    );
    let (origin, size) = (
        [bounds.x0 as f32, bounds.y0 as f32],
        [bounds.width() as f32, bounds.height() as f32],
    );
    if size[0] <= 0.0 || size[1] <= 0.0 {
        return;
    }
    remap(pixels, space, |[x, y]| {
        if !reach.contains(Point::new(f64::from(x), f64::from(y))) {
            return None;
        }
        let row = ((y - origin[1]) / size[1]).floor();
        if tiling == Tiling::Empty && !bounds.contains(Point::new(f64::from(x), f64::from(y))) {
            return None;
        }
        let shift = match tiling == Tiling::Brick && row.rem_euclid(2.0) == 1.0 {
            true => size[0] / 2.0,
            false => 0.0,
        };
        let column = ((x - origin[0] - shift) / size[0]).floor();
        let along = [
            x - origin[0] - shift - column * size[0],
            y - origin[1] - row * size[1],
        ];
        let flips = [column, row].map(|n| tiling == Tiling::Unfold && n.rem_euclid(2.0) == 1.0);
        Some(std::array::from_fn(|i| {
            origin[i]
                + if flips[i] {
                    size[i] - along[i]
                } else {
                    along[i]
                }
        }))
    });
}

/// What the seed of `turbulent-displace`'s noise along y is crossed with,
/// so the two axes move apart.
pub const TURBULENCE_Y_SEED: u32 = 0x5bd1_e995;

/// [`TurbulentDisplace`] from `turbulent-displace`'s values.
pub(crate) fn turbulent_displace_plan(values: &[ParamValue]) -> TurbulentDisplace {
    use Turbulence::*;
    // In the order of `displacement`'s choices; any other moves as the
    // first.
    const TURBULENCES: [Turbulence; 9] = [
        Turbulent,
        TurbulentSmoother,
        Bulge,
        BulgeSmoother,
        Twist,
        TwistSmoother,
        Horizontal,
        Vertical,
        Cross,
    ];
    TurbulentDisplace {
        amount: number(values, 0) / 100.0,
        size: number(values, 1).max(1.0),
        offset: point(values, 2),
        complexity: number(values, 3).max(1.0),
        evolution: number(values, 4) / 360.0,
        seed: number(values, 5).max(0.0) as u32,
        turbulence: TURBULENCES
            .get(choice(values, 6))
            .copied()
            .unwrap_or(Turbulent),
        pinning: match choice(values, 7) {
            0 => Pinning::All,
            1 => Pinning::LeftRight,
            2 => Pinning::TopBottom,
            _ => Pinning::Free,
        },
    }
}

/// `turbulent-displace`: every pixel read from up to `amount`/100 of `size`
/// px away, in a direction a smooth noise of cells `size` px wide gives,
/// in `complexity` octaves moved by `evolution`. Pinned edges of the
/// layer's box stay put, the displacement fading in from them over a cell.
pub(crate) fn turbulent_displace(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let plan = turbulent_displace_plan(values);
    let TurbulentDisplace {
        amount,
        size,
        offset,
        complexity,
        evolution,
        seed,
        turbulence: kind,
        pinning: pins,
    } = plan;
    let bounds = layer_box(pixels, space);
    let (smoother, octaves) = (kind.smoother(), plan.octaves());
    let field = |p: [f32; 2], seed: u32| {
        let (mut sum, mut total, mut weight, mut scale) = (0.0f32, 0.0f32, 1.0f32, 1.0f32);
        for octave in 0..octaves {
            let share = (complexity - octave as f32).min(1.0) * if smoother { 0.5 } else { 1.0 };
            let at = [
                (p[0] - offset[0]) / size * scale,
                (p[1] - offset[1]) / size * scale,
                evolution,
            ];
            sum += (value_noise(at, seed.wrapping_add(octave), 0) - 0.5) * weight * share;
            total += weight * share;
            weight *= 0.5;
            scale *= 2.0;
        }
        if total > 0.0 { 2.0 * sum / total } else { 0.0 }
    };
    // How much of the displacement each axis keeps near pinned edges.
    let pinned = |[x, y]: [f32; 2]| -> [f32; 2] {
        let fade = |d: f32| (d / size).clamp(0.0, 1.0);
        let (x, y) = (f64::from(x), f64::from(y));
        let edges = [
            fade((x - bounds.x0) as f32).min(fade((bounds.x1 - x) as f32)),
            fade((y - bounds.y0) as f32).min(fade((bounds.y1 - y) as f32)),
        ];
        let all = edges[0].min(edges[1]);
        match pins {
            Pinning::All => [all, all],
            Pinning::LeftRight => [edges[0], edges[0]],
            Pinning::TopBottom => [edges[1], edges[1]],
            Pinning::Free => [1.0, 1.0],
        }
    };
    let reach = amount * size;
    // The slope of the noise across a cell, each way.
    let slope = |p: [f32; 2]| {
        let e = size / 8.0;
        let at = |dx: f32, dy: f32| field([p[0] + dx, p[1] + dy], seed);
        [
            (at(e, 0.0) - at(-e, 0.0)) / (2.0 * e) * size / 4.0,
            (at(0.0, e) - at(0.0, -e)) / (2.0 * e) * size / 4.0,
        ]
    };
    remap(pixels, space, |p| {
        let keep = pinned(p);
        let d = match kind {
            // Bulges: out along the noise's slope.
            Turbulence::Bulge | Turbulence::BulgeSmoother => slope(p),
            // Twists: across it, round its hills.
            Turbulence::Twist | Turbulence::TwistSmoother => {
                let [x, y] = slope(p);
                [-y, x]
            }
            Turbulence::Horizontal => [field(p, seed), 0.0],
            Turbulence::Vertical => [0.0, field(p, seed)],
            Turbulence::Cross => {
                let n = field(p, seed);
                [n, n]
            }
            Turbulence::Turbulent | Turbulence::TurbulentSmoother => {
                [field(p, seed), field(p, seed ^ TURBULENCE_Y_SEED)]
            }
        };
        Some([p[0] + d[0] * reach * keep[0], p[1] + d[1] * reach * keep[1]])
    });
}

#[cfg(test)]
#[path = "../tests/unit/distort.rs"]
mod tests;

/// What a displacement reads from the map, for each choice of
/// `horizontal-from` and `vertical-from` (each lists its default first).
const MAP_CHANNELS: [[MapLevel; 11]; 2] = {
    use MapLevel::*;
    [
        [
            Red, Green, Blue, Alpha, Luminance, Hue, Lightness, Saturation, Full, Half, Off,
        ],
        [
            Green, Red, Blue, Alpha, Luminance, Hue, Lightness, Saturation, Full, Half, Off,
        ],
    ]
};

impl MapLevel {
    /// The level of the map's premultiplied `pixel`.
    fn level(self, pixel: [f32; 4]) -> f32 {
        let of = |level: Level| level.of(pixel, Reading::Straight);
        match self {
            MapLevel::Red => of(Level::Red),
            MapLevel::Green => of(Level::Green),
            MapLevel::Blue => of(Level::Blue),
            MapLevel::Alpha => of(Level::Alpha),
            MapLevel::Luminance => of(Level::Luminance),
            MapLevel::Hue => of(Level::Hue),
            MapLevel::Lightness => of(Level::Lightness),
            MapLevel::Saturation => of(Level::Saturation),
            MapLevel::Full => 1.0,
            MapLevel::Half => 0.75,
            MapLevel::Off => 0.5,
        }
    }
}

/// How one box, a map's or a matte's, lies on a layer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lay {
    /// At its own size, its middle on the layer's.
    Center,
    /// Stretched over the layer's box, corner to corner.
    Stretch,
    /// At its own size from the layer's middle, repeated on every side.
    Tile,
}

impl Lay {
    /// The point of `map`, a box in the layer's own coordinates, that the
    /// layer's point `[x, y]` reads, with `map` laid on `target`, the
    /// layer's box.
    pub(crate) fn point(self, target: Rect, map: Rect, [x, y]: [f64; 2]) -> [f64; 2] {
        if self == Lay::Stretch {
            return [
                map.x0 + (x - target.x0) / target.width().max(1e-6) * map.width(),
                map.y0 + (y - target.y0) / target.height().max(1e-6) * map.height(),
            ];
        }
        let m = [
            x - target.center().x + map.center().x,
            y - target.center().y + map.center().y,
        ];
        match self {
            Lay::Tile if map.width() > 0.0 && map.height() > 0.0 => [
                map.x0 + (m[0] - map.x0).rem_euclid(map.width()),
                map.y0 + (m[1] - map.y0).rem_euclid(map.height()),
            ],
            _ => m,
        }
    }
}

/// The placements of `displacement-map`'s `behaviour`, in the order of
/// its choices.
const MAP_LAYS: [Lay; 3] = [Lay::Center, Lay::Stretch, Lay::Tile];

/// [`DisplacementMap`] from `displacement-map`'s values.
pub(crate) fn displacement_map_plan(values: &[ParamValue]) -> DisplacementMap {
    DisplacementMap {
        most: [number(values, 0), number(values, 1)],
        from: [
            MAP_CHANNELS[0][choice(values, 2)],
            MAP_CHANNELS[1][choice(values, 3)],
        ],
        lay: MAP_LAYS[choice(values, 4)],
        wrap: choice(values, 5) == 1,
    }
}

/// `displacement-map`: every pixel read from up to `horizontal` and
/// `vertical` px away, by levels of the map layer (its own drawing when it
/// names none) at the same place: 0.5 moves nothing, 1 the most one way
/// and 0 the other. The map's box sits centred on the layer's, stretched
/// over it, or tiled from it, as `behaviour` says; with `edges` `wrap`,
/// what moves past the layer's box comes back on the other side.
pub(crate) fn displacement_map(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let DisplacementMap {
        most: [most_x, most_y],
        from,
        lay,
        wrap,
    } = displacement_map_plan(values);
    let target = layer_box(pixels, space);
    let (map, map_box) = match &space.input {
        Some(input) => (input.pixels.clone(), input.bounds.unwrap_or(target)),
        None => (pixels.clone(), target),
    };
    let to = space.transform;
    remap(pixels, space, |p| {
        let [mx, my] = lay.point(target, map_box, p.map(f64::from));
        let there = to * Point::new(mx, my);
        let level = map.sample(there.x, there.y);
        let d = [
            (from[0].level(level) - 0.5) * 2.0 * most_x,
            (from[1].level(level) - 0.5) * 2.0 * most_y,
        ];
        let at = [p[0] + d[0], p[1] + d[1]];
        if !wrap {
            return Some(at);
        }
        let (origin, size) = (
            [target.x0 as f32, target.y0 as f32],
            [target.width() as f32, target.height() as f32],
        );
        Some(std::array::from_fn(|i| match size[i] > 0.0 {
            true => origin[i] + (at[i] - origin[i]).rem_euclid(size[i]),
            false => at[i],
        }))
    });
}
