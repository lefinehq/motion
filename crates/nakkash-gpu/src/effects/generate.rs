//! The generators: effects that draw something of their own in the
//! layer's coordinates, a gradient, a grid, noise, a beam, or cut the
//! layer into blinds or behind a wipe.
//!
//! Each kernel reads the layer's point at a pixel from lines the CPU works
//! out in 64 bits, as `each_in_layer` works out each point
//! (`layer.wgsl`), so a layer the transform keeps upright meets a
//! generator at the points the CPU does, to the bit. Where a result jumps
//! at an edge in a layer the transform turns, the CPU gives the point at
//! every pixel instead.

use nakkash_raster::kurbo::{Affine, Point};
use nakkash_raster::{
    Beam, FourColor, Fractal, FractalNoise, GridLines, IrisWipe, LinearWipe, NOISE_CELL, NoiseKind,
    Operation, Plan, Ramp, VenetianBlinds,
};

use crate::Gpu;
use crate::effects::operation_row;
use crate::grid::Grid;
use crate::kernels::{Params, pixel_groups};
use crate::record::Batch;

/// The lines `layer.wgsl` reads the layer's points from, on a grid
/// `width` by `height` pixels that `back` maps to the layer: the layer's x
/// and y at each column's centre on the grid's top edge, then what each
/// row's centre adds to them. Each is worked out in 64 bits in the order
/// the CPU's point is, so where the transform keeps the layer upright,
/// adding a row's to a column's gives the CPU's point exactly.
pub(crate) fn layer_lines(back: Affine, width: u16, height: u16) -> Vec<f32> {
    let [a, b, c, d, e, f] = back.as_coeffs();
    let centre = |i: u16| f64::from(i) + 0.5;
    let mut lines = Vec::with_capacity(2 * (usize::from(width) + usize::from(height)));
    lines.extend((0..width).map(|i| (a * centre(i) + e) as f32));
    lines.extend((0..width).map(|i| (b * centre(i)) as f32));
    lines.extend((0..height).map(|j| (c * centre(j)) as f32));
    lines.extend((0..height).map(|j| (d * centre(j) + f) as f32));
    lines
}

/// Whether the kernels run `plan` as the CPU does, for every value it may
/// hold, when it is a generator's; `None` for other plans.
pub(crate) fn runs(plan: &Plan) -> Option<bool> {
    match plan {
        // Block noise jumps at each cell's edge, so its octaves' points
        // must be the CPU's to the bit, and the kernel works them out so.
        // Swirly noise turns each octave by the sine and cosine of the
        // level so far, which the GPU approximates: swirly block noise
        // stays on the CPU.
        Plan::FractalNoise(noise) => {
            Some(noise.kind != NoiseKind::Block || noise.fractal != Fractal::Swirly)
        }
        Plan::Ramp(_)
        | Plan::FourColor(_)
        | Plan::Grid(_)
        | Plan::VenetianBlinds(_)
        | Plan::LinearWipe(_)
        | Plan::IrisWipe(_)
        | Plan::Beam(_) => Some(true),
        _ => None,
    }
}

/// Records `plan` on `grid`, when it is a generator's; `false` for other
/// plans.
pub(crate) fn run(gpu: &Gpu, batch: &mut Batch<'_>, grid: &Grid, plan: &Plan) -> bool {
    match plan {
        Plan::FractalNoise(noise) => gpu.fractal_noise(batch, grid, noise),
        Plan::Ramp(ramp) => gpu.ramp(batch, grid, ramp),
        Plan::FourColor(four) => gpu.four_color(batch, grid, four),
        Plan::Grid(lines) => gpu.grid_lines(batch, grid, lines),
        Plan::VenetianBlinds(blinds) => gpu.venetian_blinds(batch, grid, blinds),
        Plan::LinearWipe(wipe) => gpu.linear_wipe(batch, grid, wipe),
        Plan::IrisWipe(iris) => gpu.iris_wipe(batch, grid, iris),
        Plan::Beam(beam) => gpu.beam(batch, grid, beam),
        _ => return false,
    }
    true
}

/// Whether `back` turns or skews the layer, so a row moves its x or a
/// column its y.
fn turns(back: Affine) -> bool {
    let [_, b, c, ..] = back.as_coeffs();
    b != 0.0 || c != 0.0
}

/// What a generator's kernel reads the layer's points from, on a grid
/// `width` by `height` that `back` maps to the layer, and whether it is
/// the point at every pixel: the layer's lines, or, for a generator whose
/// result jumps or turns within a pixel somewhere (`exact`) in a layer
/// `back` turns, every point. Adding a row's line to a column's rounds
/// once more than the CPU does, which only a sharp edge would show.
fn layer(back: Affine, width: u16, height: u16, exact: bool) -> (Vec<f32>, bool) {
    match exact && turns(back) {
        true => (layer_points(back, width, height), true),
        false => (layer_lines(back, width, height), false),
    }
}

