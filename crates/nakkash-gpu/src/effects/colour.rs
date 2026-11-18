//! The colour and channel effects: corrections of each pixel's colour
//! (`colour.wgsl`), channels rebuilt from levels of the layer or another
//! layer (`channel.wgsl`), and the sharpen and edge finder, which read a
//! pixel's neighbours (`neighbours.wgsl`).

use nakkash_raster::kurbo::Affine;
use nakkash_raster::{
    BlackWhite, BoxTest, ColorCycle, Curve, Curves, Exposure, Fill, FindEdges, HueSaturation,
    Invert, Level, Levels, Plan, Reading, SetMatte, Sharpen, ShiftChannels, Space, Tint,
};

use crate::Gpu;
use crate::grid::Grid;
use crate::kernels::{Family, Params, pixel_groups};
use crate::pool::Pooled;
use crate::record::Batch;

/// Whether `plan` is one of this family's and runs on the GPU: every
/// value of their plans does, apart from the mattes of [`matte_runs`].
pub(super) fn runs(plan: &Plan) -> bool {
    match plan {
        Plan::SetMatte(matte) => matte_runs(matte),
        _ => family(plan),
    }
}

/// Whether `plan` is one of this family's.
fn family(plan: &Plan) -> bool {
    matches!(
        plan,
        Plan::Curves(_)
            | Plan::SetMatte(_)
            | Plan::FindEdges(_)
            | Plan::Tint(_)
            | Plan::Fill(_)
            | Plan::ShiftChannels(_)
            | Plan::Sharpen(_)
            | Plan::Invert(_)
            | Plan::Levels(_)
            | Plan::HueSaturation(_)
            | Plan::Exposure(_)
            | Plan::BlackWhite(_)
            | Plan::ColorCycle(_)
    )
}

/// Whether `matte`'s level is one the GPU reads as the CPU does. The matte
/// is sampled between its pixels at a point the CPU works out in f64, the
/// GPU in f32; where the matte is the layer's own grid, or one laid on it
/// as it is, the two points are the same pixel centre but for the CPU's
/// rounding, some 1e-13 px. Most levels change by as little as the point
/// moves. A level read from the colour without its alpha (any but alpha
/// with `premultiply` off) or from the colour's proportions (hue and
/// saturation) does not, next to a transparent pixel: weighed by 1e-13 a
/// neighbour is its whole colour, by 0 it is nothing. There the CPU's last
/// bit decides between 0 and 1, so those mattes stay on the CPU.
fn matte_runs(matte: &SetMatte) -> bool {
    match matte.level {
        Level::Alpha | Level::Full | Level::Off => true,
        Level::Hue | Level::Saturation => false,
        _ => matte.reading == Reading::OverBlack,
    }
}

/// Whether `plan` reads another layer's drawing: the matte of `set-matte`
/// and the layer `color-cycle` adds.
pub(super) fn reads_input(plan: &Plan) -> bool {
    matches!(plan, Plan::SetMatte(_) | Plan::ColorCycle(_))
}

/// Records `plan`, one of this family's, on `grid` in `space`.
pub(super) fn run(gpu: &Gpu, batch: &mut Batch<'_>, grid: &mut Grid, plan: &Plan, space: &Space) {
    match plan {
        Plan::Curves(curves) => gpu.curves(batch, grid, curves),
        Plan::Tint(tint) => gpu.tint(batch, grid, tint),
        Plan::HueSaturation(hue) => gpu.hue_saturation(batch, grid, hue),
        Plan::Exposure(exposure) => gpu.exposure(batch, grid, exposure),
        Plan::Levels(levels) => gpu.levels(batch, grid, levels),
        Plan::Invert(invert) => gpu.invert(batch, grid, invert),
        Plan::BlackWhite(black_white) => gpu.black_white(batch, grid, black_white),
        Plan::Fill(fill) => gpu.fill(batch, grid, fill),
        Plan::SetMatte(matte) => gpu.set_matte(batch, grid, matte, space),
        Plan::ShiftChannels(shift) => gpu.shift_channels(batch, grid, shift),
        Plan::ColorCycle(cycle) => gpu.color_cycle(batch, grid, cycle, space),
        Plan::Sharpen(sharpen) => gpu.sharpen(batch, grid, sharpen),
        Plan::FindEdges(edges) => gpu.find_edges(batch, grid, edges),
        _ => unreachable!("`family` names every plan of the family"),
    }
}

