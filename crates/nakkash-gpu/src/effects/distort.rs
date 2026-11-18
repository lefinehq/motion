//! The distortions: every pixel redrawn from the point of the layer its
//! plan gives (`distort.wgsl`).

use nakkash_raster::kurbo::{Point, Rect, Vec2};
use nakkash_raster::{
    DisplacementMap, Distortion, LUMA, Lay, LensDistortion, MapLevel, Mirror, MotionTile,
    PolarCoordinates, RepeatTile, Space, TURBULENCE_Y_SEED, Tiling, Turbulence, TurbulentDisplace,
    WARP_PIECES, WarpMesh, Wrap, distortion_box,
};

use crate::Gpu;
use crate::f32_bytes;
use crate::grid::Grid;
use crate::kernels::{Params, pixel_groups};
use crate::pool::{Kind as BufferKind, Pooled};
use crate::record::Batch;

/// Whether the kernels run `distortion` as the CPU does, for every value
/// it may hold: all of them do.
pub(crate) fn runs(distortion: &Distortion) -> bool {
    let _ = distortion;
    true
}

/// How many rows of one triangle one invocation of the cover kernel
/// covers.
const COVER_BAND: u32 = 16;

/// The least 32-bit float at or above `v`: `x >= v` and `x < v` for a
/// 32-bit `x` are `x >= up(v)` and `x < up(v)`.
fn up(v: f64) -> f32 {
    let f = v as f32;
    if f64::from(f) < v { f.next_up() } else { f }
}

/// `r` as the kernels test `Rect::contains`.
fn inside(r: Rect) -> [f32; 4] {
    [r.x0, r.y0, r.x1, r.y1].map(up)
}

/// The uniform block of the distort family (`distort.wgsl`), field by
/// field.
#[derive(Debug, Clone)]
struct DistortParams {
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    map: [u32; 2],
    choices: [u32; 6],
    exact: bool,
    back: [f32; 4],
    shift: [f32; 4],
    to: [f32; 4],
    origin: [f32; 4],
    inside: [f32; 4],
    v: [[f32; 4]; 5],
}

impl DistortParams {
    /// The block for `grid` in `space`, whose box is `bounds`.
    fn new(grid: &Grid, space: &Space, bounds: Rect) -> DistortParams {
        let count = grid.count();
        let (_, row) = pixel_groups(count);
        let back = space.transform.inverse();
        let [a, b, c, d, ..] = back.as_coeffs();
        let shift = back * Point::new(0.5, 0.5);
        let [ta, tb, tc, td, ..] = space.transform.as_coeffs();
        DistortParams {
            count,
            row,
            width: u32::from(grid.width),
            height: u32::from(grid.height),
            map: [u32::from(grid.width), u32::from(grid.height)],
            choices: [0; 6],
            exact: false,
            back: [a, b, c, d].map(|v| v as f32),
            shift: [shift.x as f32, shift.y as f32, 0.0, 0.0],
            to: [ta, tb, tc, td].map(|v| v as f32),
            // As the CPU rounds the box's origin and size.
            origin: [
                bounds.x0 as f32,
                bounds.y0 as f32,
                bounds.width() as f32,
                bounds.height() as f32,
            ],
            inside: inside(bounds),
            v: [[0.0; 4]; 5],
        }
    }

    fn params(&self) -> Params {
        let mut params = Params::new()
            .u32(self.count)
            .u32(self.row)
            .u32(self.width)
            .u32(self.height)
            .u32(self.map[0])
            .u32(self.map[1]);
        for choice in self.choices {
            params = params.u32(choice);
        }
        params = params.u32(u32::from(self.exact));
        params = params
            .vec4(self.back)
            .vec4(self.shift)
            .vec4(self.to)
            .vec4(self.origin)
            .vec4(self.inside);
        for v in self.v {
            params = params.vec4(v);
        }
        params
    }
}

/// The number the kernel reads `level` by.
fn level_row(level: MapLevel) -> u32 {
    match level {
        MapLevel::Red => 0,
        MapLevel::Green => 1,
        MapLevel::Blue => 2,
        MapLevel::Alpha => 3,
        MapLevel::Luminance => 4,
        MapLevel::Hue => 5,
        MapLevel::Lightness => 6,
        MapLevel::Saturation => 7,
        MapLevel::Full => 8,
        MapLevel::Half => 9,
        MapLevel::Off => 10,
    }
}

