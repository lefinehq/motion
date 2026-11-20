//! Blurs and the effects built on them: one family of box passes, which
//! three times over approach a Gaussian, run along rows and columns of a
//! layer's pixels.
//!
//! A box pass of radius `r` averages the `2⌊r⌋ + 1` pixels around each,
//! and the two beyond them by what `r` has over `⌊r⌋`, so its width moves
//! smoothly with `r` as an animated blur needs. Three passes whose radius
//! is chosen for a variance give a Gaussian of that variance (Gwosdek et
//! al., "Theoretical foundations of Gaussian convolution by extended box
//! filtering", 2011), in time that does not grow with the blur.

use std::sync::atomic::{AtomicBool, Ordering};

use vello_cpu::kurbo::{Affine, Vec2};

use crate::colour::{choice, number, rgb};

/// The weights of red, green and blue in the luma `glow` holds against
/// its threshold: Rec. 709's, measured on the corpus against reference
/// renders (belief G3 in the import's evidence).
pub const GLOW_LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
use crate::composite::Operation;
use crate::effect::{ParamValue, Space};
use crate::memory::{self, Start};
use crate::par;
use crate::pixels::{Pixel, Pixels};
use crate::plan::{Blur, Glow, GlowComposite, Passes, Shadow};

/// A Gaussian's standard deviation per unit of a project's blurriness.
/// Measured on the corpus against reference renders (see the effects plan).
pub const SIGMA_PER_BLURRINESS: f32 = 0.3;

/// What lies beyond the edges of the pixels while blurring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edges {
    /// Nothing: colour fades out at the edges.
    Transparent,
    /// The edge pixels, repeated: colour stays solid up to the edges.
    Repeat,
}

impl Edges {
    fn from_choice(choice: usize) -> Edges {
        match choice {
            0 => Edges::Transparent,
            _ => Edges::Repeat,
        }
    }
}

/// Which way a blur runs, from a `dimensions` choice: both ways (0),
/// along rows (1) or along columns (2).
fn ways(choice: usize) -> [bool; 2] {
    match choice {
        1 => [true, false],
        2 => [false, true],
        _ => [true, true],
    }
}

/// How many pixels one unit of the layer's x and y spans.
fn scales(space: &Space) -> [f32; 2] {
    let [a, b, c, d, ..] = space.transform.as_coeffs();
    [a.hypot(b) as f32, c.hypot(d) as f32]
}