/// The bits the kernels read a set of channels by: red 1, green 2, blue 4
/// and alpha 8.
fn channel_bits(channels: &[usize]) -> u32 {
    channels.iter().fold(0, |bits, &c| bits | 1 << c)
}

/// The number `channel.wgsl` names `level` by: its place in [`Level`].
fn level_number(level: Level) -> u32 {
    match level {
        Level::Red => 0,
        Level::Green => 1,
        Level::Blue => 2,
        Level::Alpha => 3,
        Level::Intensity => 4,
        Level::Value => 5,
        Level::Luminance => 6,
        Level::Hue => 7,
        Level::Lightness => 8,
        Level::Saturation => 9,
        Level::Full => 10,
        Level::Off => 11,
    }
}

/// The pixels of each row of a `width` by `height` grid whose centres
/// `test` contains, as the first and the end column of a run, two floats a
/// row (whole numbers below 2^24, so exact); `0, 0` for a row with none.
///
/// The centres are tested by [`BoxTest`]'s own arithmetic, so a pixel is
/// in a run exactly when the CPU finds it in the box. Along a row, each
/// coordinate of a centre is a sum of the column times one coefficient and
/// of fixed terms, rounded at each step, so it never falls as the column
/// rises if the coefficient is not below 0, and never rises otherwise:
/// each of the box's four sides holds on a run of columns that starts or
/// ends the row. A binary search finds where each side's run ends, and the
/// row's run is where all four hold.
pub(crate) fn spans(test: &BoxTest, width: u16, height: u16) -> Vec<f32> {
    let (w, r) = (usize::from(width), test.bounds);
    let mut out = Vec::with_capacity(2 * usize::from(height));
    for y in 0..usize::from(height) {
        let sides: [&dyn Fn(usize) -> bool; 4] = [
            &|x| test.centre(x, y).x >= r.x0,
            &|x| test.centre(x, y).x < r.x1,
            &|x| test.centre(x, y).y >= r.y0,
            &|x| test.centre(x, y).y < r.y1,
        ];
        let (mut start, mut end) = (0, w);
        for side in sides {
            if w == 0 {
                break;
            }
            match (side(0), side(w - 1)) {
                (true, true) => {}
                (false, false) => end = 0,
                // Rising: from the first column where it holds.
                (false, true) => start = start.max(first(w, side)),
                // Falling: up to the first column where it no longer does.
                (true, false) => end = end.min(first(w, |x| !side(x))),
            }
        }
        let (start, end) = if start < end { (start, end) } else { (0, 0) };
        out.extend([start as f32, end as f32]);
    }
    out
}

