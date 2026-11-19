//! Layer masks on the GPU: each mask's outline, as the rasteriser drew it,
//! grown or shrunk, feathered and combined with the masks before it, as
//! `mask.rs` of the rasteriser does on the CPU. The coverage stays on the
//! GPU, in every channel of a grid, for the layer it cuts.

use nakkash_raster::{Blur, Edges, MaskCoverage, MaskMode, Resident};

use crate::Gpu;
use crate::grid::grid;
use crate::kernels::{Params, pixel_groups};
use crate::pool::Kind as BufferKind;

/// The number `mask.wgsl`'s `combine` knows `mode` by.
fn mode_number(mode: MaskMode) -> u32 {
    match mode {
        MaskMode::Add => 0,
        MaskMode::Subtract => 1,
        MaskMode::Intersect => 2,
        MaskMode::Lighten => 3,
        MaskMode::Darken => 4,
        MaskMode::Difference => 5,
        MaskMode::None => 6,
    }
}

impl Gpu {
    /// `kept`, or `start` everywhere when `None`, combined with `mask`.
    pub(crate) fn combine_mask(
        &self,
        kept: Option<Resident>,
        start: f32,
        mask: &MaskCoverage<'_>,
        width: u16,
        height: u16,
    ) -> Resident {
        let step = &mask.step;
        let mut batch = self.batch();
        let mut own = self.grid(width, height);
        let count = own.count();
        let (groups, row) = pixel_groups(count);
        let bytes = self.buffer(mask.drawn.len() as u64, BufferKind::Storage);
        batch.stage(bytes.buffer(), mask.drawn);
        self.kernels.unpack.dispatch(
            &self.device,
            batch.encoder(),
            "unpack",
            &Params::new().u32(count).u32(row),
            own.buffer(),
            &[bytes.buffer(), &self.table],
            groups,
        );
        self.counts.upload(mask.drawn.len() as u64);
        let params = |columns: bool| {
            Params::new()
                .u32(count)
                .u32(row)
                .u32(u32::from(width))
                .u32(u32::from(height))
                .u32(step.grow.abs() as u32)
                .u32(u32::from(step.grow > 0.0))
                .u32(u32::from(columns))
        };
        if step.grow != 0.0 {
            for columns in [false, true] {
                let out = self.grid(width, height);
                self.kernels.mask.dispatch(
                    &self.device,
                    batch.encoder(),
                    "spread",
                    &params(columns),
                    out.buffer(),
                    &[own.buffer()],
                    groups,
                );
                own = out;
            }
        }
        if step.sigma.iter().any(|s| *s > 0.0) {
            self.blur(
                &mut batch,
                &mut own,
                &Blur::gaussian(step.sigma, Edges::Transparent),
            );
        }
        let first = kept.is_none();
        let kept = kept.unwrap_or_else(|| self.grid(width, height).into_resident());
        self.kernels.mask.dispatch(
            &self.device,
            batch.encoder(),
            "combine",
            &params(false)
                .u32(u32::from(first))
                .f32(start)
                .u32(u32::from(step.inverted))
                .f32(step.opacity)
                .u32(mode_number(step.mode)),
            grid(&kept).buffer(),
            &[own.buffer()],
            groups,
        );
        drop(own);
        self.finish(batch);
        self.counts.operation();
        kept
    }
}