/// The number the kernel reads `turbulence` by: its choice's.
fn turbulence_row(turbulence: Turbulence) -> u32 {
    match turbulence {
        Turbulence::Turbulent => 0,
        Turbulence::TurbulentSmoother => 1,
        Turbulence::Bulge => 2,
        Turbulence::BulgeSmoother => 3,
        Turbulence::Twist => 4,
        Turbulence::TwistSmoother => 5,
        Turbulence::Horizontal => 6,
        Turbulence::Vertical => 7,
        Turbulence::Cross => 8,
    }
}

/// The largest a part of a layer point may be for [`centres`]: their sum
/// stays under 2^22, which the kernels' rounding holds to.
const MOST_PART: f64 = (1u32 << 21) as f64;

/// The layer point of every pixel's centre as `remap` works it out, in
/// parts the kernels add (`table` in `distort.wgsl`): `back` applied to
/// each column's centre and 0, then to 0 and each row's centre, each x
/// and y as a 64-bit fixed-point number of 2^-32 px, as bits of floats.
/// `None` when a part is not finite or too large, which leaves the kernels
/// to work points out in 32-bit floats.
fn centres(grid: &Grid, space: &Space) -> Option<Vec<f32>> {
    let back = space.transform.inverse();
    let [a, b, c, d, e, f] = back.as_coeffs();
    let (width, height) = (usize::from(grid.width), usize::from(grid.height));
    let mut table = Vec::with_capacity(4 * (width + height));
    let parts = (0..width)
        .map(|x| {
            let px = x as f64 + 0.5;
            [a * px + e, b * px + f]
        })
        .chain((0..height).map(|y| {
            let py = y as f64 + 0.5;
            [c * py, d * py]
        }));
    for part in parts {
        for v in part {
            if !v.is_finite() || v.abs() >= MOST_PART {
                return None;
            }
            let fixed = (v * 4_294_967_296.0).round() as i64;
            table.push(f32::from_bits((fixed >> 32) as u32));
            table.push(f32::from_bits(fixed as u32));
        }
    }
    Some(table)
}

/// `v` of 32-bit floats.
fn f32s(v: Vec2) -> [f32; 2] {
    [v.x as f32, v.y as f32]
}

impl Gpu {
    /// Records `distortion` on `grid` in `space`.
    pub(crate) fn distort(
        &self,
        batch: &mut Batch<'_>,
        grid: &mut Grid,
        distortion: &Distortion,
        space: &Space,
    ) {
        let bounds = distortion_box(space, grid.width, grid.height);
        let mut p = DistortParams::new(grid, space, bounds);
        let mut map = None;
        let mut mesh = None;
        let entry = match distortion {
            Distortion::MotionTile(plan) => {
                motion_tile(&mut p, plan);
                "motion_tile"
            }
            Distortion::Wrap(plan) => {
                wrap(&mut p, plan);
                "wrap"
            }
            Distortion::Mirror(plan) => {
                mirror(&mut p, plan);
                "mirror"
            }
            Distortion::PolarCoordinates(plan) => {
                polar_coordinates(&mut p, plan);
                "polar_coordinates"
            }
            Distortion::LensDistortion(plan) => {
                if !lens_distortion(&mut p, plan, bounds) {
                    return;
                }
                "lens_distortion"
            }
            Distortion::RepeatTile(plan) => {
                if !repeat_tile(&mut p, plan, bounds) {
                    return;
                }
                "repeat_tile"
            }
            Distortion::TurbulentDisplace(plan) => {
                turbulent_displace(&mut p, plan, bounds);
                "turbulent_displace"
            }
            Distortion::DisplacementMap(plan) => {
                map = self.input(batch, space);
                let map_box = match (&space.input, &map) {
                    (Some(input), Some(held)) => {
                        p.map = [u32::from(held.width), u32::from(held.height)];
                        input.bounds.unwrap_or(bounds)
                    }
                    _ => bounds,
                };
                displacement_map(&mut p, plan, bounds, map_box, space);
                "displacement_map"
            }
            Distortion::BezierWarp(plan) => {
                let Some(warp) = plan.mesh(bounds, space.transform) else {
                    return;
                };
                mesh = Some(self.cover(batch, grid, &warp, bounds, &mut p));
                "bezier_warp"
            }
        };
        let table = centres(grid, space).map(|table| self.values(batch, &table));
        p.exact = table.is_some();
        let out = self.grid(grid.width, grid.height);
        let (groups, _) = pixel_groups(p.count);
        self.kernels.distort.dispatch(
            &self.device,
            batch.encoder(),
            entry,
            &p.params(),
            out.buffer(),
            &[
                grid.buffer(),
                map.as_ref().unwrap_or(grid).buffer(),
                table.as_ref().map_or(&self.empty, Pooled::buffer),
                mesh.as_ref().map_or(&self.empty, Pooled::buffer),
            ],
            groups,
        );
        grid.pixels = out.pixels;
    }
}

