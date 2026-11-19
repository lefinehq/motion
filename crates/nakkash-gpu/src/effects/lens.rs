//! The lens blur: the exact running sums of every row of the layer, then
//! every pixel the mean of the layer over its iris (`shaders/lens.wgsl`).
//! At one radius the iris's rows are the CPU's own, worked out once; with
//! a blur map each pixel works out its own from the iris's edges.

use nakkash_raster::kurbo::{Affine, Point, Vec2};
use nakkash_raster::{IRIS_ROWS, LensBlur, LensLevel, LensMap, Space};

use crate::Gpu;
use crate::grid::Grid;
use crate::kernels::{LINE_GROUP, Params, pixel_groups};
use crate::pool::Kind as BufferKind;
use crate::record::Batch;

/// Pixels a chunk of a row, summed by one invocation.
const CHUNK: u32 = 64;

/// The number the kernels read `level` by.
fn level_row(level: LensLevel) -> u32 {
    match level {
        LensLevel::Luminance => 0,
        LensLevel::Red => 1,
        LensLevel::Green => 2,
        LensLevel::Blue => 3,
        LensLevel::Alpha => 4,
    }
}

/// A whole number as a float's bits, held to what the kernels add to it.
fn whole(v: f64) -> f32 {
    f32::from_bits((v.clamp(-1e9, 1e9) as i32) as u32)
}

/// From the centre of px (x, y) of a `width` by `height` grid to the
/// point of the map it reads, as `lens_blur` reads it: through the
/// layer's coordinates, onto the map's box, and back to the grid.
fn map_affine(map: &LensMap, width: u16, height: u16) -> Affine {
    let target = map.target(width, height);
    let map_box = map.map_bounds.unwrap_or(target);
    let onto = match map.stretch {
        true => {
            Affine::translate((map_box.x0, map_box.y0))
                * Affine::scale_non_uniform(
                    map_box.width() / target.width().max(1e-6),
                    map_box.height() / target.height().max(1e-6),
                )
                * Affine::translate((-target.x0, -target.y0))
        }
        false => Affine::translate(map_box.center() - target.center()),
    };
    map.transform * onto * map.transform.inverse()
}

