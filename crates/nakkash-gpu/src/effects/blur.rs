//! The blurs, the glow and the drop shadow: box passes, and the per-pixel
//! steps around them.

use nakkash_raster::kurbo::Affine;
use nakkash_raster::{Blur, Edges, Glow, GlowComposite, Shadow};

use crate::grid::Grid;
use crate::kernels::{LINE_GROUP, Params};
use crate::pool::Kind as BufferKind;
use crate::record::Batch;
use crate::{Gpu, grid_bytes};

impl Gpu {
    /// Records `blur`'s passes on `grid`, each into a new buffer.
    pub(crate) fn blur(&self, batch: &mut Batch<'_>, grid: &mut Grid, blur: &Blur) {
        let (width, height) = (u32::from(grid.width), u32::from(grid.height));
        // Rows: lines of `width` pixels, one after another; columns: lines
        // of `height` pixels a row apart.
        let ways = [
            (blur.rows, width, height, 1, width),
            (blur.columns, height, width, width, 1),
        ];
        for (passes, n, lines, along, across) in ways {
            if !passes.any() {
                continue;
            }
            let radius = passes.radius.max(0.0);
            let (whole, part) = (radius.floor() as u32, radius.fract());
            let window = 2 * whole + 1;
            // Enough invocations to fill the GPU, each sliding over at
            // least 32 pixels and at least as many as the window holds, so
            // summing its first window costs no more than sliding it: a
            // pass costs about the same at any radius.
            let target = u32::try_from((u64::from(n) * u64::from(lines)).div_ceil(65536))
                .unwrap_or(u32::MAX);
            let chunk = target.max(window).clamp(32, n.max(32));
            let params = Params::new()
                .u32(n)
                .u32(lines)
                .u32(along)
                .u32(across)
                .u32(chunk)
                .u32(whole)
                .f32(part)
                .f32(window as f32 + 2.0 * part)
                .u32(u32::from(blur.edges == Edges::Repeat));
            for _ in 0..passes.count {
                let out = self.buffer(grid_bytes(grid.width, grid.height), BufferKind::Storage);
                self.kernels.box_pass.dispatch(
                    &self.device,
                    batch.encoder(),
                    "box_pass",
                    &params,
                    out.buffer(),
                    &[grid.buffer()],
                    (lines.div_ceil(LINE_GROUP), n.div_ceil(chunk)),
                );
                grid.pixels = out;
            }
        }
    }

    /// Records `glow` on `grid`.
    pub(crate) fn glow(&self, batch: &mut Batch<'_>, grid: &mut Grid, glow: &Glow) {
        let count = grid.count();
        let mut light = self.duplicate_grid(batch, grid);
        let [r, g, b] = glow.luma;
        self.pixel_kernel(
            batch,
            "glow_threshold",
            count,
            |p| {
                p.u32(0)
                    .u32(0)
                    .f32(0.0)
                    .f32(glow.threshold)
                    .f32(0.0)
                    .f32(0.0)
                    .vec4([r, g, b, 0.0])
            },
            light.buffer(),
            None,
            None,
        );
        self.blur(batch, &mut light, &glow.blur);
        self.operate_kernel(batch, "light", &light, None, |p| p.k(glow.intensity));
        match glow.composite {
            GlowComposite::GlowOnly => {}
            // The light laid over the layer.
            GlowComposite::WithOriginal => {
                self.operate(batch, grid, &light, glow.operation);
                return;
            }
            // The layer laid over the light.
            GlowComposite::BehindOriginal => self.operate(batch, &light, grid, glow.operation),
        }
        grid.pixels = light.pixels;
    }

    /// Records `shadow` on `grid`.
    pub(crate) fn shadow(&self, batch: &mut Batch<'_>, grid: &mut Grid, shadow: &Shadow) {
        let count = grid.count();
        let mut cast = self.transform_into(batch, grid, Affine::translate(shadow.offset));
        let [r, g, b] = shadow.color;
        self.pixel_kernel(
            batch,
            "shadow_colour",
            count,
            |p| {
                p.u32(0)
                    .u32(0)
                    .f32(0.0)
                    .f32(0.0)
                    .f32(0.0)
                    .f32(shadow.opacity)
                    .vec4([r, g, b, 0.0])
            },
            cast.buffer(),
            None,
            None,
        );
        self.blur(batch, &mut cast, &shadow.blur);
        if shadow.alone {
            grid.pixels = cast.pixels;
            return;
        }
        self.pixel_kernel(
            batch,
            "shadow_under",
            count,
            |p| p,
            grid.buffer(),
            Some(cast.buffer()),
            None,
        );
    }
}
