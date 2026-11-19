//! Pictures as the pipelines see them, and the few operations they need:
//! resampling, rank filters, a Gaussian blur.
//!
//! Resampling follows the convolution most image libraries use: a filter
//! centred on each output pixel's centre, widened by the scale when the
//! picture shrinks so that every input pixel counts, and its weights
//! normalised over the pixels inside the picture. With the triangle filter
//! and a picture that grows, that is bilinear interpolation between pixel
//! centres. The models were trained and measured on frames resampled this
//! way, so the bake resamples the same way.

/// A colour picture, three bytes a pixel (red, green, blue), row by row from
/// the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb8 {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// The bytes, `3 * width * height` of them.
    pub data: Vec<u8>,
}

/// One channel of floats, row by row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// The values, `width * height` of them.
    pub data: Vec<f32>,
}

/// The filter a resampling convolves with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// The triangle of half-width 1: bilinear.
    Triangle,
    /// The cubic convolution of half-width 2 with a = −0.5: bicubic.
    Cubic,
}

impl Filter {
    /// How far from its centre it reaches, at a scale of 1.
    fn support(self) -> f64 {
        match self {
            Filter::Triangle => 1.0,
            Filter::Cubic => 2.0,
        }
    }

    /// Its weight at `x` from its centre, at a scale of 1.
    fn weight(self, x: f64) -> f64 {
        let x = x.abs();
        match self {
            Filter::Triangle => (1.0 - x).max(0.0),
            Filter::Cubic => {
                const A: f64 = -0.5;
                if x < 1.0 {
                    ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
                } else if x < 2.0 {
                    (((x - 5.0) * x + 8.0) * x - 4.0) * A
                } else {
                    0.0
                }
            }
        }
    }
}

/// The weights of one output pixel along one axis: the first input pixel
/// it reads and a weight for each from there.
struct Taps {
    first: usize,
    weights: Vec<f32>,
}

/// The taps of every output pixel of an axis of `from` pixels resampled to
/// `to`.
fn taps(from: usize, to: usize, filter: Filter) -> Vec<Taps> {
    let scale = from as f64 / to as f64;
    let widen = scale.max(1.0);
    let support = filter.support() * widen;
    (0..to)
        .map(|i| {
            let centre = (i as f64 + 0.5) * scale;
            let first = (centre - support).floor().max(0.0) as usize;
            let last = ((centre + support).ceil() as usize).min(from);
            let mut weights: Vec<f64> = (first..last)
                .map(|j| filter.weight((j as f64 + 0.5 - centre) / widen))
                .collect();
            let sum: f64 = weights.iter().sum();
            if sum != 0.0 {
                weights.iter_mut().for_each(|w| *w /= sum);
            }
            Taps {
                first,
                weights: weights.into_iter().map(|w| w as f32).collect(),
            }
        })
        .collect()
}

/// `src`, `width` by `height` pixels of `channels` interleaved floats,
/// resampled to `to_width` by `to_height` with `filter`, rows first.
///
/// # Panics
/// When `src` does not hold `width * height * channels` values, or a size
/// is zero.
pub fn resample(
    src: &[f32],
    (width, height): (usize, usize),
    channels: usize,
    (to_width, to_height): (usize, usize),
    filter: Filter,
) -> Vec<f32> {
    assert_eq!(
        src.len(),
        width * height * channels,
        "one value a channel of a pixel"
    );
    assert!(
        width > 0 && height > 0 && to_width > 0 && to_height > 0,
        "an empty picture"
    );
    let across = taps(width, to_width, filter);
    let mut rows = vec![0f32; to_width * height * channels];
    for y in 0..height {
        for (x, tap) in across.iter().enumerate() {
            for c in 0..channels {
                let mut sum = 0f32;
                for (k, w) in tap.weights.iter().enumerate() {
                    sum += w * src[(y * width + tap.first + k) * channels + c];
                }
                rows[(y * to_width + x) * channels + c] = sum;
            }
        }
    }
    let down = taps(height, to_height, filter);
    let mut out = vec![0f32; to_width * to_height * channels];
    for (y, tap) in down.iter().enumerate() {
        for x in 0..to_width {
            for c in 0..channels {
                let mut sum = 0f32;
                for (k, w) in tap.weights.iter().enumerate() {
                    sum += w * rows[((tap.first + k) * to_width + x) * channels + c];
                }
                out[(y * to_width + x) * channels + c] = sum;
            }
        }
    }
    out
}

