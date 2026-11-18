//! An optional GPU backend for the float canvas, on `wgpu`: Metal on macOS,
//! Vulkan or DX12 elsewhere ([D74]).
//!
//! [`Gpu`] is an [`Accelerator`] the rasteriser hands work to when a caller
//! asks for it (`nakkash render --gpu`). It lays layers by every blend mode
//! with their opacity, clips, masks and mattes, mixes filter layers, moves
//! pixels, and runs the effects that have a plan ([`nakkash_raster::PLANS`]):
//! the box and Gaussian blurs, the glow and the drop shadow. Every other
//! effect runs on the CPU, with a copy each way. Paths, glyphs and pictures
//! are always drawn on the CPU.
//!
//! Work is recorded, not run, as it is handed over: every thread drawing
//! records into one command encoder, which is submitted when pixels must
//! come back to the CPU or when enough is recorded. A frame
//! whose pixels stay on the GPU is one submission, when it is read back at
//! the end as the 8-bit bytes it is written in.
//!
//! Frames it draws approximate the CPU's, which stays the reference: the
//! kernels port the CPU's own arithmetic in 32-bit floats, and the
//! tolerances they meet are measured and set in D74. It never opens a
//! software adapter: a machine without a GPU gets a [`Refusal`] and stays
//! on the CPU.
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

mod block;
mod device;
mod grid;
mod kernels;
mod pool;
mod record;
mod tally;

use std::sync::{Arc, Mutex, mpsc};

use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_raster::kurbo::Affine;
use nakkash_raster::{
    Accelerator, Amount, BlendMode, Blur, Edges, Effect, Glow, GlowComposite, MatteMode, Move,
    Operation, Pixel, Pixels, Plan, Resident, Shadow, Share, Space, Tally,
};

pub use device::{Adapter, Refusal};

use grid::{Grid, grid, grid_mut};
use kernels::{Kernels, LINE_GROUP, Params, mode_row, pixel_groups};
use pool::{Kind as BufferKind, Pool, Pooled};
use record::{Batch, Recorder};
use tally::Counts;

/// Bytes in one float pixel.
const PIXEL_BYTES: u64 = 16;

/// A GPU the float canvas's work is handed to.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: Adapter,
    max_binding: u64,
    pool: Arc<Pool>,
    kernels: Kernels,
    counts: Counts,
    /// The work recorded and not yet submitted.
    recorder: Mutex<Recorder>,
    /// `c / 255` for every byte `c`, as the CPU works it out.
    table: wgpu::Buffer,
    /// Bound where a kernel reads nothing.
    empty: wgpu::Buffer,
}

