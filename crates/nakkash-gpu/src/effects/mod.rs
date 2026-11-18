//! The effects with kernels, by family. Each runs the [`Plan`] the
//! rasteriser gives for its values on a layer's pixels, so it never reads
//! the parameters itself ([D74]); a family adds its variants to [`runs`]
//! and [`run`].
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

mod bevel;
mod blur;
mod colour;
mod composite;
mod distort;
mod generate;
mod lens;
mod radial;
mod vector;

pub(crate) use composite::operation_row;

use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_raster::{Plan, Space};

use crate::Gpu;
use crate::grid::Grid;
use crate::record::Batch;

/// Whether the kernels run `plan` as the CPU does, for every value it
/// may hold.
pub(crate) fn runs(plan: &Plan) -> bool {
    if let Some(runs) = generate::runs(plan) {
        return runs;
    }
    if let Plan::Distort(distortion) = plan {
        return distort::runs(distortion);
    }
    matches!(
        plan,
        Plan::Blur(_) | Plan::Shadow(_) | Plan::Glow(_) | Plan::SolidComposite(_)
    ) || colour::runs(plan)
        || matches!(plan, Plan::Radial(r) if radial::runs(r))
        || matches!(plan, Plan::Vector(_) | Plan::Lens(_) | Plan::Bevel(_))
}

/// Whether `plan` reads the drawing of another layer, which the kernels
/// take from [`Space::input`]. An effect that reads one runs on the GPU
/// only when its plan says so.
pub(crate) fn reads_input(plan: &Plan) -> bool {
    // A displacement map reads its map layer, and so do the vector blur
    // and the lens blur.
    colour::reads_input(plan)
        || matches!(
            plan,
            Plan::Distort(nakkash_raster::Distortion::DisplacementMap(_))
                | Plan::Vector(_)
                | Plan::Lens(_)
        )
}

impl Gpu {
    /// The drawing of the layer an effect reads ([`Space::input`]), copied
    /// to the GPU in `batch`, if it reads one.
    #[allow(dead_code, reason = "for the families that read another layer")]
    pub(crate) fn input(&self, batch: &mut Batch<'_>, space: &Space) -> Option<Grid> {
        let pixels = &space.input.as_ref()?.pixels;
        let grid = self.grid(pixels.width(), pixels.height());
        let bytes: &[u8] = bytemuck::cast_slice(pixels.data());
        batch.stage(grid.buffer(), bytes);
        self.counts.upload(bytes.len() as u64);
        Some(grid)
    }
}

/// Records `plan` on `grid` in `space`; only called when [`runs`] says
/// the kernels can.
pub(crate) fn run(
    gpu: &Gpu,
    batch: &mut Batch<'_>,
    grid: &mut Grid,
    plan: &Plan,
    space: &Space,
) -> Result<()> {
    let _ = space;
    if generate::run(gpu, batch, grid, plan) {
        return Ok(());
    }
    match plan {
        Plan::Blur(blur) => gpu.blur(batch, grid, blur),
        Plan::Glow(glow) => gpu.glow(batch, grid, glow),
        Plan::Shadow(shadow) => gpu.shadow(batch, grid, shadow),
        Plan::SolidComposite(solid) => gpu.solid_composite(batch, grid, solid),
        plan if colour::runs(plan) => colour::run(gpu, batch, grid, plan, space),
        Plan::Distort(distortion) => gpu.distort(batch, grid, distortion, space),
        Plan::Radial(radial) => gpu.radial(batch, grid, radial),
        Plan::Vector(vector) => gpu.vector(batch, grid, vector, space),
        Plan::Lens(lens) => gpu.lens(batch, grid, lens, space),
        Plan::Bevel(bevel) => gpu.bevel(batch, grid, bevel),
        _ => {
            return Err(
                Diagnostic::new(Kind::Invalid, "the GPU has no kernel for this plan")
                    .expected("a plan the GPU runs"),
            );
        }
    }
    Ok(())
}