/// One extended box pass along `line`, in place: radius `whole` plus the
/// share `part` of one pixel more on each side.
///
/// Where the window and both its partial pixels lie on a run of one pixel,
/// the same bits again and again ([D93]), the pass writes the pixel it
/// worked out at the run's start over the rest of it instead of working
/// each out: every slide inside the run takes a value from the sum and
/// adds the same value back, `v − v = +0.0`, and a sum plus `+0.0` is the
/// sum (it is never `−0.0`: it starts at `+0.0`, and an exact zero comes
/// out `+0.0`), so every pixel there has the sum, the partial pixels and
/// the bits of the first. Runs of a value that is not finite, and windows
/// whose sum is not, are worked out pixel by pixel as before.
///
/// [D93]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d93-glow-and-blurs-do-less-of-the-same-work
fn box_pass(line: &mut [Pixel], whole: usize, part: f32, edges: Edges, padded: &mut Vec<Pixel>) {
    let n = line.len();
    if n == 0 || (whole == 0 && part <= 0.0) {
        return;
    }
    let pad = whole + 1;
    padded.clear();
    let beyond = |at: usize| match edges {
        Edges::Transparent => [0.0; 4],
        Edges::Repeat => line[at],
    };
    padded.extend((0..pad).map(|_| beyond(0)));
    padded.extend_from_slice(line);
    padded.extend((0..pad).map(|_| beyond(n - 1)));
    let width = (2 * whole + 1) as f32 + 2.0 * part;
    // The window's sum, slid one pixel at a time, in f64 against drift.
    let mut sum = [0.0f64; 4];
    for p in &padded[1..2 * whole + 2] {
        for c in 0..4 {
            sum[c] += f64::from(p[c]);
        }
    }
    // Pixel i of `line` sits at i + pad in `padded`; the window runs from
    // i + 1 to i + 2·whole + 1, its two partial pixels at i and i + span.
    let span = 2 * whole + 2;
    let same = |a: &Pixel, b: &Pixel| a.map(f32::to_bits) == b.map(f32::to_bits);
    // How many pixels up to the head of the window, padded[i + span] and
    // those before it, are the same bits.
    let mut run = 1 + padded[..span]
        .iter()
        .rev()
        .take_while(|p| same(p, &padded[span]))
        .count();
    let mut i = 0;
    while i < n {
        let (low, high) = (padded[i], padded[i + span]);
        let mut out = [0.0f32; 4];
        for c in 0..4 {
            out[c] = ((sum[c] + f64::from(part) * f64::from(low[c] + high[c])) as f32) / width;
        }
        line[i] = out;
        if run > span && high.iter().all(|c| c.is_finite()) && sum.iter().all(|s| s.is_finite()) {
            // padded[i..=i + span] is one run: it lasts to `end`, and every
            // pixel whose window and partial pixels it holds is this one.
            let mut end = i + span;
            while end + 1 < padded.len() && same(&padded[end + 1], &high) {
                end += 1;
            }
            let last = (end - span).min(n - 1);
            line[i + 1..=last].fill(out);
            run += last - i;
            i = last;
        }
        if i + 1 < n {
            let (leaving, entering) = (padded[i + 1], padded[i + span]);
            for c in 0..4 {
                sum[c] += f64::from(entering[c]) - f64::from(leaving[c]);
            }
            run = match same(&padded[i + 1 + span], &padded[i + span]) {
                true => run + 1,
                false => 1,
            };
        }
        i += 1;
    }
}

/// [`glow`] as it was before it left light of nothing alone: its
/// reference ([D90], rule 7), which tests hold it to bit for bit.
///
/// [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget
#[cfg(test)]
pub(crate) fn glow_reference(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Glow {
        threshold,
        luma: weights,
        blur: spread,
        intensity,
        composite,
        operation,
    } = glow_plan(values, space);
    let source = pixels.data();
    let mut light = Pixels::made(pixels.width(), pixels.height(), |i| {
        let p = source[i];
        let a = p[3];
        let luma = match a > 0.0 {
            true => (p[0] * weights[0] + p[1] * weights[1] + p[2] * weights[2]) / a,
            false => 0.0,
        };
        match luma < threshold {
            true => [0.0; 4],
            false => p,
        }
    });
    blur(&mut light, &spread);
    par::each(light.data_mut(), |_, p| {
        *p = p.map(|c| (c * intensity).min(1.0));
    });
    match composite {
        GlowComposite::GlowOnly => *pixels = light,
        GlowComposite::BehindOriginal => {
            operation.pixels(&mut light, pixels);
            *pixels = light;
        }
        GlowComposite::WithOriginal => operation.pixels(pixels, &light),
    }
}

