//! The vector blur: the slope of a map's level, softened by Gaussians
//! summed exactly, and every pixel smeared along it
//! (`shaders/vector.wgsl`, the box passes of `shaders/boxpass.wgsl`).
//!
//! The streaks' lengths are each slope over the steepest, and their
//! directions the slope's, of a level of the straight colour: the map's
//! colour divided by its alpha. Where the map fades out that division
//! magnifies what the Gaussian rounds; the box passes sum their windows
//! exactly, so the GPU's streaks follow the CPU's to the float.

use nakkash_raster::{Space, VectorBlur, VectorLevel, VectorStreak};

use crate::Gpu;
use crate::grid::Grid;
use crate::kernels::{Params, pixel_groups};
use crate::pool::Kind as BufferKind;
use crate::record::Batch;

/// The number the kernels read `streak` by.
fn streak_row(streak: VectorStreak) -> u32 {
    match streak {
        VectorStreak::Natural => 0,
        VectorStreak::Constant => 1,
        VectorStreak::Across => 2,
        VectorStreak::Behind => 3,
        VectorStreak::Fading => 4,
    }
}

/// The number the kernels read `level` by.
fn level_row(level: VectorLevel) -> u32 {
    match level {
        VectorLevel::Red => 0,
        VectorLevel::Green => 1,
        VectorLevel::Blue => 2,
        VectorLevel::Alpha => 3,
        VectorLevel::Luminance => 4,
        VectorLevel::Lightness => 5,
        VectorLevel::Hue => 6,
        VectorLevel::Saturation => 7,
    }
}

impl Gpu {
    /// Records `plan` on `grid` in `space`, whose input, if any, is the map.
    pub(crate) fn vector(
        &self,
        batch: &mut Batch<'_>,
        grid: &mut Grid,
        plan: &VectorBlur,
        space: &Space,
    ) {
        if grid.width == 0 || grid.height == 0 || plan.amount == 0.0 {
            return;
        }
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        let mut map = match self.input(batch, space) {
            Some(map) => map,
            None => self.duplicate_grid(batch, grid),
        };
        if let Some(softening) = plan.softening() {
            self.blur(batch, &mut map, &softening);
        }
        // Each workgroup of `steepest` takes 4,096 pixels.
        let partials = count.div_ceil(4096);
        let (sin, cos) = plan.turn.sin_cos();
        let params = |p: Params| {
            p.u32(u32::from(grid.width))
                .u32(u32::from(grid.height))
                .u32(count)
                .u32(row)
                .u32(streak_row(plan.streak))
                .u32(level_row(plan.level))
                .u32(map.count())
                .u32(partials)
                .f32(plan.amount * plan.scale)
                .f32(sin)
                .f32(cos)
        };
        let params = params(Params::new());
        let dispatch = |batch: &mut Batch<'_>,
                        entry: &str,
                        written: &wgpu::Buffer,
                        reads: [&wgpu::Buffer; 3],
                        groups: (u32, u32)| {
            self.kernels.vector.dispatch(
                &self.device,
                batch.encoder(),
                entry,
                &params,
                written,
                &reads,
                groups,
            );
        };
        let mut field = self.grid(grid.width, grid.height);
        dispatch(
            batch,
            "slopes",
            field.buffer(),
            [map.buffer(), &self.empty, &self.empty],
            groups,
        );
        drop(map);
        if let Some(smoothing) = plan.smoothing() {
            self.blur(batch, &mut field, &smoothing);
        }
        let partial = self.buffer(16 * u64::from(partials), BufferKind::Storage);
        dispatch(
            batch,
            "steepest",
            partial.buffer(),
            [&self.empty, field.buffer(), &self.empty],
            (partials, 1),
        );
        let steepest = self.buffer(16, BufferKind::Storage);
        dispatch(
            batch,
            "most_of",
            steepest.buffer(),
            [&self.empty, &self.empty, partial.buffer()],
            (1, 1),
        );
        let out = self.grid(grid.width, grid.height);
        dispatch(
            batch,
            "smear",
            out.buffer(),
            [grid.buffer(), field.buffer(), steepest.buffer()],
            groups,
        );
        grid.pixels = out.pixels;
    }
}
