//! A layer's pixels while its effects run: what the layer drew, as
//! premultiplied RGBA from 0 to 1 per channel, for effects that need to read
//! what was drawn rather than paint over it.

use vello_cpu::kurbo::{Affine, Point};
use vello_cpu::peniko::ImageAlphaType;
use vello_cpu::{PixelMetadata, Pixmap};

use crate::blend::BlendMode;
use crate::memory::{self, Start};
use crate::par;

/// One pixel: premultiplied red, green, blue and alpha, each 0 to 1.
pub type Pixel = [f32; 4];

/// A frame-sized grid of pixels, row by row from the top.
#[derive(Debug, PartialEq)]
pub struct Pixels {
    width: u16,
    height: u16,
    data: Vec<Pixel>,
}

impl Clone for Pixels {
    /// A copy, made on every thread: effects copy whole frames to read
    /// while they write, and a copy made on one thread would be most of
    /// the time of a fast effect on many. Its grid comes from the pool
    /// when the pool has one of its size ([`memory`]).
    fn clone(&self) -> Pixels {
        let mut data = memory::grid(self.data.len(), Start::Written);
        par::each_with(&mut data, &self.data, |_, to, from| *to = *from);
        Pixels {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

impl Drop for Pixels {
    /// Gives the grid back to the budget's pool, or to the system
    /// ([`memory`]).
    fn drop(&mut self) {
        memory::release(std::mem::take(&mut self.data));
    }
}

impl Pixels {
    /// `width` by `height` transparent pixels.
    pub fn transparent(width: u16, height: u16) -> Pixels {
        Pixels {
            width,
            height,
            data: memory::grid(usize::from(width) * usize::from(height), Start::Clear),
        }
    }

    /// `width` by `height` pixels, each the one `make` gives for its index,
    /// row by row from the top, made on every thread: a grid made from
    /// another is made into a grid of its own, not into a copy of it.
    pub(crate) fn made(width: u16, height: u16, make: impl Fn(usize) -> Pixel + Sync) -> Pixels {
        let mut data = memory::grid(usize::from(width) * usize::from(height), Start::Written);
        par::each(&mut data, |i, pixel| *pixel = make(i));
        Pixels {
            width,
            height,
            data,
        }
    }

    /// `width` by `height` pixels holding `data`, row by row from the top.
    ///
    /// # Panics
    /// When `data` does not hold `width` × `height` pixels.
    pub fn from_data(width: u16, height: u16, data: Vec<Pixel>) -> Pixels {
        assert_eq!(
            data.len(),
            usize::from(width) * usize::from(height),
            "pixels for every place of the grid"
        );
        memory::made(data.len());
        Pixels {
            width,
            height,
            data,
        }
    }

    /// The pixels of a drawn pixmap.
    pub(crate) fn from_pixmap(pixmap: &Pixmap) -> Pixels {
        let bytes = pixmap.data();
        Pixels::made(pixmap.width(), pixmap.height(), |i| {
            let p = bytes[i];
            [p.r, p.g, p.b, p.a].map(|c| f32::from(c) / 255.0)
        })
    }

    /// The pixels as a pixmap, each channel rounded to 8 bits and kept
    /// within its alpha, as premultiplied pixels must be.
    pub(crate) fn to_pixmap(&self) -> Pixmap {
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let data = self.bytes(|[r, g, b, a]| {
            let a = byte(a);
            let channel = |c: f32| byte(c).min(a);
            [channel(r), channel(g), channel(b), a]
        });
        Pixmap::from_parts(
            data,
            self.width,
            self.height,
            PixelMetadata::new(ImageAlphaType::AlphaPremultiplied, true),
        )
    }

    /// The pixels as straight (not premultiplied) RGBA at 8 bits a
    /// channel, row by row: an image's bytes. Each channel is kept within
    /// 0 to 1, and a colour within its alpha, before it is rounded.
    pub(crate) fn to_rgba8(&self) -> Vec<u8> {
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        self.bytes(|[r, g, b, a]| {
            let a = a.clamp(0.0, 1.0);
            match byte(a) {
                0 => [0; 4],
                alpha => [byte(r / a), byte(g / a), byte(b / a), alpha],
            }
        })
    }

    /// The four bytes `byte` gives for each pixel, row by row.
    fn bytes(&self, byte: impl Fn(Pixel) -> [u8; 4] + Sync) -> Vec<u8> {
        let mut bytes = vec![0; self.data.len() * 4];
        let task = par::task();
        par::rows(
            &mut bytes,
            4 * task,
            || (),
            |run, out, ()| {
                let first = run * task;
                for (i, px) in out.chunks_exact_mut(4).enumerate() {
                    px.copy_from_slice(&byte(self.data[first + i]));
                }
            },
        );
        bytes
    }

    /// Lays `source`, the same size, on these pixels by `mode`, each of
    /// its pixels first scaled by the share `amount` gives for it, from 0
    /// to 1: how the float canvas composites a layer. As when the pixels
    /// become an 8-bit picture, each source pixel is kept within 0 to 1 and
    /// its colour within its alpha, but nothing is rounded.
    pub fn lay(&mut self, source: &Pixels, mode: BlendMode, amount: impl Fn(usize) -> f32 + Sync) {
        debug_assert_eq!(
            (self.width, self.height),
            (source.width, source.height),
            "laid pixels are the canvas's size"
        );
        par::each_with(&mut self.data, &source.data, |i, below, &[r, g, b, a]| {
            // A level that is not a number covers nothing: such a pixel is
            // left out, never laid (and never a panic in the clamps below).
            let a = a.clamp(0.0, 1.0);
            if a.is_nan() || a <= 0.0 {
                return;
            }
            let k = amount(i).clamp(0.0, 1.0);
            if k.is_nan() || k <= 0.0 {
                return;
            }
            let within = |c: f32| if c.is_nan() { 0.0 } else { c.clamp(0.0, a) * k };
            *below = mode.lay(*below, [within(r), within(g), within(b), a * k]);
        });
    }

    /// Its width in pixels.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Its height in pixels.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Every pixel, row by row from the top.
    pub fn data(&self) -> &[Pixel] {
        &self.data
    }

    /// Every pixel, to change in place.
    pub fn data_mut(&mut self) -> &mut [Pixel] {
        &mut self.data
    }

    /// The pixel at `x`, `y`, or transparent outside the grid.
    pub fn get(&self, x: i64, y: i64) -> Pixel {
        match (u16::try_from(x), u16::try_from(y)) {
            (Ok(x), Ok(y)) if x < self.width && y < self.height => {
                self.data[usize::from(y) * usize::from(self.width) + usize::from(x)]
            }
            _ => [0.0; 4],
        }
    }

    /// The colour at the point `x`, `y`, where pixel centres sit at half
    /// steps, blended from the four nearest pixels; transparent beyond the
    /// edges.
    pub fn sample(&self, x: f64, y: f64) -> Pixel {
        let (x, y) = (x - 0.5, y - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let mix = |a: Pixel, b: Pixel, t: f32| -> Pixel {
            std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
        };
        let top = mix(self.get(x0, y0), self.get(x0 + 1, y0), fx);
        let bottom = mix(self.get(x0, y0 + 1), self.get(x0 + 1, y0 + 1), fx);
        mix(top, bottom, fy)
    }

    /// [`Pixels::sample`] with the edge pixels repeated beyond the grid:
    /// the point held to the grid's outermost pixel centres first. A grid
    /// of no pixels is transparent everywhere.
    pub fn sample_repeating(&self, x: f64, y: f64) -> Pixel {
        let (width, height) = (f64::from(self.width), f64::from(self.height));
        if width == 0.0 || height == 0.0 {
            return [0.0; 4];
        }
        self.sample(x.clamp(0.5, width - 0.5), y.clamp(0.5, height - 0.5))
    }

    /// The pixels moved by `transform`, in pixels of the grid: the pixel at
    /// a point is the one `transform` brings there, sampled bilinearly.
    pub fn transformed(&self, transform: Affine) -> Pixels {
        let back = transform.inverse();
        let mut out = Pixels::transparent(self.width, self.height);
        // A move that flattens the pixels to a line or a point has no
        // finite way back: nothing of them is left to see.
        if !back.as_coeffs().iter().all(|c| c.is_finite()) {
            return out;
        }
        let width = usize::from(self.width);
        par::each(&mut out.data, |i, pixel| {
            let (x, y) = ((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
            let from = back * Point::new(x, y);
            *pixel = self.sample(from.x, from.y);
        });
        out
    }

    /// Each pixel moved from this one towards `other`'s by the share
    /// `amount` gives for it, from 0 (this one) to 1 (`other`'s).
    pub fn mix(&mut self, other: &Pixels, amount: impl Fn(usize) -> f32 + Sync) {
        par::each_with(&mut self.data, &other.data, |i, pixel, to| {
            let t = amount(i).clamp(0.0, 1.0);
            for c in 0..4 {
                pixel[c] += (to[c] - pixel[c]) * t;
            }
        });
    }
}

#[cfg(test)]
#[path = "../tests/unit/pixels.rs"]
mod tests;