/// [`box_pass`] as it was before it stepped over runs: its reference
/// ([D90], rule 7), which tests hold it to bit for bit.
///
/// [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget
#[cfg(test)]
fn box_pass_reference(
    line: &mut [Pixel],
    whole: usize,
    part: f32,
    edges: Edges,
    padded: &mut Vec<Pixel>,
) {
    let n = line.len();
    if n == 0 || (whole == 0 && part <= 0.0) {
        return;
    }
    let pad = whole + 1;
    padded.clear();
    let beyond = |at: usize| match edges {
        Edges::Transparent => [0.0; 4],
        Edges::Repeat => line[at],
    };
    padded.extend((0..pad).map(|_| beyond(0)));
    padded.extend_from_slice(line);
    padded.extend((0..pad).map(|_| beyond(n - 1)));
    let width = (2 * whole + 1) as f32 + 2.0 * part;
    // The window's sum, slid one pixel at a time, in f64 against drift.
    let mut sum = [0.0f64; 4];
    for p in &padded[1..2 * whole + 2] {
        for c in 0..4 {
            sum[c] += f64::from(p[c]);
        }
    }
    for (i, out) in line.iter_mut().enumerate() {
        // Pixel i of `line` sits at i + pad in `padded`; the window runs
        // from i + 1 to i + 2·whole + 1, its two partial pixels either side.
        let (low, high) = (padded[i], padded[i + 2 * whole + 2]);
        for c in 0..4 {
            out[c] = ((sum[c] + f64::from(part) * f64::from(low[c] + high[c])) as f32) / width;
        }
        if i + 1 < n {
            let (leaving, entering) = (padded[i + 1], padded[i + 2 * whole + 2]);
            for c in 0..4 {
                sum[c] += f64::from(entering[c]) - f64::from(leaving[c]);
            }
        }
    }
}

/// Runs `passes` box passes of `radius` along rows (`way` 0) or columns (1).
fn along(pixels: &mut Pixels, way: usize, radius: f32, passes: usize, edges: Edges) {
    let radius = radius.max(0.0);
    let (whole, part) = (radius.floor() as usize, radius.fract());
    if passes == 0 || (whole == 0 && part <= 0.0) {
        return;
    }
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let data = pixels.data_mut();
    let line = |line: &mut [Pixel], padded: &mut Vec<Pixel>| {
        // A line of nothing but `+0.0` stays as it is ([D90], K3): every
        // sum stays `+0.0`, and `+0.0` over the width is `+0.0`, at any
        // radius and with either edges. (A `-0.0` comes out `+0.0`, so a
        // line holding one is passed as any other.)
        //
        // [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget
        if line.iter().all(|p| p.iter().all(|c| c.to_bits() == 0)) {
            return;
        }
        for _ in 0..passes {
            box_pass(line, whole, part, edges, padded);
        }
    };
    match way {
        0 => par::rows(data, width, Vec::new, |_, row, padded| line(row, padded)),
        _ => {
            // Columns of nothing but `+0.0` are left as they are, as above,
            // so their bands are not even copied out ([D93]); the room the
            // others are copied to is a grid of the budget's.
            //
            // [D93]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d93-glow-and-blurs-do-less-of-the-same-work
            let held = par::columns_holding(data, width, |p| p.iter().any(|c| c.to_bits() != 0));
            if !held.contains(&true) {
                return;
            }
            let mut bands = memory::grid(width * height, Start::Written);
            par::columns_in(data, width, height, &mut bands, |x| held[x], Vec::new, line);
            memory::release(bands);
        }
    }
}

/// The radius of each of three box passes whose sum has variance
/// `sigma`², by the extended box's own variance.
pub(crate) fn gaussian_radius(sigma: f32) -> f32 {
    const PASSES: f32 = 3.0;
    let variance = sigma * sigma / PASSES;
    // A box of whole radius l and edge share a has variance
    // (l(l+1)(2l+1)/3 + 2a(l+1)²) / (2l + 1 + 2a); solve for a.
    let l = (((12.0 * variance + 1.0).sqrt() - 1.0) / 2.0).floor();
    let a = ((2.0 * l + 1.0) * (variance - l * (l + 1.0) / 3.0))
        / (2.0 * ((l + 1.0) * (l + 1.0) - variance));
    l + a.clamp(0.0, 1.0)
}

/// Blurs `pixels` with a Gaussian of standard deviation `sigma` px each way.
pub(crate) fn gaussian(pixels: &mut Pixels, sigma: [f32; 2], edges: Edges) {
    blur(pixels, &Blur::gaussian(sigma, edges));
}

/// Runs `blur`'s passes on `pixels`: along rows, then along columns.
pub(crate) fn blur(pixels: &mut Pixels, blur: &Blur) {
    for (way, Passes { radius, count }) in [blur.rows, blur.columns].into_iter().enumerate() {
        along(pixels, way, radius, count, blur.edges);
    }
}

