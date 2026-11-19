//! Projected layers on the GPU ([D58]): a plane's buffer and its halved
//! copies made on the GPU, then laid onto a grid through each of its
//! landings, as `projected.rs` lays them on the CPU.
//!
//! [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera

use nakkash_format::Result;
use nakkash_raster::kurbo::Point;
use nakkash_raster::{Landing, Resident, Texels};

use crate::grid::{Grid, grid};
use crate::kernels::{Params, pixel_groups};
use crate::pool::Kind as BufferKind;
use crate::record::Batch;
use crate::{Gpu, grid_bytes};

/// The sizes of a buffer and of each of its halved copies, down to a px:
/// as many copies as halving the longest side takes to reach one px.
fn chain_sizes(width: u16, height: u16) -> Vec<(u32, u32)> {
    let longest = width.max(height);
    let halves = u16::BITS - longest.saturating_sub(1).leading_zeros();
    let mut sizes = vec![(u32::from(width), u32::from(height))];
    for _ in 0..halves {
        let (w, h) = *sizes.last().expect("the buffer itself");
        sizes.push((w.div_ceil(2), h.div_ceil(2)));
    }
    sizes
}

impl Gpu {
    /// A buffer holding `words`, copied into it in `batch`.
    pub(crate) fn words(&self, batch: &mut Batch<'_>, words: &[u32]) -> crate::pool::Pooled {
        let buffer = self.buffer(4 * words.len() as u64, BufferKind::Storage);
        batch.stage(buffer.buffer(), bytemuck::cast_slice(words));
        buffer
    }

    /// `buffer` laid onto a new `width` by `height` grid through each of
    /// `landings`, adding `share` of it; `None` when its chain of copies
    /// is larger than one binding may be.
    pub(crate) fn projected(
        &self,
        buffer: Texels<'_>,
        landings: &[Landing],
        share: f32,
        width: u16,
        height: u16,
    ) -> Option<Grid> {
        let (bytes, (bw, bh)) = match buffer {
            Texels::Bytes { width, height, .. } => (true, (width, height)),
            Texels::Floats(pixels) => (false, (pixels.width(), pixels.height())),
            Texels::Held(held) => (false, (held.width(), held.height())),
        };
        let sizes = chain_sizes(bw, bh);
        let mut offsets = Vec::with_capacity(sizes.len());
        let mut total = 0u64;
        for &(w, h) in &sizes {
            offsets.push(total);
            total += u64::from(w) * u64::from(h);
        }
        let words_per_px: u64 = if bytes { 1 } else { 4 };
        let chain_bytes = total * words_per_px * 4;
        if chain_bytes > self.max_binding || u32::try_from(total * words_per_px).is_err() {
            return None;
        }
        let chain = self.buffer(chain_bytes, BufferKind::Storage);
        let mut batch = self.batch();
        match buffer {
            Texels::Bytes { rgba, .. } => {
                batch.stage(chain.buffer(), rgba);
                self.counts.upload(rgba.len() as u64);
            }
            Texels::Floats(pixels) => {
                let data: &[u8] = bytemuck::cast_slice(pixels.data());
                batch.stage(chain.buffer(), data);
                self.counts.upload(data.len() as u64);
            }
            Texels::Held(held) => {
                batch.encoder().copy_buffer_to_buffer(
                    grid(held).buffer(),
                    0,
                    chain.buffer(),
                    0,
                    Some(grid_bytes(bw, bh)),
                );
            }
        }
        // Each copy from the one before it.
        for n in 1..sizes.len() {
            let ((fw, fh), (tw, th)) = (sizes[n - 1], sizes[n]);
            let count = tw * th;
            let (groups, row) = pixel_groups(count);
            let params = Params::new()
                .u32(count)
                .u32(row)
                .u32(u32::from(bytes))
                .u32(offsets[n - 1] as u32)
                .u32(fw)
                .u32(fh)
                .u32(offsets[n] as u32)
                .u32(tw);
            self.kernels.halve.dispatch(
                &self.device,
                batch.encoder(),
                "halve",
                &params,
                chain.buffer(),
                &[],
                groups,
            );
        }
        let out = self.grid(width, height);
        batch.encoder().clear_buffer(out.buffer(), 0, None);
        let laid: Vec<&Landing> = landings
            .iter()
            .filter(|l| l.landed[0] < l.landed[2] && l.landed[1] < l.landed[3])
            .collect();
        if let Some(area) = laid.iter().map(|l| l.landed).reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        }) {
            let levels: Vec<u32> = sizes
                .iter()
                .zip(&offsets)
                .flat_map(|(&(w, h), &at)| [at as u32, w, h, 0])
                .collect();
            let levels = self.words(&mut batch, &levels);
            let records: Vec<u32> = laid.iter().flat_map(|l| landing_words(l)).collect();
            let records = self.words(&mut batch, &records);
            let [x0, y0, x1, y1] = area.map(|v| v as u32);
            let count = (x1 - x0) * (y1 - y0);
            let (groups, row) = pixel_groups(count);
            let params = Params::new()
                .u32(count)
                .u32(row)
                .u32(u32::from(width))
                .u32(x0)
                .u32(y0)
                .u32(x1 - x0)
                .u32(laid.len() as u32)
                .u32(sizes.len() as u32 - 1)
                .u32(u32::from(bytes))
                .f32(share)
                .f32(f32::from(bw))
                .f32(f32::from(bh));
            self.kernels.project.dispatch(
                &self.device,
                batch.encoder(),
                "project",
                &params,
                out.buffer(),
                &[chain.buffer(), levels.buffer(), records.buffer()],
                groups,
            );
        }
        self.finish(batch);
        self.counts.operation();
        Some(out)
    }
}

/// A landing as the project kernel reads it: the map back at the centre of
/// its first px, worked out here in 64-bit floats, and its steps one px
/// right and one px down, each `(x, y, w)`, then the px it lands on.
fn landing_words(landing: &Landing) -> [u32; 16] {
    let [g0, g1, g2] = landing.to_buffer.rows();
    let [x0, y0, x1, y1] = landing.landed;
    let p = Point::new(x0 as f64 + 0.5, y0 as f64 + 0.5);
    let dot = |r: [f64; 3]| r[0] * p.x + r[1] * p.y + r[2];
    let float = |v: f64| (v as f32).to_bits();
    [
        float(dot(g0)),
        float(dot(g1)),
        float(dot(g2)),
        0,
        float(g0[0]),
        float(g1[0]),
        float(g2[0]),
        0,
        float(g0[1]),
        float(g1[1]),
        float(g2[1]),
        0,
        x0 as u32,
        y0 as u32,
        x1 as u32,
        y1 as u32,
    ]
}

/// Lays `buffer` on the CPU, as the rasteriser's own accelerator default
/// does, for a chain too large for the GPU.
pub(crate) fn on_cpu(
    gpu: &Gpu,
    buffer: Texels<'_>,
    landings: &[Landing],
    share: f32,
    width: u16,
    height: u16,
) -> Result<Resident> {
    nakkash_raster::project_on_cpu(gpu, buffer, landings, share, width, height)
}

#[cfg(test)]
#[path = "../tests/unit/project.rs"]
mod tests;
