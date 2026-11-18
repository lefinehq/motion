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

use vello_cpu::kurbo::{Affine, Vec2};

use crate::colour::{LUMA, choice, number, rgb};
use crate::composite::Operation;
use crate::effect::{ParamValue, Space};
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
        for _ in 0..passes {
            box_pass(line, whole, part, edges, padded);
        }
    };
    match way {
        0 => par::rows(data, width, Vec::new, |_, row, padded| line(row, padded)),
        _ => par::columns(data, width, height, Vec::new, line),
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

/// `glow`'s steps: the colours brighter than `threshold`, blurred by
/// `radius` as a blurriness, times `intensity`, held to 0 to 1. With
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
        luma: LUMA,
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
    let mut light = pixels.clone();
    par::each(light.data_mut(), |_, p| {
        let a = p[3];
        let luma = match a > 0.0 {
            true => (p[0] * weights[0] + p[1] * weights[1] + p[2] * weights[2]) / a,
            false => 0.0,
        };
        if luma < threshold {
            *p = [0.0; 4];
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

/// `sharpen`: each pixel moved away from the mean of its four neighbours
/// by `amount`/100 of the difference, a 3×3 unsharp mask in frame px.
pub(crate) fn sharpen(pixels: &mut Pixels, values: &[ParamValue], _: &Space) {
    let amount = number(values, 0) / 100.0;
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

/// `find-edges`: each colour channel's slope across the 3×3 pixels around
/// each pixel, by the Sobel operator scaled so that a step from 0 to 1
/// gives 1, and at most 1: light where the colour changes and black where
/// it is even, or the inverse with `lines` `dark-on-light`. Alpha stays,
/// and `blend` of the layer as it was shows through. The pixels are the
/// frame's; past the edges of the grid the edge pixels repeat.
pub(crate) fn find_edges(pixels: &mut Pixels, values: &[ParamValue], _: &Space) {
    let (blend, light) = (number(values, 0), choice(values, 1) == 1);
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