/// `gaussian-blur`'s passes: a Gaussian whose standard deviation is
/// [`SIGMA_PER_BLURRINESS`] of `blurriness`, in the layer's px.
pub(crate) fn gaussian_blur_plan(values: &[ParamValue], space: &Space) -> Blur {
    let blurriness = number(values, 0) * SIGMA_PER_BLURRINESS;
    let [x, y] = ways(choice(values, 1));
    let [sx, sy] = scales(space);
    let sigma = [
        if x { blurriness * sx } else { 0.0 },
        if y { blurriness * sy } else { 0.0 },
    ];
    Blur::gaussian(sigma, Edges::from_choice(choice(values, 2)))
}

/// `gaussian-blur`, see [`gaussian_blur_plan`].
pub(crate) fn gaussian_blur(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    blur(pixels, &gaussian_blur_plan(values, space));
}

/// `box-blur`'s passes: `iterations` box passes of `radius`, in the
/// layer's px.
pub(crate) fn box_blur_plan(values: &[ParamValue], space: &Space) -> Blur {
    let (radius, iterations) = (
        number(values, 0),
        number(values, 1).round().max(1.0) as usize,
    );
    let [x, y] = ways(choice(values, 2));
    let edges = Edges::from_choice(choice(values, 3));
    let [sx, sy] = scales(space);
    let passes = |on: bool, scale: f32| match on {
        true => Passes {
            radius: radius * scale,
            count: iterations,
        },
        false => Passes::NONE,
    };
    Blur {
        rows: passes(x, sx),
        columns: passes(y, sy),
        edges,
    }
}

/// `box-blur`, see [`box_blur_plan`].
pub(crate) fn box_blur(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    blur(pixels, &box_blur_plan(values, space));
}

/// `drop-shadow`'s steps: the layer's alpha in `color` at `opacity`, moved
/// `distance` px towards `direction` (degrees clockwise from up), blurred
/// by `softness` as a blurriness, and drawn behind the layer, or alone.
pub(crate) fn drop_shadow_plan(values: &[ParamValue], space: &Space) -> Shadow {
    let (color, opacity) = (rgb(values, 0), number(values, 1));
    let (direction, distance) = (number(values, 2).to_radians(), number(values, 3));
    let softness = number(values, 4) * SIGMA_PER_BLURRINESS;
    let alone = choice(values, 5) == 1;
    let towards = Vec2::new(
        f64::from(direction.sin() * distance),
        f64::from(-direction.cos() * distance),
    );
    let linear = Affine::new({
        let [a, b, c, d, ..] = space.transform.as_coeffs();
        [a, b, c, d, 0.0, 0.0]
    });
    let offset = linear * towards.to_point();
    let [sx, sy] = scales(space);
    Shadow {
        color,
        opacity,
        offset: offset.to_vec2(),
        blur: Blur::gaussian([softness * sx, softness * sy], Edges::Transparent),
        alone,
    }
}

/// `drop-shadow`, see [`drop_shadow_plan`].
pub(crate) fn drop_shadow(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Shadow {
        color,
        opacity,
        offset,
        blur: softened,
        alone,
    } = drop_shadow_plan(values, space);
    let mut shadow = pixels.transformed(Affine::translate(offset));
    par::each(shadow.data_mut(), |_, p| {
        let a = p[3] * opacity;
        *p = [color[0] * a, color[1] * a, color[2] * a, a];
    });
    blur(&mut shadow, &softened);
    if alone {
        *pixels = shadow;
        return;
    }
    // The layer over its shadow.
    par::each_with(pixels.data_mut(), shadow.data(), |_, p, s| {
        let under = 1.0 - p[3];
        for c in 0..4 {
            p[c] += s[c] * under;
        }
    });
}

