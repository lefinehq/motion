//! The radial blurs: a zoom along rays out from a point and a spin round
//! rings about it, each pixel the mean of its stretch of the two lines
//! either side of it, from the exact running sums of every line
//! (`shaders/radial.wgsl`).
//!
//! The lines are read in batches that fit one buffer: a batch reads its
//! lines, sums them, and gives every pixel between two of its lines its
//! mean, so a 4K frame's rays, which hold several gigabytes of sums, are
//! never all held at once.

use std::f64::consts::TAU;
use std::sync::OnceLock;

use nakkash_raster::kurbo::Point;
use nakkash_raster::{RadialBlur, RadialFade, RadialKind, RadialStretch, Rays, Rings};

use crate::Gpu;
use crate::grid::Grid;
use crate::kernels::{LINE_GROUP, Params, pixel_groups};
use crate::pool::{Kind as BufferKind, Pooled};
use crate::record::Batch;

/// Samples a chunk of a line, read and summed by one invocation.
const CHUNK: u32 = 64;

/// The most bytes one batch of lines takes.
const BATCH_BYTES: u64 = 192 << 20;

/// The angles round a turn the table of cosines and sines holds.
const TRIG: usize = 16384;

/// The farthest a centre may lie from the origin of the layer's
/// coordinates, in px, for the kernels to read its lines as finely as the
/// CPU: a zoom's rays split each step into exact parts up to 2^24 px out,
/// a spin's rings up to 2^16. Past it the effect stays on the CPU.
const ZOOM_REACH: f64 = 16_777_216.0;
const SPIN_REACH: f64 = 32_768.0;

/// Whether the kernels run `plan` as the CPU does.
pub(crate) fn runs(plan: &RadialBlur) -> bool {
    let reach = match plan.kind {
        RadialKind::Zoom => ZOOM_REACH,
        RadialKind::Spin => SPIN_REACH,
    };
    plan.centre.x.abs() <= reach && plan.centre.y.abs() <= reach
}

/// The cosine and sine of `TRIG` angles evenly round a turn, each as a
/// whole number over 2^15, which the kernels multiply by a radius in
/// integers, and the rest as a float.
fn trig() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..TRIG)
            .flat_map(|m| {
                let (sin, cos) = (TAU * m as f64 / TRIG as f64).sin_cos();
                let (c, s) = ((cos * 32_768.0).round(), (sin * 32_768.0).round());
                [
                    f32::from_bits((c as i32) as u32),
                    f32::from_bits((s as i32) as u32),
                    (cos - c / 32_768.0) as f32,
                    (sin - s / 32_768.0) as f32,
                ]
            })
            .collect()
    })
}

/// A whole number held to what the kernels add to it, as a float's bits.
fn whole(v: f64) -> f32 {
    f32::from_bits((v.clamp(-1e9, 1e9) as i32) as u32)
}

/// The radial family's uniform block (`radial.wgsl`'s `Params`).
#[derive(Debug, Clone, Default)]
struct RadialParams {
    width: u32,
    height: u32,
    count: u32,
    row: u32,
    lines: u32,
    samples: u32,
    batch_first: u32,
    batch_lines: u32,
    chunks: u32,
    linear: bool,
    across: bool,
    whole: bool,
    chunk_base: u32,
    inner: u32,
    point_base: u32,
    chunk_first: u32,
    spin: bool,
    shift: u32,
    centre: Point,
    view: [f32; 4],
}

impl RadialParams {
    /// How many vec4s a point of a line takes.
    fn stride(&self) -> u32 {
        if self.linear { 5 } else { 3 }
    }

    fn params(&self) -> Params {
        let cw = (self.centre.x.floor(), self.centre.y.floor());
        Params::new()
            .u32(self.width)
            .u32(self.height)
            .u32(self.count)
            .u32(self.row)
            .u32(self.lines)
            .u32(self.samples)
            .u32(self.batch_first)
            .u32(self.batch_lines)
            .u32(CHUNK)
            .u32(self.chunks)
            .u32(u32::from(self.whole))
            .u32(u32::from(self.linear))
            .u32(u32::from(self.across))
            .u32(self.stride())
            .u32(self.chunk_base)
            .u32(self.inner)
            .u32(self.point_base)
            .u32(self.chunk_first)
            .u32(u32::from(self.spin))
            .u32(self.shift)
            .vec4([whole(cw.0), whole(cw.1), 0.0, 0.0])
            .vec4([
                (self.centre.x - cw.0) as f32,
                (self.centre.y - cw.1) as f32,
                0.0,
                0.0,
            ])
            .vec4(self.view)
    }
}