/// The parameters of a kernel of `generate.wgsl` beyond the grid's, field
/// by field.
#[derive(Debug, Clone, Default)]
struct Generator {
    operation: Option<Operation>,
    points: bool,
    extra: usize,
    n: u32,
    flag: bool,
    spots: [[f32; 4]; 2],
    colours: [[f32; 4]; 4],
    k: [f32; 4],
    k2: [f32; 4],
    range: [f32; 4],
}

/// A straight colour with its alpha.
fn rgba([r, g, b]: [f32; 3], a: f32) -> [f32; 4] {
    [r, g, b, a]
}

/// `f`'s values at every pixel of a grid `width` by `height`, `N` a
/// pixel, row by row, worked out on every core: the CPU's share of a
/// kernel that needs the CPU's own 64-bit steps at every pixel.
fn per_pixel<const N: usize>(
    width: u16,
    height: u16,
    f: impl Fn(u16, u16) -> [f32; N] + Sync,
) -> Vec<f32> {
    let (w, h) = (usize::from(width), usize::from(height));
    let mut values = vec![0.0f32; N * w * h];
    if values.is_empty() {
        return values;
    }
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let rows = h.div_ceil(threads);
    std::thread::scope(|scope| {
        for (chunk, part) in values.chunks_mut(N * w * rows).enumerate() {
            let f = &f;
            scope.spawn(move || {
                for (k, value) in part.chunks_exact_mut(N).enumerate() {
                    let i = chunk * rows * w + k;
                    let (x, y) = ((i % w) as u16, (i / w) as u16);
                    value.copy_from_slice(&f(x, y));
                }
            });
        }
    });
    values
}

/// The layer's point at every pixel of a grid `width` by `height` that
/// `back` maps to the layer, x then y, as `each_in_layer` works each out.
fn layer_points(back: Affine, width: u16, height: u16) -> Vec<f32> {
    per_pixel(width, height, |x, y| {
        let at = back * Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
        [at.x as f32, at.y as f32]
    })
}

impl Gpu {
    /// Records `noise` on `grid`.
    pub(crate) fn fractal_noise(&self, batch: &mut Batch<'_>, grid: &Grid, noise: &FractalNoise) {
        let (width, height) = (grid.width, grid.height);
        // The layer's lines give the CPU's points exactly where the
        // transform keeps the layer upright, and within a rounding where it
        // turns it, which only block noise would show: it takes the layer's
        // point at every pixel there.
        let [_, b, c, ..] = noise.back.as_coeffs();
        let (lines, points) = match noise.kind == NoiseKind::Block && (b != 0.0 || c != 0.0) {
            true => (layer_points(noise.back, width, height), true),
            false => (layer_lines(noise.back, width, height), false),
        };
        let lines = self.values(batch, &lines);
        let (op, mode) = operation_row(noise.operation);
        let (sin, cos) = noise.rotation.sin_cos();
        let (sub_sin, sub_cos) = noise.sub_rotation.sin_cos();
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        let params = Params::new()
            .u32(count)
            .u32(row)
            .u32(u32::from(width))
            .u32(u32::from(height))
            .u32(op)
            .u32(mode)
            .u32(noise.octaves())
            .u32(u32::from(points))
            .u32(noise.seed)
            .u32(noise.fractal as u32)
            .u32(noise.kind as u32)
            .u32(noise.overflow as u32)
            .u32(u32::from(noise.invert))
            .f32(noise.contrast)
            .f32(noise.brightness)
            .f32(noise.complexity)
            .f32(noise.influence)
            .f32(noise.scaling)
            .f32(noise.evolution)
            .f32(noise.opacity)
            .vec4([sin, cos, sub_sin, sub_cos])
            .vec4([
                noise.scale[0],
                noise.scale[1],
                noise.offset[0],
                noise.offset[1],
            ])
            .vec4([
                noise.sub_offset[0] / NOISE_CELL,
                noise.sub_offset[1] / NOISE_CELL,
                0.0,
                1.0,
            ]);
        self.kernels.noise.dispatch(
            &self.device,
            batch.encoder(),
            "fractal_noise",
            &params,
            grid.buffer(),
            &[lines.buffer()],
            groups,
        );
    }