/// How `glow` combines its light with the layer, by name in
/// [`crate::composite::OPERATIONS`]; the first is the default.
pub(crate) const GLOW_OPERATIONS: &[&str] = &[
    "add",
    "normal",
    "multiply",
    "dissolve",
    "screen",
    "overlay",
    "soft-light",
    "hard-light",
    "darken",
    "lighten",
    "difference",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "color-dodge",
    "color-burn",
    "exclusion",
    "stencil-alpha",
    "stencil-luma",
    "silhouette-alpha",
    "silhouette-luma",
    "premultiplied-over",
    "alpha-add",
];

/// `glow`'s steps: the colours whose Rec. 709 luma ([`GLOW_LUMA`]) is
/// past `threshold`, blurred by `radius` as a blurriness, times
/// `intensity`, held to 0 to 1. With
/// `composite` `with-original` the light is laid over the layer by
/// `operation`; with `behind-original` the layer is laid over the light by
/// it; with `glow-only` the light is drawn alone.
pub(crate) fn glow_plan(values: &[ParamValue], space: &Space) -> Glow {
    let (threshold, radius, intensity) = (number(values, 0), number(values, 1), number(values, 2));
    let [x, y] = ways(choice(values, 3));
    let composite = match choice(values, 4) {
        1 => GlowComposite::GlowOnly,
        2 => GlowComposite::BehindOriginal,
        _ => GlowComposite::WithOriginal,
    };
    let operation = Operation::named(GLOW_OPERATIONS[choice(values, 5)])
        .expect("every glow operation is a row of the operations");
    let sigma = radius * SIGMA_PER_BLURRINESS;
    let [sx, sy] = scales(space);
    Glow {
        threshold,
        luma: GLOW_LUMA,
        blur: Blur::gaussian(
            [
                if x { sigma * sx } else { 0.0 },
                if y { sigma * sy } else { 0.0 },
            ],
            Edges::Transparent,
        ),
        intensity,
        composite,
        operation,
    }
}

/// `glow`, see [`glow_plan`].
pub(crate) fn glow(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Glow {
        threshold,
        luma: weights,
        blur: spread,
        intensity,
        composite,
        operation,
    } = glow_plan(values, space);
    // The bright parts, made from the layer as they are read: the layer is
    // not copied first. Whether any of them is other than `+0.0` is noted
    // as they are made; it is an "or", the same at any split.
    let source = pixels.data();
    let shines = AtomicBool::new(false);
    let mut light = Pixels::made(pixels.width(), pixels.height(), |i| {
        let p = source[i];
        let a = p[3];
        let luma = match a > 0.0 {
            true => (p[0] * weights[0] + p[1] * weights[1] + p[2] * weights[2]) / a,
            false => 0.0,
        };
        let lit = match luma < threshold {
            true => [0.0; 4],
            false => p,
        };
        if !shines.load(Ordering::Relaxed) && lit.iter().any(|c| c.to_bits() != 0) {
            shines.store(true, Ordering::Relaxed);
        }
        lit
    });
    // Light of nothing but `+0.0` blurs to itself, every line of it left
    // as it is (K3), and an intensity that is finite and not negative
    // takes `+0.0` to `+0.0`, held to 1 or not: so where nothing passes
    // the threshold neither is worked out, and the light laid with the
    // layer is the `+0.0` it would have been ([D93]).
    //
    // [D93]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d93-glow-and-blurs-do-less-of-the-same-work
    let keeps_nothing = intensity.is_finite() && intensity.is_sign_positive();
    if shines.into_inner() || !keeps_nothing {
        blur(&mut light, &spread);
        par::each(light.data_mut(), |_, p| {
            *p = p.map(|c| (c * intensity).min(1.0));
        });
    }
    match composite {
        GlowComposite::GlowOnly => *pixels = light,
        GlowComposite::BehindOriginal => {
            operation.pixels(&mut light, pixels);
            *pixels = light;
        }
        GlowComposite::WithOriginal => operation.pixels(pixels, &light),
    }
}