/// One batch of lines: the first, how many it reads, and for rings where
/// their points and chunks start among every ring's.
struct Span {
    first: u32,
    lines: u32,
    chunks: u32,
    point_base: u32,
    chunk_first: u32,
    /// Where the chunks' sums start, in vec4s, and the bytes it takes.
    chunk_base: u32,
    bytes: u64,
}

/// How many bits after the point a zoom's ray steps keep as a whole
/// number: so many that the step times any sample's number stays within
/// 2^23, which a float and an integer both hold exactly.
fn ray_shift(rays: &Rays) -> u32 {
    let mut shift = 0;
    while shift < 22 && (1u64 << (shift + 1)) * rays.samples as u64 <= 1 << 23 {
        shift += 1;
    }
    shift
}

/// The rays of `rays` from `centre`, three vec4s each: the first
/// sample's whole px and part less half a px; the step along the ray as
/// whole numbers over 2^`shift` and the rest; and the step as floats.
fn ray_table(rays: &Rays, centre: Point, shift: u32) -> Vec<f32> {
    let scale = f64::from(1u32 << shift);
    let mut table = Vec::with_capacity(12 * (rays.count + 1));
    for j in 0..=rays.count {
        // As `zoom` reads ray j.
        let a = rays.from + j as f64 * rays.step;
        let (dx, dy) = (a.cos(), -a.sin());
        let (x, y) = (
            centre.x + dx * rays.first - 0.5,
            centre.y + dy * rays.first - 0.5,
        );
        let (ix, iy) = (x.floor(), y.floor());
        let (mx, my) = ((dx * scale).round(), (dy * scale).round());
        table.extend_from_slice(&[
            whole(ix),
            whole(iy),
            (x - ix) as f32,
            (y - iy) as f32,
            whole(mx),
            whole(my),
            (dx - mx / scale) as f32,
            (dy - my / scale) as f32,
            dx as f32,
            dy as f32,
            0.0,
            0.0,
        ]);
    }
    table
}

impl Gpu {
    /// Records the radial family's `entry` over `groups`, writing
    /// `written` and reading `reads`.
    fn radial_kernel(
        &self,
        batch: &mut Batch<'_>,
        entry: &str,
        params: &RadialParams,
        written: &wgpu::Buffer,
        reads: [Option<&wgpu::Buffer>; 5],
        groups: (u32, u32),
    ) {
        self.kernels.radial.dispatch(
            &self.device,
            batch.encoder(),
            entry,
            &params.params(),
            written,
            &reads.map(|read| read.unwrap_or(&self.empty)),
            groups,
        );
    }

    /// Records `plan` on `grid`.
    pub(crate) fn radial(&self, batch: &mut Batch<'_>, grid: &mut Grid, plan: &RadialBlur) {
        let (width, height) = (usize::from(grid.width), usize::from(grid.height));
        if width == 0 || height == 0 || plan.none() {
            return;
        }
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        let (reach, across) = match plan.stretch {
            RadialStretch::Behind(s) => (s, false),
            RadialStretch::Across(s) => (s, true),
        };
        let mut params = RadialParams {
            width: u32::from(grid.width),
            height: u32::from(grid.height),
            count,
            row,
            linear: plan.fade == RadialFade::Linear,
            across,
            centre: plan.centre,
            ..RadialParams::default()
        };
        let stride = u64::from(params.stride());
        let budget = BATCH_BYTES.min(self.max_binding);
        // The lines, and the batches they are read in.
        let (table, trig, spans) = match plan.kind {
            RadialKind::Zoom => {
                let rays = plan.rays(width, height);
                params.lines = rays.count as u32;
                params.samples = rays.samples as u32;
                params.whole = rays.whole;
                params.view = [
                    rays.from as f32,
                    rays.step as f32,
                    rays.first as f32,
                    reach as f32,
                ];
                params.shift = ray_shift(&rays);
                let table = self.values(batch, &ray_table(&rays, plan.centre, params.shift));
                (table, None, zoom_spans(&rays, stride, budget))
            }
            RadialKind::Spin => {
                let rings = plan.rings(width, height);
                params.lines = rings.count as u32;
                params.inner = rings.inner as u32;
                params.spin = true;
                params.view = [0.0, 0.0, 0.0, reach as f32];
                let (table, spans) = ring_table(&rings, stride, budget);
                let table = self.values(batch, &table);
                (table, Some(self.values(batch, trig())), spans)
            }
        };
        let trig = trig.as_ref().map(Pooled::buffer);
        let keys = self.buffer(4 * u64::from(count.div_ceil(4) * 4), BufferKind::Storage);
        let (keys_entry, pixels_entry) = match plan.kind {
            RadialKind::Zoom => ("zoom_keys", "zoom_pixels"),
            RadialKind::Spin => ("spin_keys", "spin_pixels"),
        };
        // Four pixels an invocation.
        let (key_groups, key_row) = pixel_groups(count.div_ceil(4));
        self.radial_kernel(
            batch,
            keys_entry,
            &RadialParams {
                row: key_row,
                ..params.clone()
            },
            keys.buffer(),
            [None, Some(table.buffer()), trig, None, None],
            key_groups,
        );
        let out = self.grid(grid.width, grid.height);
        let most = spans.iter().map(|s| s.bytes).max().unwrap_or(0);
        let lines = self.buffer(most, BufferKind::Storage);
        for span in &spans {
            let params = RadialParams {
                batch_first: span.first,
                batch_lines: span.lines,
                chunks: span.chunks,
                chunk_base: span.chunk_base,
                point_base: span.point_base,
                chunk_first: span.chunk_first,
                ..params.clone()
            };
            let line_groups = (span.lines.div_ceil(LINE_GROUP), span.chunks.max(1));
            let read = [Some(grid.buffer()), Some(table.buffer()), trig, None, None];
            self.radial_kernel(
                batch,
                "read_lines",
                &params,
                lines.buffer(),
                read,
                line_groups,
            );
            self.radial_kernel(
                batch,
                "sums",
                &params,
                lines.buffer(),
                read,
                (span.lines.div_ceil(LINE_GROUP), 1),
            );
            self.radial_kernel(batch, "prefix", &params, lines.buffer(), read, line_groups);
            self.radial_kernel(
                batch,
                pixels_entry,
                &params,
                out.buffer(),
                [
                    None,
                    Some(table.buffer()),
                    trig,
                    Some(lines.buffer()),
                    Some(keys.buffer()),
                ],
                groups,
            );
        }
        grid.pixels = out.pixels;
    }
}