impl Rgb8 {
    /// It resampled to `width` by `height` with `filter`, each channel
    /// rounded back to a byte, as an 8-bit picture library does.
    pub fn resized(&self, width: usize, height: usize, filter: Filter) -> Rgb8 {
        let floats: Vec<f32> = self.data.iter().map(|&b| f32::from(b)).collect();
        let out = resample(
            &floats,
            (self.width, self.height),
            3,
            (width, height),
            filter,
        );
        Rgb8 {
            width,
            height,
            data: out
                .iter()
                .map(|v| v.round().clamp(0.0, 255.0) as u8)
                .collect(),
        }
    }

    /// The part `[x, y, width, height]` of it.
    ///
    /// # Panics
    /// When the part does not lie inside it.
    pub fn crop(&self, [x, y, width, height]: [usize; 4]) -> Rgb8 {
        assert!(
            x + width <= self.width && y + height <= self.height,
            "a crop inside the picture"
        );
        let mut data = Vec::with_capacity(width * height * 3);
        for row in y..y + height {
            let start = (row * self.width + x) * 3;
            data.extend_from_slice(&self.data[start..start + width * 3]);
        }
        Rgb8 {
            width,
            height,
            data,
        }
    }

    /// Its luma, Rec. 601 weights, from 0 to 255.
    pub fn grey(&self) -> Plane {
        Plane {
            width: self.width,
            height: self.height,
            data: self
                .data
                .chunks_exact(3)
                .map(|p| {
                    0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])
                })
                .collect(),
        }
    }
}

impl Plane {
    /// A plane of `width` by `height`, every value `value`.
    pub fn filled(width: usize, height: usize, value: f32) -> Plane {
        Plane {
            width,
            height,
            data: vec![value; width * height],
        }
    }

    /// It resampled to `width` by `height` with `filter`.
    pub fn resized(&self, width: usize, height: usize, filter: Filter) -> Plane {
        Plane {
            width,
            height,
            data: resample(
                &self.data,
                (self.width, self.height),
                1,
                (width, height),
                filter,
            ),
        }
    }

    /// Each value replaced by the largest in the `size` by `size` square
    /// around it (`size` odd), the square cut at the edges.
    pub fn max_filter(&self, size: usize) -> Plane {
        self.rank(size, f32::max)
    }

    /// Each value replaced by the smallest in the `size` by `size` square
    /// around it (`size` odd), the square cut at the edges.
    pub fn min_filter(&self, size: usize) -> Plane {
        self.rank(size, f32::min)
    }

    /// The square rank filter by `pick`, one axis at a time.
    fn rank(&self, size: usize, pick: fn(f32, f32) -> f32) -> Plane {
        let r = size / 2;
        let (w, h) = (self.width, self.height);
        let mut rows = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
                rows[y * w + x] = (a..=b)
                    .map(|i| self.data[y * w + i])
                    .reduce(pick)
                    .unwrap_or(0.0);
            }
        }
        let mut out = vec![0f32; w * h];
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
            for x in 0..w {
                out[y * w + x] = (a..=b).map(|j| rows[j * w + x]).reduce(pick).unwrap_or(0.0);
            }
        }
        Plane {
            width: w,
            height: h,
            data: out,
        }
    }

    /// It blurred by a Gaussian of standard deviation `sigma` px, reaching
    /// three deviations, the edge pixels repeated beyond the edges.
    pub fn gaussian_blur(&self, sigma: f32) -> Plane {
        let reach = (3.0 * sigma).ceil() as isize;
        let mut kernel: Vec<f32> = (-reach..=reach)
            .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let sum: f32 = kernel.iter().sum();
        kernel.iter_mut().for_each(|k| *k /= sum);
        let (w, h) = (self.width as isize, self.height as isize);
        let at = |v: isize, n: isize| v.clamp(0, n - 1) as usize;
        let mut rows = vec![0f32; self.data.len()];
        for y in 0..h {
            for x in 0..w {
                rows[(y * w + x) as usize] = kernel
                    .iter()
                    .enumerate()
                    .map(|(k, wk)| wk * self.data[(y * w) as usize + at(x + k as isize - reach, w)])
                    .sum();
            }
        }
        let mut out = vec![0f32; self.data.len()];
        for y in 0..h {
            for x in 0..w {
                out[(y * w + x) as usize] = kernel
                    .iter()
                    .enumerate()
                    .map(|(k, wk)| {
                        wk * rows[at(y + k as isize - reach, h) * w as usize + x as usize]
                    })
                    .sum();
            }
        }
        Plane {
            width: self.width,
            height: self.height,
            data: out,
        }
    }
}

/// The logistic function, `1 / (1 + e^-x)`, with `x` clamped to ±30 so it
/// never overflows.
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x.clamp(-30.0, 30.0)).exp())
}

#[cfg(test)]
#[path = "../tests/unit/image.rs"]
mod tests;