/// `sharpen`'s plan: each pixel moved away from the mean of its four
/// neighbours by `amount` of the difference, each channel held to 0 to 1
/// and each colour to the alpha, a 3×3 unsharp mask in frame px; past the
/// grid's edges a pixel stands for its missing neighbours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sharpen {
    /// How far a pixel moves, as a share of its difference from the mean;
    /// nothing changes when not above 0.
    pub amount: f32,
}

/// [`Sharpen`] from `sharpen`'s values.
pub(crate) fn sharpen_plan(values: &[ParamValue], _: &Space) -> Sharpen {
    Sharpen {
        amount: number(values, 0) / 100.0,
    }
}

/// `sharpen`, see [`Sharpen`].
pub(crate) fn sharpen(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Sharpen { amount } = sharpen_plan(values, space);
    if amount <= 0.0 {
        return;
    }
    let source = pixels.clone();
    let width = i64::from(source.width());
    let row_width = usize::from(source.width());
    par::rows(
        pixels.data_mut(),
        row_width,
        || (),
        |y, row, ()| {
            let y = y as i64;
            for (x, p) in row.iter_mut().enumerate() {
                let x = x as i64;
                let at = |dx: i64, dy: i64| {
                    let (nx, ny) = (x + dx, y + dy);
                    let inside =
                        nx >= 0 && ny >= 0 && nx < width && ny < i64::from(source.height());
                    if inside {
                        source.get(nx, ny)
                    } else {
                        source.get(x, y)
                    }
                };
                let around = [at(-1, 0), at(1, 0), at(0, -1), at(0, 1)];
                let me = source.get(x, y);
                for c in 0..4 {
                    let mean = around.iter().map(|n| n[c]).sum::<f32>() / 4.0;
                    p[c] = (me[c] + (me[c] - mean) * amount).clamp(0.0, 1.0);
                }
                let a = p[3];
                for c in &mut p[..3] {
                    *c = c.min(a);
                }
            }
        },
    );
}

/// `find-edges`' plan: each colour channel's slope across the 3×3 pixels
/// around each pixel, by the Sobel operator scaled so that a step from 0 to
/// 1 gives 1, and at most 1: light where the colour changes and black where
/// it is even, or the inverse. Alpha stays, and `blend` of the layer as it
/// was shows through. The pixels are the frame's; past the edges of the
/// grid the edge pixels repeat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FindEdges {
    /// How much of the layer as it was shows through, 0 to 1.
    pub blend: f32,
    /// Whether edges are light on dark (`lines` `light-on-dark`), rather
    /// than dark on light.
    pub light: bool,
}

/// [`FindEdges`] from `find-edges`' values.
pub(crate) fn find_edges_plan(values: &[ParamValue], _: &Space) -> FindEdges {
    FindEdges {
        blend: number(values, 0),
        light: choice(values, 1) == 1,
    }
}

/// `find-edges`, see [`FindEdges`].
pub(crate) fn find_edges(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let FindEdges { blend, light } = find_edges_plan(values, space);
    let source = pixels.clone();
    let (width, height) = (i64::from(source.width()), i64::from(source.height()));
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = (i as i64 % width, i as i64 / width);
        let at = |dx: i64, dy: i64| {
            source.get((x + dx).clamp(0, width - 1), (y + dy).clamp(0, height - 1))
        };
        let me = source.get(x, y);
        let a = me[3];
        for c in 0..3 {
            let v = |dx, dy| at(dx, dy)[c];
            let across =
                (v(1, -1) + 2.0 * v(1, 0) + v(1, 1)) - (v(-1, -1) + 2.0 * v(-1, 0) + v(-1, 1));
            let down =
                (v(-1, 1) + 2.0 * v(0, 1) + v(1, 1)) - (v(-1, -1) + 2.0 * v(0, -1) + v(1, -1));
            let edge = (across.hypot(down) / 4.0).min(1.0);
            let line = if light { edge } else { 1.0 - edge };
            // The colour channels stay premultiplied by the layer's alpha.
            p[c] = line * a * (1.0 - blend) + me[c] * blend;
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/blur.rs"]
mod tests;