/// The first of `0..n` where `holds`, for a `holds` false and then true,
/// true at `n - 1`.
fn first(n: usize, holds: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, n - 1);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if holds(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

/// The uniform block of `channel.wgsl` after the count, the row and the
/// grid's size, field by field.
#[derive(Debug, Clone, Default)]
struct ChannelParams {
    levels: [u32; 4],
    reading: u32,
    flags: u32,
    matte: [u32; 2],
    add_mode: u32,
    modify: u32,
    lens: [u32; 4],
    place: [f32; 4],
    shift: [f32; 4],
    cycle: [f32; 4],
}

impl ChannelParams {
    /// The block's fields after `params`.
    fn after(&self, params: Params) -> Params {
        let mut params = params;
        for value in self
            .levels
            .iter()
            .chain([self.reading, self.flags].iter())
            .chain(&self.matte)
            .chain([self.add_mode, self.modify].iter())
            .chain(&self.lens)
        {
            params = params.u32(*value);
        }
        params.vec4(self.place).vec4(self.shift).vec4(self.cycle)
    }
}

impl Gpu {
    /// Records `entry` of `family` over `grid`'s pixels, writing `written`
    /// and reading `read`, with the parameters `params` adds after the
    /// count, the row and the grid's size.
    #[allow(clippy::too_many_arguments)]
    fn colour_kernel(
        &self,
        batch: &mut Batch<'_>,
        family: &Family,
        entry: &str,
        grid: &Grid,
        written: &wgpu::Buffer,
        read: &[Option<&wgpu::Buffer>],
        params: impl FnOnce(Params) -> Params,
    ) {
        let count = grid.count();
        let (groups, row) = pixel_groups(count);
        let params = params(
            Params::new()
                .u32(count)
                .u32(row)
                .u32(u32::from(grid.width))
                .u32(u32::from(grid.height)),
        );
        let read: Vec<&wgpu::Buffer> = read.iter().map(|b| b.unwrap_or(&self.empty)).collect();
        family.dispatch(
            &self.device,
            batch.encoder(),
            entry,
            &params,
            written,
            &read,
            groups,
        );
    }

    /// Records a kernel of `colour.wgsl` on `grid` in place: `channels`,
    /// `choice` and `boxed`, then the four vectors, as its `Params` lays
    /// them out.
    #[allow(clippy::too_many_arguments)]
    fn colour_pass(
        &self,
        batch: &mut Batch<'_>,
        entry: &str,
        grid: &Grid,
        table: Option<&Pooled>,
        [channels, choice, boxed]: [u32; 3],
        [numbers, more, first, second]: [[f32; 4]; 4],
    ) {
        self.colour_kernel(
            batch,
            &self.kernels.colour,
            entry,
            grid,
            grid.buffer(),
            &[table.map(Pooled::buffer)],
            |p| {
                p.u32(channels)
                    .u32(choice)
                    .u32(boxed)
                    .vec4(numbers)
                    .vec4(more)
                    .vec4(first)
                    .vec4(second)
            },
        );
    }

    /// Records `curves` on `grid`.
    pub(crate) fn curves(&self, batch: &mut Batch<'_>, grid: &Grid, curves: &Curves) {
        if !curves.any() {
            return;
        }
        let mut levels = vec![0.0; 5 * 256];
        let mut present = 0;
        for (t, table) in curves.tables.iter().enumerate() {
            if let Some(table) = table {
                levels[t * 256..(t + 1) * 256].copy_from_slice(&table.0);
                present |= 1 << t;
            }
        }
        let table = self.values(batch, &levels);
        self.colour_pass(
            batch,
            "curves",
            grid,
            Some(&table),
            [present, 0, 0],
            [[0.0; 4]; 4],
        );
    }

    /// Records `tint` on `grid`.
    pub(crate) fn tint(&self, batch: &mut Batch<'_>, grid: &Grid, tint: &Tint) {
        let ([br, bg, bb], [wr, wg, wb]) = (tint.black, tint.white);
        self.colour_pass(
            batch,
            "tint",
            grid,
            None,
            [0; 3],
            [
                [tint.amount, 0.0, 0.0, 0.0],
                [0.0; 4],
                [br, bg, bb, 0.0],
                [wr, wg, wb, 0.0],
            ],
        );
    }

    /// Records `hue` on `grid`.
    pub(crate) fn hue_saturation(&self, batch: &mut Batch<'_>, grid: &Grid, hue: &HueSaturation) {
        let (colorize, numbers) = match *hue {
            HueSaturation::Adjust {
                turn,
                saturation,
                lightness,
            } => (0, [turn, saturation, lightness, 0.0]),
            HueSaturation::Colorize {
                hue,
                saturation,
                lightness,
            } => (1, [hue, saturation, lightness, 0.0]),
        };
        self.colour_pass(
            batch,
            "hue_saturation",
            grid,
            None,
            [0, colorize, 0],
            [numbers, [0.0; 4], [0.0; 4], [0.0; 4]],
        );
    }

    /// Records `exposure` on `grid`.
    pub(crate) fn exposure(&self, batch: &mut Batch<'_>, grid: &Grid, exposure: &Exposure) {
        // The powers the CPU raises to, worked out as it does.
        let (power, encode) = (1.0 / exposure.gamma, 1.0 / 2.4);
        self.colour_pass(
            batch,
            "exposure",
            grid,
            None,
            [0, u32::from(exposure.linear), 0],
            [
                [exposure.scale, exposure.offset, power, encode],
                [0.0; 4],
                [0.0; 4],
                [0.0; 4],
            ],
        );
    }

    /// Records `levels` on `grid`.
    pub(crate) fn levels(&self, batch: &mut Batch<'_>, grid: &Grid, levels: &Levels) {
        let power = if levels.gamma > 0.0 {
            1.0 / levels.gamma
        } else {
            0.0
        };
        self.colour_pass(
            batch,
            "levels",
            grid,
            None,
            [channel_bits(levels.channels), 0, 0],
            [
                [
                    levels.input_black,
                    levels.input_white,
                    levels.output_black,
                    levels.output_white,
                ],
                [levels.gamma, power, 0.0, 0.0],
                [0.0; 4],
                [0.0; 4],
            ],
        );
    }

    /// Records `invert` on `grid`.
    pub(crate) fn invert(&self, batch: &mut Batch<'_>, grid: &Grid, invert: &Invert) {
        self.colour_pass(
            batch,
            "invert",
            grid,
            None,
            [channel_bits(invert.channels), 0, 0],
            [[invert.blend, 0.0, 0.0, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]],
        );
    }

    /// Records `black_white` on `grid`.
    pub(crate) fn black_white(&self, batch: &mut Batch<'_>, grid: &Grid, black_white: &BlackWhite) {
        let [s0, s1, s2, s3, s4, s5] = black_white.share;
        self.colour_pass(
            batch,
            "black_white",
            grid,
            None,
            [0; 3],
            [
                [
                    black_white.hue,
                    black_white.saturation,
                    black_white.tint,
                    0.0,
                ],
                [0.0; 4],
                [s0, s1, s2, s3],
                [s4, s5, 0.0, 0.0],
            ],
        );
    }

    /// Records `fill` on `grid`.
    pub(crate) fn fill(&self, batch: &mut Batch<'_>, grid: &Grid, fill: &Fill) {
        let inside = fill
            .inside
            .filter(|_| fill.inverted)
            .map(|test| self.values(batch, &spans(&test, grid.width, grid.height)));
        let [r, g, b] = fill.rgb;
        self.colour_pass(
            batch,
            "fill",
            grid,
            inside.as_ref(),
            [0, u32::from(fill.inverted), u32::from(inside.is_some())],
            [
                [fill.share, 0.0, 0.0, 0.0],
                [0.0; 4],
                [r, g, b, 0.0],
                [0.0; 4],
            ],
        );
    }

    /// Records `matte` on `grid` in `space`: the matte is the drawing the
    /// effect reads, or the layer's own as it was.
    pub(crate) fn set_matte(
        &self,
        batch: &mut Batch<'_>,
        grid: &Grid,
        matte: &SetMatte,
        space: &Space,
    ) {
        let test = matte.area.test(grid.width, grid.height);
        let target = test.bounds;
        let (source, matte_box) = match self.input(batch, space) {
            Some(input) => (input, matte.matte_bounds.unwrap_or(target)),
            None => (self.duplicate_grid(batch, grid), target),
        };
        // `Lay::point` as an affine map of the layer's coordinates: the
        // matte's box stretched over the layer's, or their centres matched.
        let lay = if matte.stretch {
            let sx = matte_box.width() / target.width().max(1e-6);
            let sy = matte_box.height() / target.height().max(1e-6);
            Affine::new([
                sx,
                0.0,
                0.0,
                sy,
                matte_box.x0 - target.x0 * sx,
                matte_box.y0 - target.y0 * sy,
            ])
        } else {
            Affine::translate(matte_box.center() - target.center())
        };
        // From a px's column and row to the matte's point sampled at its
        // centre, half a px back as `Pixels::sample` starts, in f64.
        let place = Affine::translate((-0.5, -0.5))
            * matte.area.transform
            * lay
            * test.back
            * Affine::translate((0.5, 0.5));
        let [a, b, c, d, e, f] = place.as_coeffs().map(|v| v as f32);
        let spans =
            (!matte.composite).then(|| self.values(batch, &spans(&test, grid.width, grid.height)));
        let flags = u32::from(matte.inverted) | u32::from(matte.composite) << 1;
        let reading = match matte.reading {
            Reading::Straight => 0,
            Reading::OverBlack => 1,
        };
        self.colour_kernel(
            batch,
            &self.kernels.channel,
            "set_matte",
            grid,
            grid.buffer(),
            &[Some(source.buffer()), spans.as_ref().map(Pooled::buffer)],
            |p| {
                ChannelParams {
                    levels: [level_number(matte.level), 0, 0, 0],
                    reading,
                    flags,
                    matte: [u32::from(source.width), u32::from(source.height)],
                    place: [a, b, c, d],
                    shift: [e, f, 0.0, 0.0],
                    ..ChannelParams::default()
                }
                .after(p)
            },
        );
    }

    /// Records `shift` on `grid`.
    pub(crate) fn shift_channels(&self, batch: &mut Batch<'_>, grid: &Grid, shift: &ShiftChannels) {
        let fills = shift.fills();
        let spans = fills.then(|| {
            let test = shift.area.test(grid.width, grid.height);
            self.values(batch, &spans(&test, grid.width, grid.height))
        });
        let [alpha, red, green, blue] = shift.levels.map(level_number);
        self.colour_kernel(
            batch,
            &self.kernels.channel,
            "shift_channels",
            grid,
            grid.buffer(),
            &[None, spans.as_ref().map(Pooled::buffer)],
            |p| {
                ChannelParams {
                    levels: [alpha, red, green, blue],
                    flags: u32::from(fills) << 2,
                    ..ChannelParams::default()
                }
                .after(p)
            },
        );
    }

    /// Records `cycle` on `grid` in `space`, adding the drawing the effect
    /// reads, if it reads one.
    pub(crate) fn color_cycle(
        &self,
        batch: &mut Batch<'_>,
        grid: &Grid,
        cycle: &ColorCycle,
        space: &Space,
    ) {
        let added = self.input(batch, space);
        let mut points = vec![0.0; 4 * Curve::MAX * 2];
        for (c, curve) in cycle.cycle.iter().enumerate() {
            for (k, [x, y]) in curve.points().iter().enumerate() {
                let at = (c * Curve::MAX + k) * 2;
                points[at] = *x;
                points[at + 1] = *y;
            }
        }
        let points = self.values(batch, &points);
        let lens = cycle.cycle.map(|curve| curve.points().len() as u32);
        let flags = u32::from(added.is_some()) << 3
            | u32::from(cycle.smooth) << 4
            | u32::from(cycle.modify_alpha) << 5;
        // The shift in turns, as the CPU works it out for every pixel.
        let turn = cycle.shift / 360.0;
        self.colour_kernel(
            batch,
            &self.kernels.channel,
            "color_cycle",
            grid,
            grid.buffer(),
            &[added.as_ref().map(Grid::buffer), Some(points.buffer())],
            |p| {
                ChannelParams {
                    levels: [level_number(cycle.from), level_number(cycle.add_from), 0, 0],
                    flags,
                    add_mode: u32::try_from(cycle.add_mode).unwrap_or(u32::MAX),
                    modify: u32::try_from(cycle.modify).unwrap_or(u32::MAX),
                    lens,
                    cycle: [cycle.repetitions, turn, cycle.blend, 0.0],
                    ..ChannelParams::default()
                }
                .after(p)
            },
        );
    }

    /// Records `sharpen` on `grid`, into a new buffer.
    pub(crate) fn sharpen(&self, batch: &mut Batch<'_>, grid: &mut Grid, sharpen: &Sharpen) {
        if sharpen.amount <= 0.0 {
            return;
        }
        self.neighbours(batch, grid, "sharpen", 0, sharpen.amount);
    }

    /// Records `edges` on `grid`, into a new buffer.
    pub(crate) fn find_edges(&self, batch: &mut Batch<'_>, grid: &mut Grid, edges: &FindEdges) {
        self.neighbours(
            batch,
            grid,
            "find_edges",
            u32::from(edges.light),
            edges.blend,
        );
    }

    /// Records `entry` of `neighbours.wgsl` reading `grid` into a new
    /// buffer, which becomes the grid's.
    fn neighbours(&self, batch: &mut Batch<'_>, grid: &mut Grid, entry: &str, light: u32, k: f32) {
        let out = self.grid(grid.width, grid.height);
        self.colour_kernel(
            batch,
            &self.kernels.neighbours,
            entry,
            grid,
            out.buffer(),
            &[Some(grid.buffer())],
            |p| p.u32(light).f32(k),
        );
        grid.pixels = out.pixels;
    }
}