impl Gpu {
    /// Records `warp`'s mesh, over `bounds`, laid on `grid`: its words
    /// copied into a buffer and the pixels each triangle holds marked
    /// (`cover.wgsl`), and the draw's values set in `p`.
    fn cover(
        &self,
        batch: &mut Batch<'_>,
        grid: &Grid,
        warp: &WarpMesh,
        bounds: Rect,
        p: &mut DistortParams,
    ) -> Pooled {
        let n = u32::try_from(WARP_PIECES).expect("a few pieces");
        // The patch's points less the box's middle, which its speed does
        // not see, so they keep more of their digits in 32 bits.
        let middle = bounds.center();
        let mut words: Vec<f32> = warp.points.iter().flat_map(|q| f32s(*q - middle)).collect();
        let corners = u32::try_from(words.len()).expect("twelve points");
        words.extend(warp.corners.iter().flat_map(|q| [q.x as f32, q.y as f32]));
        let shows = u32::try_from(words.len()).expect("a mesh of a few thousand corners");
        let count = grid.count();
        let buffer = self.buffer(
            4 * (u64::from(shows) + u64::from(count)),
            BufferKind::Storage,
        );
        batch.stage(buffer.buffer(), f32_bytes(&words));
        batch.encoder().clear_buffer(
            buffer.buffer(),
            4 * u64::from(shows),
            Some(4 * u64::from(count)),
        );
        let triangles = 2 * n * n;
        let (groups, _) = pixel_groups(triangles);
        let bands = u32::from(grid.height).div_ceil(COVER_BAND).max(1);
        let params = Params::new()
            .u32(triangles)
            .u32(n)
            .u32(u32::from(grid.width))
            .u32(u32::from(grid.height))
            .u32(corners)
            .u32(shows)
            .u32(COVER_BAND);
        self.kernels.cover.dispatch(
            &self.device,
            batch.encoder(),
            "warp_cover",
            &params,
            buffer.buffer(),
            &[],
            (groups.0, bands),
        );
        p.v[0] = p.origin;
        let r = warp.inset;
        p.v[1] = [r.x0, r.y0, r.x1, r.y1].map(|v| v as f32);
        p.v[2] = [warp.margin[0] as f32, warp.margin[1] as f32, 0.0, 0.0];
        p.choices[0] = n;
        p.choices[1] = shows;
        buffer
    }
}

/// `distort::motion_tile`'s values on the box.
fn motion_tile(p: &mut DistortParams, plan: &MotionTile) {
    let [ox, oy, sw, sh] = p.origin;
    let (origin, size) = ([ox, oy], [sw, sh]);
    let tile = [size[0] * plan.tile[0], size[1] * plan.tile[1]].map(|t| t.max(1e-3));
    let output = [size[0] * plan.output[0], size[1] * plan.output[1]];
    let middle = [origin[0] + size[0] / 2.0, origin[1] + size[1] / 2.0];
    p.v[0] = [plan.center[0], plan.center[1], plan.phase, 0.0];
    p.v[1] = [tile[0], tile[1], output[0], output[1]];
    p.v[2] = [middle[0], middle[1], 0.0, 0.0];
    p.choices[0] = u32::from(plan.mirror);
    p.choices[1] = u32::from(plan.rows);
}

/// `distort::wrap`'s values on the box.
fn wrap(p: &mut DistortParams, plan: &Wrap) {
    let [ox, oy, sw, sh] = p.origin;
    let shift = [plan.to[0] - ox - sw / 2.0, plan.to[1] - oy - sh / 2.0];
    p.v[0] = [shift[0], shift[1], plan.blend, 0.0];
}

/// `distort::mirror`'s values.
fn mirror(p: &mut DistortParams, plan: &Mirror) {
    p.v[0] = [plan.center[0], plan.center[1], plan.sin, plan.cos];
}

/// `distort::polar_coordinates`' values on the box.
fn polar_coordinates(p: &mut DistortParams, plan: &PolarCoordinates) {
    let [left, top, width, height] = p.origin;
    let middle = [left + width / 2.0, top + height / 2.0];
    p.v[0] = [left, top, width, height];
    p.v[1] = [middle[0], middle[1], plan.amount, 0.0];
    p.choices[0] = u32::from(plan.unroll);
}

