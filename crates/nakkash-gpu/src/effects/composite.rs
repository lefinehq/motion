//! Compositing inside effects: an effect's result combined with the
//! layer's pixels by any of the operations ([`nakkash_raster::OPERATIONS`]),
//! and the solid composite.

use nakkash_raster::{Operation, SolidComposite};

use crate::Gpu;
use crate::grid::Grid;
use crate::kernels::{OperateParams, mode_row, pixel_groups};
use crate::record::Batch;

/// The number and blend mode the kernels read `operation` by.
pub(crate) fn operation_row(operation: Operation) -> (u32, u32) {
    match operation {
        Operation::Replace => (0, 0),
        Operation::Blend(mode) => (1, mode_row(mode)),
        Operation::Dissolve => (2, 0),
        Operation::PremultipliedOver => (7, 0),
        Operation::AlphaAdd => (8, 0),
    }
}

impl Gpu {
    /// Records the operate family's `entry` on `written`, reading `read`,
    /// with the parameters `params` sets on the grid's own.
    pub(crate) fn operate_kernel(
        &self,
        batch: &mut Batch<'_>,
        entry: &str,
        written: &Grid,
        read: Option<&Grid>,
        params: impl FnOnce(OperateParams) -> OperateParams,
    ) {
        let count = written.count();
        let (groups, row) = pixel_groups(count);
        let params = params(OperateParams::new(count, row, u32::from(written.width)));
        self.kernels.operate.dispatch(
            &self.device,
            batch.encoder(),
            entry,
            &params.params(),
            written.buffer(),
            &[read.map_or(&self.empty, Grid::buffer)],
            groups,
        );
    }

    /// Records `over` combined with the pixels `under` by `operation`, in
    /// place, as `Operation::pixels` does.
    pub(crate) fn operate(
        &self,
        batch: &mut Batch<'_>,
        under: &Grid,
        over: &Grid,
        operation: Operation,
    ) {
        self.operate_kernel(batch, "operate", under, Some(over), |p| {
            p.operation(operation)
        });
    }

    /// Records `solid` on `grid`.
    pub(crate) fn solid_composite(
        &self,
        batch: &mut Batch<'_>,
        grid: &Grid,
        solid: &SolidComposite,
    ) {
        self.operate_kernel(batch, "solid_composite", grid, None, |p| {
            p.operation(solid.operation)
                .source(solid.source)
                .solid(solid.color, solid.opacity)
                .cover(solid.cover.as_ref())
        });
    }
}