/// The batches of a zoom's rays: as many rays each as fit `budget`, each
/// batch reading one ray more than the spaces it covers.
fn zoom_spans(rays: &Rays, stride: u64, budget: u64) -> Vec<Span> {
    let samples = rays.samples as u64;
    let chunks = samples.div_ceil(u64::from(CHUNK));
    let per_line = ((samples + 1) * stride + 4 * chunks) * 16;
    let most = (budget / per_line).clamp(2, rays.count as u64 + 1) as u32;
    let mut spans = Vec::new();
    let mut first = 0u32;
    while first < rays.count as u32 {
        let lines = most.min(rays.count as u32 + 1 - first);
        let chunk_base = (u64::from(lines) * (samples + 1) * stride) as u32;
        spans.push(Span {
            first,
            lines,
            chunks: chunks as u32,
            point_base: 0,
            chunk_first: 0,
            chunk_base,
            bytes: u64::from(lines) * per_line,
        });
        first += lines - 1;
    }
    spans
}

/// The rings of `rings`, two vec4s each (how many samples a turn holds,
/// its first sample, how many it takes, where its points start; where its
/// chunks start, its radius), and the batches they are read in: as many
/// rings each as fit `budget`, each reading one ring more than the spaces
/// it covers.
fn ring_table(rings: &Rings, stride: u64, budget: u64) -> (Vec<f32>, Vec<Span>) {
    let mut table = Vec::with_capacity(8 * (rings.count + 1));
    let mut sizes = Vec::with_capacity(rings.count + 1);
    let (mut points, mut chunks) = (0u64, 0u64);
    for k in 0..=rings.count {
        let ring = rings.ring(k);
        let samples = ring.samples as u64;
        let n = samples.div_ceil(u64::from(CHUNK));
        table.extend_from_slice(&[
            ring.round.min(u32::MAX as usize) as u32,
            (ring.first as i32) as u32,
            samples as u32,
            points as u32,
            chunks as u32,
            ring.radius as u32,
            0,
            0,
        ]);
        sizes.push((points, chunks, samples + 1, n));
        points += samples + 1;
        chunks += n;
    }
    let bytes = |from: usize, to: usize| {
        let (p0, c0, ..) = sizes[from];
        let (p1, c1, pn, cn) = sizes[to];
        ((p1 + pn - p0) * stride + 4 * (c1 + cn - c0)) * 16
    };
    let mut spans = Vec::new();
    let mut first = 0usize;
    while first < rings.count {
        let mut last = first + 1;
        while last < rings.count && bytes(first, last + 1) <= budget {
            last += 1;
        }
        let (p0, c0, ..) = sizes[first];
        let (p1, _, pn, _) = sizes[last];
        let most = (first..=last).map(|k| sizes[k].3).max().unwrap_or(1);
        spans.push(Span {
            first: first as u32,
            lines: (last - first + 1) as u32,
            chunks: most as u32,
            point_base: p0 as u32,
            chunk_first: c0 as u32,
            chunk_base: ((p1 + pn - p0) * stride) as u32,
            bytes: bytes(first, last),
        });
        first = last;
    }
    (table.into_iter().map(f32::from_bits).collect(), spans)
}
