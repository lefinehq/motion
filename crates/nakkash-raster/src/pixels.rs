//! A layer's pixels while its effects run: what the layer drew, as
//! premultiplied RGBA from 0 to 1 per channel, for effects that need to read
//! what was drawn rather than paint over it.

use vello_cpu::kurbo::{Affine, Point};
use vello_cpu::peniko::ImageAlphaType;
use vello_cpu::{PixelMetadata, Pixmap};

use crate::blend::BlendMode;

/// One pixel: premultiplied red, green, blue and alpha, each 0 to 1.
pub type Pixel = [f32; 4];

/// A frame-sized grid of pixels, row by row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Pixels {
    width: u16,
    height: u16,
    data: Vec<Pixel>,
}

impl Pixels {
    /// `width` by `height` transparent pixels.
    pub fn transparent(width: u16, height: u16) -> Pixels {
        Pixels {
            width,
            height,
            data: vec![[0.0; 4]; usize::from(width) * usize::from(height)],
        }
    }

    /// The pixels of a drawn pixmap.
    pub(crate) fn from_pixmap(pixmap: &Pixmap) -> Pixels {
        let data = pixmap
            .data()
            .iter()
            .map(|p| [p.r, p.g, p.b, p.a].map(|c| f32::from(c) / 255.0))
            .collect();
        Pixels {
            width: pixmap.width(),
            height: pixmap.height(),
            data,
        }
    }

    /// The pixels as a pixmap, each channel rounded to 8 bits and kept
    /// within its alpha, as premultiplied pixels must be.
    pub(crate) fn to_pixmap(&self) -> Pixmap {
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let data = self
            .data
            .iter()
            .flat_map(|&[r, g, b, a]| {
                let a = byte(a);
                let channel = |c: f32| byte(c).min(a);
                [channel(r), channel(g), channel(b), a]
            })
            .collect();
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
        self.data
            .iter()
            .flat_map(|&[r, g, b, a]| {
                let a = a.clamp(0.0, 1.0);
                match byte(a) {
                    0 => [0; 4],
                    alpha => [byte(r / a), byte(g / a), byte(b / a), alpha],
                }
            })
            .collect()
    }

    /// Lays `source`, the same size, on these pixels by `mode`, each of
    /// its pixels first scaled by the share `amount` gives for it, from 0
    /// to 1: how the float canvas composites a layer. Like
    /// [`Pixels::to_pixmap`], each source pixel is kept within 0 to 1 and
    /// its colour within its alpha, but nothing is rounded.
    pub(crate) fn lay(&mut self, source: &Pixels, mode: BlendMode, amount: impl Fn(usize) -> f32) {
        debug_assert_eq!(
            (self.width, self.height),
            (source.width, source.height),
            "laid pixels are the canvas's size"
        );
        for (i, (below, &[r, g, b, a])) in self.data.iter_mut().zip(&source.data).enumerate() {
            let a = a.clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let k = amount(i).clamp(0.0, 1.0);
            if k <= 0.0 {
                continue;
            }
            let within = |c: f32| c.clamp(0.0, a) * k;
            *below = mode.lay(*below, [within(r), within(g), within(b), a * k]);
        }
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

    /// The pixels moved by `transform`, in pixels of the grid: the pixel at
    /// a point is the one `transform` brings there, sampled bilinearly.
    pub fn transformed(&self, transform: Affine) -> Pixels {
        let back = transform.inverse();
        let mut out = Pixels::transparent(self.width, self.height);
        let width = usize::from(self.width);
        for (i, pixel) in out.data.iter_mut().enumerate() {
            let (x, y) = ((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
            let from = back * Point::new(x, y);
            *pixel = self.sample(from.x, from.y);
        }
        out
    }

    /// Each pixel moved from this one towards `other`'s by the share
    /// `amount` gives for it, from 0 (this one) to 1 (`other`'s).
    pub fn mix(&mut self, other: &Pixels, amount: impl Fn(usize) -> f32) {
        for (i, (pixel, to)) in self.data.iter_mut().zip(&other.data).enumerate() {
            let t = amount(i).clamp(0.0, 1.0);
            for c in 0..4 {
                pixel[c] += (to[c] - pixel[c]) * t;
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/pixels.rs"]
mod tests;