impl Gpu {
    /// Records `plan` on `grid` in `space`, whose input, if any, is the
    /// blur map.
    pub(crate) fn lens(
        &self,
        batch: &mut Batch<'_>,
        grid: &mut Grid,
        plan: &LensBlur,
        space: &Space,
    ) {
        let (width, height) = (u32::from(grid.width), u32::from(grid.height));
        if width == 0 || height == 0 {
            return;
        }
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        let chunks = width.div_ceil(CHUNK);
        let map = plan.map.as_ref().zip(self.input(batch, space));
        // Without a map, the iris's rows at its one radius, as the CPU
        // works them out, and their weight.
        let mut rows = Vec::new();
        let mut weight = 0.0f64;
        if map.is_none() && plan.radius >= 0.05 {
            plan.iris.rows(plan.radius, &mut rows);
            for r in &rows {
                weight += r.share * (r.right - r.left);
            }
        }
        let table: Vec<f32> = match &map {
            Some(_) => plan
                .iris
                .edges
                .iter()
                .flat_map(|[l, r]| [*l as f32, *r as f32, 0.0, 0.0])
                .collect(),
            None => rows
                .iter()
                .flat_map(|r| {
                    let (left, right) = (0.5 + r.left, 0.5 + r.right);
                    let (l, rr) = (left.floor(), right.floor());
                    [
                        whole(r.dy as f64),
                        whole(l),
                        whole(rr),
                        r.share as f32,
                        (left - l) as f32,
                        (right - rr) as f32,
                        0.0,
                        0.0,
                    ]
                })
                .collect(),
        };
        debug_assert_eq!(
            IRIS_ROWS,
            plan.iris.edges.len(),
            "the kernels read IRIS_ROWS edges"
        );
        let table = self.values(batch, if table.is_empty() { &[0.0; 4] } else { &table });
        let mut params = Params::new()
            .u32(width)
            .u32(height)
            .u32(count)
            .u32(row)
            .u32(CHUNK)
            .u32(chunks)
            .u32(u32::from(plan.repeat))
            .u32(u32::from(map.is_some()))
            .u32(rows.len() as u32);
        let mut placed = ([0.0f32; 4], [0.0f32; 4], [0.0f32; 4], [0.0f32; 4]);
        match &map {
            Some((lens_map, input)) => {
                let a = map_affine(lens_map, grid.width, grid.height);
                let [ca, cb, cc, cd, ..] = a.as_coeffs();
                let base = a * Point::new(0.5, 0.5) - Vec2::new(0.5, 0.5);
                let (wx, wy) = (base.x.floor(), base.y.floor());
                // Steps as whole numbers over 2^shift, their sum over the
                // grid within an i32.
                let reach =
                    (ca.abs() + cc.abs()).max(cb.abs() + cd.abs()) * f64::from(width.max(height));
                let mut shift = 0u32;
                while shift < 20 && f64::from(1u32 << (shift + 1)) * reach <= f64::from(1u32 << 30)
                {
                    shift += 1;
                }
                let scale = f64::from(1u32 << shift);
                let fits = scale * reach <= f64::from(1u32 << 30);
                let hi = [ca, cb, cc, cd].map(|c| if fits { (c * scale).round() } else { 0.0 });
                placed = (
                    [whole(wx), whole(wy), 0.0, 0.0],
                    [(base.x - wx) as f32, (base.y - wy) as f32, 0.0, 0.0],
                    hi.map(whole),
                    [ca, cb, cc, cd]
                        .iter()
                        .zip(hi)
                        .map(|(c, h)| (c - h / scale) as f32)
                        .collect::<Vec<_>>()
                        .try_into()
                        .expect("four"),
                );
                params = params
                    .u32(level_row(lens_map.level))
                    .u32(u32::from(lens_map.inverted))
                    .u32(u32::from(input.width))
                    .u32(u32::from(input.height))
                    .u32(shift);
            }
            None => params = params.u32(0).u32(0).u32(0).u32(0).u32(0),
        }
        let focal = plan.map.as_ref().map_or(0.0, |m| m.focal);
        let params = params
            .u32(0)
            .u32(0)
            .f32(plan.radius as f32)
            .f32(focal as f32)
            .f32(weight as f32)
            .f32(plan.iris.top as f32)
            .f32(plan.iris.bottom as f32)
            .f32(0.0)
            .f32(0.0)
            .f32(0.0)
            .vec4(placed.0)
            .vec4(placed.1)
            .vec4(placed.2)
            .vec4(placed.3);
        let dispatch = |batch: &mut Batch<'_>,
                        entry: &str,
                        written: &wgpu::Buffer,
                        reads: [Option<&wgpu::Buffer>; 5],
                        groups: (u32, u32)| {
            self.kernels.lens.dispatch(
                &self.device,
                batch.encoder(),
                entry,
                &params,
                written,
                &reads.map(|read| read.unwrap_or(&self.empty)),
                groups,
            );
        };
        // Every row's running sums.
        let scratch = self.buffer(
            32 * u64::from(height) * u64::from(chunks),
            BufferKind::Storage,
        );
        let sums = self.buffer(
            16 * u64::from(height) * u64::from(width + 1),
            BufferKind::Storage,
        );
        let line_groups = (height.div_ceil(LINE_GROUP), chunks);
        let source = Some(grid.buffer());
        dispatch(
            batch,
            "row_chunks",
            scratch.buffer(),
            [source, None, None, None, None],
            line_groups,
        );
        dispatch(
            batch,
            "row_scan",
            scratch.buffer(),
            [None, None, None, None, None],
            (height.div_ceil(LINE_GROUP), 1),
        );
        dispatch(
            batch,
            "row_sums",
            sums.buffer(),
            [source, Some(scratch.buffer()), None, None, None],
            line_groups,
        );
        let out = self.grid(grid.width, grid.height);
        dispatch(
            batch,
            "spread",
            out.buffer(),
            [
                source,
                None,
                Some(sums.buffer()),
                Some(table.buffer()),
                map.as_ref().map(|(_, input)| input.buffer()),
            ],
            groups,
        );
        grid.pixels = out.pixels;
    }
}