    /// Records `entry` of the generate family on `grid`, reading `lines`.
    fn generator(
        &self,
        batch: &mut Batch<'_>,
        grid: &Grid,
        entry: &str,
        lines: &[f32],
        generator: Generator,
    ) {
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        let (op, mode) = generator.operation.map_or((0, 0), operation_row);
        let mut params = Params::new()
            .u32(count)
            .u32(row)
            .u32(u32::from(grid.width))
            .u32(u32::from(grid.height))
            .u32(op)
            .u32(mode)
            .u32(u32::from(generator.points))
            .u32(u32::try_from(generator.extra).expect("a grid's lines fit"))
            .u32(generator.n)
            .u32(u32::from(generator.flag))
            .vec4([0.0, 1.0, 0.0, 0.0]);
        for v in generator.spots.into_iter().chain(generator.colours) {
            params = params.vec4(v);
        }
        let params = params
            .vec4(generator.k)
            .vec4(generator.k2)
            .vec4(generator.range);
        let lines = self.values(batch, lines);
        self.kernels.generate.dispatch(
            &self.device,
            batch.encoder(),
            entry,
            &params,
            grid.buffer(),
            &[lines.buffer()],
            groups,
        );
    }

    /// Records `ramp` on `grid`.
    pub(crate) fn ramp(&self, batch: &mut Batch<'_>, grid: &Grid, ramp: &Ramp) {
        let (lines, points) = layer(ramp.back, grid.width, grid.height, false);
        let [sx, sy] = ramp.start;
        let [dx, dy] = ramp.direction;
        let mut colours = [[0.0; 4]; 4];
        colours[..2].copy_from_slice(&[ramp.from, ramp.to]);
        self.generator(
            batch,
            grid,
            "ramp",
            &lines,
            Generator {
                points,
                flag: ramp.radial,
                spots: [[sx, sy, dx, dy], [0.0; 4]],
                colours,
                k: [ramp.length2, ramp.blend, 0.0, 0.0],
                ..Generator::default()
            },
        );
    }

    /// Records `four` on `grid`.
    pub(crate) fn four_color(&self, batch: &mut Batch<'_>, grid: &Grid, four: &FourColor) {
        let (lines, points) = layer(four.back, grid.width, grid.height, false);
        let [a, b, c, d] = four.points;
        self.generator(
            batch,
            grid,
            "four_color",
            &lines,
            Generator {
                operation: Some(four.operation),
                points,
                spots: [[a[0], a[1], b[0], b[1]], [c[0], c[1], d[0], d[1]]],
                colours: four.colors.map(|c| rgba(c, 1.0)),
                k: [four.power, four.opacity, 0.0, 0.0],
                ..Generator::default()
            },
        );
    }

    /// Records `lines` on `grid`. A line's edges are a pixel soft, so a
    /// turned layer gives every point.
    pub(crate) fn grid_lines(&self, batch: &mut Batch<'_>, grid: &Grid, lines: &GridLines) {
        let (layer, points) = layer(lines.back, grid.width, grid.height, true);
        let [ax, ay] = lines.anchor;
        let [cx, cy] = lines.cell;
        let mut colours = [[0.0; 4]; 4];
        colours[0] = rgba(lines.color, 1.0);
        self.generator(
            batch,
            grid,
            "grid",
            &layer,
            Generator {
                operation: Some(lines.operation),
                points,
                flag: lines.cells,
                spots: [[ax, ay, cx, cy], [0.0; 4]],
                colours,
                k: [lines.border, lines.opacity, 0.0, 0.0],
                ..Generator::default()
            },
        );
    }

    /// Records `blinds` on `grid`. Each stripe's edges are hard where it
    /// wraps, so a turned layer gives every point.
    pub(crate) fn venetian_blinds(
        &self,
        batch: &mut Batch<'_>,
        grid: &Grid,
        blinds: &VenetianBlinds,
    ) {
        let (lines, points) = layer(blinds.back, grid.width, grid.height, true);
        let [sin, cos] = blinds.turn;
        self.generator(
            batch,
            grid,
            "venetian_blinds",
            &lines,
            Generator {
                points,
                k: [blinds.completion, sin, cos, blinds.width],
                k2: [blinds.feather, 0.0, 0.0, 0.0],
                ..Generator::default()
            },
        );
    }

    /// Records `wipe` on `grid`. A hard edge is a pixel soft, so a turned
    /// layer gives every point.
    pub(crate) fn linear_wipe(&self, batch: &mut Batch<'_>, grid: &Grid, wipe: &LinearWipe) {
        let (lines, points) = layer(wipe.back, grid.width, grid.height, true);
        let [sin, cos] = wipe.turn;
        let edge = wipe.edge(grid.width, grid.height);
        self.generator(
            batch,
            grid,
            "linear_wipe",
            &lines,
            Generator {
                points,
                k: [sin, cos, edge, wipe.feather],
                ..Generator::default()
            },
        );
    }

