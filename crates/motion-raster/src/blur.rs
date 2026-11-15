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
use crate::effect::{ParamValue, Space};
use crate::pixels::{Pixel, Pixels};

/// A Gaussian's standard deviation per unit of a project's blurriness.
/// Measured on the corpus against reference renders (see the effects plan).
pub const SIGMA_PER_BLURRINESS: f32 = 0.3;

/// What lies beyond the edges of the pixels while blurring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edges {
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
    let mut padded = Vec::new();
    match way {
        0 => {
            for row in data.chunks_exact_mut(width) {
                for _ in 0..passes {
                    box_pass(row, whole, part, edges, &mut padded);
                }
            }
        }
        _ => {
            let mut column = vec![[0.0; 4]; height];
            for x in 0..width {
                for (y, p) in column.iter_mut().enumerate() {
                    *p = data[y * width + x];
                }
                for _ in 0..passes {
                    box_pass(&mut column, whole, part, edges, &mut padded);
                }
                for (y, p) in column.iter().enumerate() {
                    data[y * width + x] = *p;
                }
            }
        }
    }
}

/// The radius of each of three box passes whose sum has variance
/// `sigma`², by the extended box's own variance.
fn gaussian_radius(sigma: f32) -> f32 {
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
    for (way, sigma) in sigma.into_iter().enumerate() {
        if sigma > 0.0 {
            along(pixels, way, gaussian_radius(sigma), 3, edges);
        }
    }
}

/// `gaussian-blur`: a Gaussian whose standard deviation is
/// [`SIGMA_PER_BLURRINESS`] of `blurriness`, in the layer's px.
pub(crate) fn gaussian_blur(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let blurriness = number(values, 0) * SIGMA_PER_BLURRINESS;
    let [x, y] = ways(choice(values, 1));
    let [sx, sy] = scales(space);
    let sigma = [
        if x { blurriness * sx } else { 0.0 },
        if y { blurriness * sy } else { 0.0 },
    ];
    gaussian(pixels, sigma, Edges::from_choice(choice(values, 2)));
}

/// `box-blur`: `iterations` box passes of `radius`, in the layer's px.
pub(crate) fn box_blur(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let (radius, iterations) = (
        number(values, 0),
        number(values, 1).round().max(1.0) as usize,
    );
    let [x, y] = ways(choice(values, 2));
    let edges = Edges::from_choice(choice(values, 3));
    let [sx, sy] = scales(space);
    if x {
        along(pixels, 0, radius * sx, iterations, edges);
    }
    if y {
        along(pixels, 1, radius * sy, iterations, edges);
    }
}

/// `drop-shadow`: the layer's alpha in `color` at `opacity`, moved
/// `distance` px towards `direction` (degrees clockwise from up), blurred
/// by `softness` as a blurriness, and drawn behind the layer, or alone.
pub(crate) fn drop_shadow(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
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
    let mut shadow = pixels.transformed(Affine::translate(offset.to_vec2()));
    for p in shadow.data_mut() {
        let a = p[3] * opacity;
        *p = [color[0] * a, color[1] * a, color[2] * a, a];
    }
    let [sx, sy] = scales(space);
    gaussian(
        &mut shadow,
        [softness * sx, softness * sy],
        Edges::Transparent,
    );
    if alone {
        *pixels = shadow;
        return;
    }
    // The layer over its shadow.
    for (p, s) in pixels.data_mut().iter_mut().zip(shadow.data()) {
        let under = 1.0 - p[3];
        for c in 0..4 {
            p[c] += s[c] * under;
        }
    }
}

/// `glow`: the colours brighter than `threshold`, blurred by `radius` as a
/// blurriness, times `intensity`, added to the layer; with `composite`
/// `none`, the glow alone.
pub(crate) fn glow(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let (threshold, radius, intensity) = (number(values, 0), number(values, 1), number(values, 2));
    let [x, y] = ways(choice(values, 3));
    let alone = choice(values, 4) == 1;
    let mut light = pixels.clone();
    for p in light.data_mut() {
        let a = p[3];
        let luma = match a > 0.0 {
            true => (p[0] * LUMA[0] + p[1] * LUMA[1] + p[2] * LUMA[2]) / a,
            false => 0.0,
        };
        if luma < threshold {
            *p = [0.0; 4];
        }
    }
    let sigma = radius * SIGMA_PER_BLURRINESS;
    let [sx, sy] = scales(space);
    gaussian(
        &mut light,
        [
            if x { sigma * sx } else { 0.0 },
            if y { sigma * sy } else { 0.0 },
        ],
        Edges::Transparent,
    );
    if alone {
        for p in light.data_mut() {
            *p = p.map(|c| (c * intensity).min(1.0));
        }
        *pixels = light;
        return;
    }
    for (p, g) in pixels.data_mut().iter_mut().zip(light.data()) {
        for c in 0..4 {
            p[c] = (p[c] + g[c] * intensity).min(1.0);
        }
        let a = p[3];
        for c in &mut p[..3] {
            *c = c.min(a);
        }
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
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = (i as i64 % width, i as i64 / width);
        let at = |dx: i64, dy: i64| {
            let (nx, ny) = (x + dx, y + dy);
            let inside = nx >= 0 && ny >= 0 && nx < width && ny < i64::from(source.height());
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
}

#[cfg(test)]
#[path = "../tests/unit/blur.rs"]
mod tests;