/// `distort::lens_distortion`'s values on `bounds`; false when it bends
/// nothing.
fn lens_distortion(p: &mut DistortParams, plan: &LensDistortion, bounds: Rect) -> bool {
    let Some(focal) = plan.focal(bounds) else {
        return false;
    };
    let [x, y] = plan.center;
    p.v[0] = [x, y, focal as f32, 0.0];
    p.v[1] = inside(plan.shown(bounds));
    p.choices[0] = u32::from(plan.remove);
    true
}

/// `distort::repeat_tile`'s values on `bounds`; false when the box has no
/// area, which leaves the pixels as they are.
fn repeat_tile(p: &mut DistortParams, plan: &RepeatTile, bounds: Rect) -> bool {
    let [right, left, down, up] = plan.reach;
    let reach = Rect::new(
        bounds.x0 - f64::from(left),
        bounds.y0 - f64::from(up),
        bounds.x1 + f64::from(right),
        bounds.y1 + f64::from(down),
    );
    if p.origin[2] <= 0.0 || p.origin[3] <= 0.0 {
        return false;
    }
    p.v[0] = inside(reach);
    p.choices[0] = match plan.tiling {
        Tiling::Repeat => 0,
        Tiling::Unfold => 1,
        Tiling::Brick => 2,
        Tiling::Empty => 3,
    };
    true
}

/// `distort::turbulent_displace`'s values on `bounds`.
fn turbulent_displace(p: &mut DistortParams, plan: &TurbulentDisplace, bounds: Rect) {
    let reach = plan.amount * plan.size;
    p.v[0] = [plan.offset[0], plan.offset[1], plan.size, plan.complexity];
    p.v[1] = [plan.evolution, reach, 0.0, 0.0];
    p.v[2] = [bounds.x0, bounds.y0, bounds.x1, bounds.y1].map(|v| v as f32);
    p.choices = [
        turbulence_row(plan.turbulence),
        plan.octaves(),
        u32::from(plan.turbulence.smoother()),
        plan.seed,
        plan.seed ^ TURBULENCE_Y_SEED,
        match plan.pinning {
            nakkash_raster::Pinning::All => 0,
            nakkash_raster::Pinning::LeftRight => 1,
            nakkash_raster::Pinning::TopBottom => 2,
            nakkash_raster::Pinning::Free => 3,
        },
    ];
}

/// `distort::displacement_map`'s values on `target`, the layer's box, with
/// the map's box `map`.
fn displacement_map(
    p: &mut DistortParams,
    plan: &DisplacementMap,
    target: Rect,
    map: Rect,
    space: &Space,
) {
    let linear = |v: Vec2| {
        let to = space.transform;
        f32s(to * v.to_point() - to * Point::ZERO)
    };
    let centred = linear(map.center() - target.center());
    p.v[0] = [plan.most[0], plan.most[1], centred[0], centred[1]];
    // Stretched, the map's point is `map.x0 + (x - target.x0) · s` for the
    // layer's `x`: `x` plus `c + a · (s - 1)`, `a` the 32-bit difference
    // from the box's rounded left, so only `a`'s rounding is multiplied,
    // and by `s - 1`.
    let stretch = [
        map.width() / target.width().max(1e-6),
        map.height() / target.height().max(1e-6),
    ];
    let left = [target.x0 as f32, target.y0 as f32];
    let lean = [
        map.x0 - target.x0 + (f64::from(left[0]) - target.x0) * (stretch[0] - 1.0),
        map.y0 - target.y0 + (f64::from(left[1]) - target.y0) * (stretch[1] - 1.0),
    ];
    p.v[1] = [
        left[0],
        left[1],
        (stretch[0] - 1.0) as f32,
        (stretch[1] - 1.0) as f32,
    ];
    p.v[2] = [
        lean[0] as f32,
        lean[1] as f32,
        map.width() as f32,
        map.height() as f32,
    ];
    let k = target.center() - map.center() + map.origin().to_vec2();
    let j = map.center() - target.center();
    p.v[4] = [k.x as f32, k.y as f32, j.x as f32, j.y as f32];
    let [r, g, b] = LUMA;
    p.v[3] = [r, g, b, 0.0];
    let tiled = map.width() > 0.0 && map.height() > 0.0;
    p.choices = [
        level_row(plan.from[0]),
        level_row(plan.from[1]),
        match plan.lay {
            Lay::Center => 0,
            Lay::Stretch => 1,
            Lay::Tile if tiled => 2,
            Lay::Tile => 0,
        },
        u32::from(plan.wrap),
        0,
        0,
    ];
}