    /// Records `iris` on `grid`: nothing when it has no hole. A hard edge
    /// is a pixel soft, so a turned layer gives every point.
    pub(crate) fn iris_wipe(&self, batch: &mut Batch<'_>, grid: &Grid, iris: &IrisWipe) {
        let Some(outline) = &iris.outline else {
            return;
        };
        let (mut lines, points) = layer(iris.back, grid.width, grid.height, true);
        let extra = lines.len();
        lines.extend(outline.iter().flatten());
        self.generator(
            batch,
            grid,
            "iris_wipe",
            &lines,
            Generator {
                points,
                extra,
                n: u32::try_from(outline.len()).expect("a polygon's corners are few"),
                k: [iris.feather, 0.0, 0.0, 0.0],
                ..Generator::default()
            },
        );
    }

    /// Records `beam` on `grid`, as the pen draws it: its stroke worked
    /// out from a whole pixel near its start, and the layer's box, whose
    /// edge is hard, by the CPU.
    pub(crate) fn beam(&self, batch: &mut Batch<'_>, grid: &Grid, beam: &Beam) {
        let Some(stroke) = beam.stroke else {
            if beam.alone {
                // Nothing drawn, in place of the layer.
                self.generator(
                    batch,
                    grid,
                    "beam",
                    &[],
                    Generator {
                        flag: true,
                        ..Generator::default()
                    },
                );
            }
            return;
        };
        // As `Pen::new` and `Pen::tapered` work them out.
        let scale = beam.transform.determinant().abs().sqrt();
        let softness = stroke.softness.clamp(0.0, 1.0);
        let halves = stroke.thickness.map(|t| t * scale / 2.0);
        let (a, b) = (beam.transform * stroke.from, beam.transform * stroke.to);
        let widest = halves[0].max(halves[1]);
        let reach = widest + (widest * softness * 2.0).max(1.0) / 2.0;
        let (width, height) = (i64::from(grid.width), i64::from(grid.height));
        let x0 = ((a.x.min(b.x) - reach).floor() as i64).max(0);
        let x1 = ((a.x.max(b.x) + reach).ceil() as i64).min(width);
        let y0 = ((a.y.min(b.y) - reach).floor() as i64).max(0);
        let y1 = ((a.y.max(b.y) + reach).ceil() as i64).min(height);
        let ab = b - a;
        let length2 = ab.hypot2();
        let origin = Point::new(a.x.floor(), a.y.floor());
        let (lines, n) = beam_box(beam, grid.width, grid.height);
        let [inside, outside] = stroke.colors;
        self.generator(
            batch,
            grid,
            "beam",
            &lines,
            Generator {
                n,
                flag: beam.alone,
                spots: [
                    [
                        (a.x - origin.x) as f32,
                        (a.y - origin.y) as f32,
                        ab.x as f32,
                        ab.y as f32,
                    ],
                    [origin.x as f32, origin.y as f32, 0.0, 0.0],
                ],
                colours: [inside, outside, [0.0; 4], [0.0; 4]],
                k: [
                    length2 as f32,
                    length2.sqrt() as f32,
                    halves[0] as f32,
                    halves[1] as f32,
                ],
                k2: [softness as f32, 0.0, 0.0, 0.0],
                range: [x0, y0, x1, y1].map(|v| v as f32),
                ..Generator::default()
            },
        );
    }
}

/// Where `beam` may draw on a grid `width` by `height`, as `Pen::lay`
/// tests the layer's box, and how the kernel reads it: everywhere (0);
/// whether each column's and each row's centre lies inside, for a layer
/// the transform keeps upright (1); or each pixel's (2).
fn beam_box(beam: &Beam, width: u16, height: u16) -> (Vec<f32>, u32) {
    let Some(bounds) = beam.bounds else {
        return (Vec::new(), 0);
    };
    let back = beam.transform.inverse();
    let inside = |x: f64, y: f64| f32::from(u8::from(bounds.contains(back * Point::new(x, y))));
    let centre = |i: u16| f64::from(i) + 0.5;
    if turns(back) {
        return (
            per_pixel(width, height, |x, y| [inside(centre(x), centre(y))]),
            2,
        );
    }
    // Upright: a column's x and a row's y are the pixel's, whatever its
    // row and column.
    let at = |x: f64, y: f64| back * Point::new(x, y);
    let mut flags: Vec<f32> = (0..width)
        .map(|x| {
            let q = at(centre(x), 0.5);
            f32::from(u8::from(q.x >= bounds.x0 && q.x < bounds.x1))
        })
        .collect();
    flags.extend((0..height).map(|y| {
        let q = at(0.5, centre(y));
        f32::from(u8::from(q.y >= bounds.y0 && q.y < bounds.y1))
    }));
    (flags, 1)
}
