//! The bevel: how far inside the layer's outline each pixel lies, within
//! a reach just past the bevel's thickness, and every pixel lit by the
//! slope of the surface it raises (`shaders/bevel.wgsl`).

use nakkash_raster::Bevel;

use crate::Gpu;
use crate::grid::Grid;
use crate::kernels::{Params, pixel_groups};
use crate::record::Batch;

impl Gpu {
    /// Records `plan` on `grid`.
    pub(crate) fn bevel(&self, batch: &mut Batch<'_>, grid: &mut Grid, plan: &Bevel) {
        if plan.none() || grid.count() == 0 {
            return;
        }
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        // A pixel this far inside or farther is on the flat top: its depth,
        // less half a px, is past the thickness.
        let reach = (plan.thickness.ceil() + 1.0).min(f64::from(u16::MAX)) as u32;
        let [r, g, b] = plan.light;
        let params = Params::new()
            .u32(u32::from(grid.width))
            .u32(u32::from(grid.height))
            .u32(count)
            .u32(row)
            .u32(reach)
            .u32(reach.saturating_mul(reach))
            .f32(plan.thickness as f32)
            .f32(plan.intensity)
            .vec4([plan.toward[0] as f32, plan.toward[1] as f32, 0.0, 0.0])
            .vec4([r, g, b, 0.0]);
        let dispatch =
            |batch: &mut Batch<'_>, entry: &str, written: &Grid, distances: Option<&Grid>| {
                self.kernels.bevel.dispatch(
                    &self.device,
                    batch.encoder(),
                    entry,
                    &params,
                    written.buffer(),
                    &[grid.buffer(), distances.map_or(&self.empty, Grid::buffer)],
                    groups,
                );
            };
        let columns = self.grid(grid.width, grid.height);
        dispatch(batch, "columns", &columns, None);
        let rows = self.grid(grid.width, grid.height);
        dispatch(batch, "rows", &rows, Some(&columns));
        let out = self.grid(grid.width, grid.height);
        dispatch(batch, "light", &out, Some(&rows));
        grid.pixels = out.pixels;
    }
}