impl Gpu {
    /// Opens the best hardware GPU of Metal, Vulkan or DX12.
    ///
    /// # Errors
    /// A [`Refusal`] when there is none, when the only adapter draws in
    /// software, or when it will not open a device.
    pub fn open() -> std::result::Result<Gpu, Refusal> {
        let opened = device::open()?;
        let device = opened.device;
        let kernels = Kernels::new(&device);
        let table: Vec<f32> = (0..=255u8).map(|c| f32::from(c) / 255.0).collect();
        let table = wgpu::util::DeviceExt::create_buffer_init(
            &device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("bytes to floats"),
                contents: f32_bytes(&table),
                usage: wgpu::BufferUsages::STORAGE,
            },
        );
        let empty = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("nothing"),
            size: PIXEL_BYTES,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Ok(Gpu {
            recorder: Mutex::new(Recorder::new(&device)),
            device,
            queue: opened.queue,
            adapter: opened.adapter,
            max_binding: opened.max_binding,
            pool: Arc::new(Pool::default()),
            kernels,
            counts: Counts::default(),
            table,
            empty,
        })
    }

    /// The adapter it draws on.
    pub fn adapter(&self) -> &Adapter {
        &self.adapter
    }

    /// A grid of `width` by `height` pixels whose contents are not set.
    fn grid(&self, width: u16, height: u16) -> Grid {
        Grid {
            pixels: self.buffer(grid_bytes(width, height), BufferKind::Storage),
            width,
            height,
        }
    }

    /// A pooled buffer of `bytes`, at least one pixel's.
    fn buffer(&self, bytes: u64, kind: BufferKind) -> Pooled {
        Pooled::new(&self.pool, &self.device, bytes.max(PIXEL_BYTES), kind)
    }

    /// A buffer holding `values`, copied into it in `batch`.
    fn values(&self, batch: &mut Batch<'_>, values: &[f32]) -> Pooled {
        let buffer = self.buffer(4 * values.len() as u64, BufferKind::Storage);
        batch.stage(buffer.buffer(), f32_bytes(values));
        buffer
    }

    /// The recorder, locked to record one operation.
    fn batch(&self) -> Batch<'_> {
        Batch::lock(&self.recorder)
    }

    /// Records a per-pixel kernel of the pixel family over `count` pixels.
    #[allow(clippy::too_many_arguments)]
    fn pixel_kernel(
        &self,
        batch: &mut Batch<'_>,
        entry: &str,
        count: u32,
        params: impl FnOnce(Params) -> Params,
        written: &wgpu::Buffer,
        read: Option<&wgpu::Buffer>,
        amount: Option<&wgpu::Buffer>,
    ) {
        let (groups, row) = pixel_groups(count);
        let params = params(Params::new().u32(count).u32(row));
        self.kernels.pixel.dispatch(
            &self.device,
            batch.encoder(),
            entry,
            &params,
            written,
            &[read.unwrap_or(&self.empty), amount.unwrap_or(&self.empty)],
            groups,
        );
    }

    /// Records the per-pixel amount `amount` gives over `count` pixels, if
    /// it changes from pixel to pixel.
    fn amount(&self, batch: &mut Batch<'_>, amount: &Amount<'_>, count: u32) -> Option<Pooled> {
        if amount.shares.is_empty() {
            return None;
        }
        let out = self.buffer(4 * u64::from(count), BufferKind::Storage);
        let (groups, row) = pixel_groups(count);
        for (n, share) in amount.shares.iter().enumerate() {
            let (entry, mode, values, pixels) = match share {
                Share::Values(values) => {
                    ("share_values", 0, Some(self.values(batch, values)), None)
                }
                Share::Alpha(held) => ("share_alpha", 0, None, Some(grid(held).buffer())),
                Share::Matte(held, mode) => (
                    "share_matte",
                    matte_row(*mode),
                    None,
                    Some(grid(held).buffer()),
                ),
            };
            let params = Params::new()
                .u32(count)
                .u32(row)
                .u32(u32::from(n == 0))
                .u32(mode)
                .f32(amount.opacity);
            self.kernels.amount.dispatch(
                &self.device,
                batch.encoder(),
                entry,
                &params,
                out.buffer(),
                &[
                    values.as_ref().map_or(&self.empty, Pooled::buffer),
                    pixels.unwrap_or(&self.empty),
                ],
                groups,
            );
        }
        Some(out)
    }

    /// Records `grid` moved by `transform`, into a new grid.
    fn transform_into(&self, batch: &mut Batch<'_>, grid: &Grid, transform: Affine) -> Grid {
        let out = self.grid(grid.width, grid.height);
        let [a, b, c, d, e, f] = transform.inverse().as_coeffs().map(|v| v as f32);
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        let params = Params::new()
            .u32(u32::from(grid.width))
            .u32(u32::from(grid.height))
            .u32(count)
            .u32(row)
            .f32(a)
            .f32(b)
            .f32(c)
            .f32(d)
            .f32(e)
            .f32(f);
        self.kernels.transform.dispatch(
            &self.device,
            batch.encoder(),
            "transformed",
            &params,
            out.buffer(),
            &[grid.buffer()],
            groups,
        );
        out
    }

    /// Records `blur`'s passes on `grid`, each into a new buffer.
    fn blur(&self, batch: &mut Batch<'_>, grid: &mut Grid, blur: &Blur) {
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
            // least 32 pixels and, once the window is long, about as many
            // as the window holds, so summing its first window costs no
            // more than sliding it.
            let target = u32::try_from((u64::from(n) * u64::from(lines)).div_ceil(65536))
                .unwrap_or(u32::MAX);
            let chunk = target.clamp(32, window.max(32));
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

    /// Records a copy of `grid`.
    fn duplicate_grid(&self, batch: &mut Batch<'_>, grid: &Grid) -> Grid {
        let out = self.grid(grid.width, grid.height);
        batch
            .encoder()
            .copy_buffer_to_buffer(grid.buffer(), 0, out.buffer(), 0, None);
        out
    }

    /// Records `glow` on `grid`.
    fn glow(&self, batch: &mut Batch<'_>, grid: &mut Grid, glow: &Glow) {
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
        // Added light is the same either way round, so behind the layer
        // too ([`glow_on_kernels`]).
        let entry = match glow.composite {
            GlowComposite::GlowOnly => "glow_alone",
            GlowComposite::WithOriginal | GlowComposite::BehindOriginal => "glow_add",
        };
        self.pixel_kernel(
            batch,
            entry,
            count,
            |p| p.u32(0).u32(0).f32(0.0).f32(0.0).f32(glow.intensity),
            grid.buffer(),
            Some(light.buffer()),
            None,
        );
    }

    /// Records `shadow` on `grid`.
    fn shadow(&self, batch: &mut Batch<'_>, grid: &mut Grid, shadow: &Shadow) {
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

    /// Ends one operation's recording, submitting what is recorded when
    /// it is enough.
    fn finish(&self, mut batch: Batch<'_>) {
        if batch.done() {
            self.flush(batch);
        }
    }

    /// Records what `record` copies into a readback buffer of `bytes`,
    /// submits everything recorded, waits for it, and reads the copy with
    /// `read`.
    fn read_back<T>(
        &self,
        bytes: u64,
        record: impl FnOnce(&mut Batch<'_>, &wgpu::Buffer),
        read: impl FnOnce(&[u8]) -> T,
    ) -> Result<T> {
        let readback = self.buffer(bytes, BufferKind::Readback);
        let mut batch = self.batch();
        record(&mut batch, readback.buffer());
        let index = self.flush(batch).expect("the copy back was recorded");
        let (sender, receiver) = mpsc::channel();
        readback
            .buffer()
            .map_async(wgpu::MapMode::Read, ..bytes, move |mapped| {
                // The receiver waits below, so the send is always heard.
                let _ = sender.send(mapped);
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: None,
            })
            .map_err(|e| failed("finish its work", e))?;
        receiver
            .recv()
            .map_err(|e| failed("map its pixels", e))?
            .map_err(|e| failed("map its pixels", e))?;
        let out = {
            let mapped = readback
                .buffer()
                .get_mapped_range(..bytes)
                .map_err(|e| failed("read its pixels", e))?;
            read(&mapped)
        };
        readback.buffer().unmap();
        self.counts.download(bytes);
        Ok(out)
    }

    /// Submits everything recorded, `None` when nothing is.
    fn flush(&self, batch: Batch<'_>) -> Option<wgpu::SubmissionIndex> {
        let index = batch.submit(&self.queue)?;
        self.counts.submission();
        // Lets the staging belt take back what finished submissions read.
        let _ = self.device.poll(wgpu::PollType::Poll);
        Some(index)
    }

    /// Records and submits `lay` or `mix` of `source` on `target`.
    fn blend(
        &self,
        entry: &str,
        target: &mut Resident,
        source: &Resident,
        mode: u32,
        amount: &Amount<'_>,
    ) -> Result<()> {
        let mut batch = self.batch();
        let count = grid(target).count();
        let shared = self.amount(&mut batch, amount, count);
        self.pixel_kernel(
            &mut batch,
            entry,
            count,
            |p| {
                p.u32(mode)
                    .u32(u32::from(shared.is_some()))
                    .f32(amount.opacity)
            },
            grid(target).buffer(),
            Some(grid(source).buffer()),
            shared.as_ref().map(Pooled::buffer),
        );
        drop(shared);
        self.finish(batch);
        self.counts.operation();
        Ok(())
    }
}

/// The bytes of a `width` by `height` grid of float pixels.
fn grid_bytes(width: u16, height: u16) -> u64 {
    u64::from(width) * u64::from(height) * PIXEL_BYTES
}

/// Floats as the bytes that hold them, as the GPU reads them.
fn f32_bytes(values: &[f32]) -> &[u8] {
    bytemuck::cast_slice(values)
}

/// The number the amount kernels read a matte's mode by.
fn matte_row(mode: MatteMode) -> u32 {
    match mode {
        MatteMode::Alpha => 0,
        MatteMode::AlphaInverted => 1,
        MatteMode::Luma => 2,
        MatteMode::LumaInverted => 3,
    }
}

/// A failure of the GPU, as a diagnostic.
fn failed(what: &str, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("the GPU failed to {what}: {error}"))
        .expected("a render on the CPU, without `--gpu`")
}

impl Accelerator for Gpu {
    fn holds(&self, width: u16, height: u16) -> bool {
        grid_bytes(width, height) <= self.max_binding
    }

    fn upload(&self, pixels: &Pixels) -> Result<Resident> {
        let grid = self.grid(pixels.width(), pixels.height());
        let bytes: &[u8] = bytemuck::cast_slice(pixels.data());
        let mut batch = self.batch();
        batch.stage(grid.buffer(), bytes);
        self.finish(batch);
        self.counts.upload(bytes.len() as u64);
        Ok(grid.into_resident())
    }

    fn upload_bytes(&self, rgba: &[u8], width: u16, height: u16) -> Result<Resident> {
        let grid = self.grid(width, height);
        let bytes = self.buffer(rgba.len() as u64, BufferKind::Storage);
        let mut batch = self.batch();
        batch.stage(bytes.buffer(), rgba);
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        self.kernels.unpack.dispatch(
            &self.device,
            batch.encoder(),
            "unpack",
            &Params::new().u32(count).u32(row),
            grid.buffer(),
            &[bytes.buffer(), &self.table],
            groups,
        );
        self.finish(batch);
        self.counts.upload(rgba.len() as u64);
        Ok(grid.into_resident())
    }

    fn transparent(&self, width: u16, height: u16) -> Result<Resident> {
        let grid = self.grid(width, height);
        let mut batch = self.batch();
        batch.encoder().clear_buffer(grid.buffer(), 0, None);
        self.finish(batch);
        Ok(grid.into_resident())
    }

    fn duplicate(&self, pixels: &Resident) -> Result<Resident> {
        let mut batch = self.batch();
        let copy = self.duplicate_grid(&mut batch, grid(pixels));
        self.finish(batch);
        Ok(copy.into_resident())
    }

    fn download(&self, pixels: &Resident) -> Result<Pixels> {
        let grid = grid(pixels);
        let bytes = grid_bytes(grid.width, grid.height);
        let data = self.read_back(
            bytes,
            |batch, readback| {
                batch
                    .encoder()
                    .copy_buffer_to_buffer(grid.buffer(), 0, readback, 0, Some(bytes));
            },
            |mapped| {
                // A mapping is aligned for floats in practice; if not, the
                // bytes are copied into place instead of read in place.
                match bytemuck::try_cast_slice::<u8, Pixel>(mapped) {
                    Ok(pixels) => pixels.to_vec(),
                    Err(_) => {
                        let mut data = vec![[0.0; 4]; grid.count() as usize];
                        bytemuck::cast_slice_mut::<Pixel, u8>(&mut data).copy_from_slice(mapped);
                        data
                    }
                }
            },
        )?;
        Ok(Pixels::from_data(grid.width, grid.height, data))
    }

    fn download_rgba8(&self, pixels: &Resident) -> Result<Vec<u8>> {
        let grid = grid(pixels);
        let count = grid.count();
        let bytes = 4 * u64::from(count);
        let packed = self.buffer(bytes, BufferKind::Storage);
        self.read_back(
            bytes,
            |batch, readback| {
                let (groups, row) = pixel_groups(count);
                self.kernels.pack.dispatch(
                    &self.device,
                    batch.encoder(),
                    "pack",
                    &Params::new().u32(count).u32(row),
                    packed.buffer(),
                    &[grid.buffer()],
                    groups,
                );
                batch
                    .encoder()
                    .copy_buffer_to_buffer(packed.buffer(), 0, readback, 0, Some(bytes));
            },
            <[u8]>::to_vec,
        )
    }

    fn lay(
        &self,
        canvas: &mut Resident,
        source: &Resident,
        mode: BlendMode,
        amount: &Amount<'_>,
    ) -> Result<()> {
        self.blend("lay", canvas, source, mode_row(mode), amount)
    }

    fn mix(&self, pixels: &mut Resident, other: &Resident, amount: &Amount<'_>) -> Result<()> {
        self.blend("mix_towards", pixels, other, 0, amount)
    }

    fn cut(&self, pixels: &mut Resident, kept: &[f32]) -> Result<()> {
        let mut batch = self.batch();
        let kept = self.values(&mut batch, kept);
        let grid = grid(pixels);
        self.pixel_kernel(
            &mut batch,
            "cut",
            grid.count(),
            |p| p,
            grid.buffer(),
            None,
            Some(kept.buffer()),
        );
        self.finish(batch);
        self.counts.operation();
        Ok(())
    }

    fn transformed(&self, pixels: &Resident, transform: Affine) -> Result<Resident> {
        let mut batch = self.batch();
        let out = self.transform_into(&mut batch, grid(pixels), transform);
        self.finish(batch);
        self.counts.operation();
        Ok(out.into_resident())
    }

    fn runs(&self, effect: &Effect) -> bool {
        effect.input().is_none()
            && (effect.movement().is_some()
                // A glow's composite and operation come from its values
                // alone, so any space shows whether its kernels draw it.
                || match effect.plan(&Space::default()) {
                    Some(Plan::Glow(glow)) => glow_on_kernels(&glow),
                    Some(Plan::Blur(_) | Plan::Shadow(_)) => true,
                    None => false,
                })
    }

    fn effect(&self, pixels: &mut Resident, effect: &Effect, space: &Space) -> Result<()> {
        let mut batch = self.batch();
        let grid = grid_mut(pixels);
        match (effect.movement(), effect.plan(space)) {
            (Some(Move { transform, opacity }), _) => {
                // As `Effect::apply` moves pixels.
                let inside = space.transform * transform * space.transform.inverse();
                if inside != Affine::IDENTITY {
                    let moved = self.transform_into(&mut batch, grid, inside);
                    grid.pixels = moved.pixels;
                }
                if opacity < 1.0 {
                    self.pixel_kernel(
                        &mut batch,
                        "scale",
                        grid.count(),
                        |p| p.u32(0).u32(0).f32(opacity),
                        grid.buffer(),
                        None,
                        None,
                    );
                }
            }
            (None, Some(Plan::Blur(blur))) => self.blur(&mut batch, grid, &blur),
            (None, Some(Plan::Glow(glow))) => self.glow(&mut batch, grid, &glow),
            (None, Some(Plan::Shadow(shadow))) => self.shadow(&mut batch, grid, &shadow),
            (None, None) => {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("the GPU has no kernel for `{}`", effect.spec().name),
                )
                .expected("an effect the GPU runs"));
            }
        }
        self.finish(batch);
        self.counts.accelerated(effect.spec().name);
        Ok(())
    }

    fn fell_back(&self, effect: &Effect) {
        self.counts.fell_back(effect.spec().name);
    }

    fn tally(&self) -> Tally {
        self.counts.tally()
    }
}

/// Whether the glow kernels draw `glow`: its light alone, or added to the
/// layer, which is the same with the layer behind it. Its other operations
/// combine the light with the layer on the CPU.
fn glow_on_kernels(glow: &Glow) -> bool {
    glow.composite == GlowComposite::GlowOnly || glow.operation == Operation::Blend(BlendMode::Add)
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
