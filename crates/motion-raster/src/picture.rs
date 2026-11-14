//! Pictures: image files decoded once into premultiplied pixels that
//! [`ImageDraw`](crate::ImageDraw) leaves paint, shared by every frame that
//! draws them. The codecs and the rasteriser's pixel buffer stay private
//! ([D37]); a caller sees only [`Picture`].
//!
//! The formats are rows of one table, each recognised by how its files
//! start, never by a file name, so a renamed file still decodes and an
//! unknown one is told which formats there are.
//!
//! A picture drawn at under half its size is sampled from a copy halved
//! as often as it takes (a mipmap), so each pixel drawn averages the
//! pixels under it: bilinear sampling of the full picture would read only
//! the few nearest and skip the rest, and a large logo drawn small would
//! come out jagged and shimmering.
//!
//! [D37]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d37-engines-stay-behind-our-own-types

use std::fmt;
use std::sync::{Arc, OnceLock};

use motion_format::{Diagnostic, Kind, Result};
use vello_cpu::peniko::{ImageAlphaType, ImageQuality, ImageSampler};
use vello_cpu::{ImageSource, PixelMetadata, Pixmap};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use crate::image::Image;

/// A file format pictures are read from.
struct Format {
    /// Its name, as errors write it.
    name: &'static str,
    /// The bytes every file of it starts with.
    signature: &'static [u8],
    /// Reads a whole file of it as straight-alpha RGBA.
    decode: fn(&[u8]) -> Result<Image>,
}

/// The formats a picture is read from, recognised by their signatures.
const FORMATS: &[Format] = &[
    Format {
        name: "PNG",
        signature: b"\x89PNG\r\n\x1a\n",
        decode: Image::from_png,
    },
    Format {
        name: "JPEG",
        signature: &[0xFF, 0xD8, 0xFF],
        decode: jpeg,
    },
];

/// A decoded picture, ready to paint: premultiplied pixels behind a shared
/// handle, so cloning one, or putting it in many frames' display lists,
/// copies no pixels.
#[derive(Clone)]
pub struct Picture {
    levels: Arc<Levels>,
}

/// A picture's pixels at full size, and halved again and again for drawing
/// it small.
struct Levels {
    /// The pixels as decoded, premultiplied.
    full: Arc<Pixmap>,
    /// Each half the size of the one before, down to one pixel a side,
    /// made the first time the picture is drawn at under half its size.
    halves: OnceLock<Vec<Arc<Pixmap>>>,
}

impl Picture {
    /// Reads a PNG or JPEG file's bytes, recognised by how they start.
    /// PNG keeps its alpha; JPEG is opaque. Colours are taken as sRGB.
    ///
    /// # Errors
    /// An invalid-value diagnostic for bytes of no format the table knows,
    /// or that its decoder rejects, and an out-of-range one for a picture
    /// above 65535 pixels a side.
    pub fn decode(bytes: &[u8]) -> Result<Picture> {
        let format = FORMATS
            .iter()
            .find(|format| bytes.starts_with(format.signature))
            .ok_or_else(|| {
                Diagnostic::new(Kind::Invalid, "a file of no picture format").expected(expected())
            })?;
        Picture::from_image(&(format.decode)(bytes)?)
    }

    /// A picture of `image`'s pixels.
    ///
    /// # Errors
    /// An out-of-range diagnostic for an image above 65535 pixels a side,
    /// which the rasteriser cannot hold, or with no pixels at all.
    pub fn from_image(image: &Image) -> Result<Picture> {
        let side = |pixels: u32| u16::try_from(pixels).ok().filter(|&side| side > 0);
        let (Some(width), Some(height)) = (side(image.width()), side(image.height())) else {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("a {}x{} picture", image.width(), image.height()),
            )
            .expected("1 to 65535 pixels a side"));
        };
        let pixels = Pixmap::from_parts(
            image.pixels().to_vec(),
            width,
            height,
            PixelMetadata::new(ImageAlphaType::Alpha, true),
        );
        Ok(Picture {
            levels: Arc::new(Levels {
                full: Arc::new(pixels),
                halves: OnceLock::new(),
            }),
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        u32::from(self.levels.full.width())
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        u32::from(self.levels.full.height())
    }

    /// The paint that draws it where one of its pixels spans `step` pixels
    /// of the target along its longer side, with that paint's size. Under
    /// half size it samples the largest halved copy whose pixels still span
    /// at least one target pixel, so bilinear sampling skips none of them.
    /// Edge pixels repeat past the sides.
    pub(crate) fn paint(&self, step: f64) -> (vello_cpu::Image, u32, u32) {
        let level = match step {
            step if step > 0.0 && step < 0.5 => (1.0 / step).log2().floor() as usize,
            _ => 0,
        };
        let pixels = match level {
            0 => &self.levels.full,
            level => {
                let halves = self.levels.halves.get_or_init(|| halves(&self.levels.full));
                halves
                    .get(level - 1)
                    .or(halves.last())
                    .unwrap_or(&self.levels.full)
            }
        };
        let paint = vello_cpu::Image {
            image: ImageSource::Pixmap(Arc::clone(pixels)),
            sampler: ImageSampler::new().with_quality(ImageQuality::Medium),
        };
        (paint, u32::from(pixels.width()), u32::from(pixels.height()))
    }
}

/// `full` halved again and again, down to one pixel a side.
fn halves(full: &Pixmap) -> Vec<Arc<Pixmap>> {
    let mut halves: Vec<Arc<Pixmap>> = Vec::new();
    while let Some(next) = half(halves.last().map_or(full, |last| last)) {
        halves.push(Arc::new(next));
    }
    halves
}

/// `pixels` at half its size, each pixel the mean of the two by two it
/// covers (premultiplied, so transparent pixels add no colour); a side of
/// one pixel stays one, and an odd side's last pixel is shared. `None` for
/// a single pixel.
fn half(pixels: &Pixmap) -> Option<Pixmap> {
    let (width, height) = (pixels.width(), pixels.height());
    if width == 1 && height == 1 {
        return None;
    }
    let (half_width, half_height) = ((width / 2).max(1), (height / 2).max(1));
    let data = pixels.data();
    let at = |x: u16, y: u16| {
        data[usize::from(y.min(height - 1)) * usize::from(width) + usize::from(x.min(width - 1))]
    };
    let mut bytes = Vec::with_capacity(usize::from(half_width) * usize::from(half_height) * 4);
    for y in 0..half_height {
        for x in 0..half_width {
            let four = [
                at(2 * x, 2 * y),
                at(2 * x + 1, 2 * y),
                at(2 * x, 2 * y + 1),
                at(2 * x + 1, 2 * y + 1),
            ];
            let mean = |channel: fn(&vello_cpu::color::PremulRgba8) -> u8| {
                let sum: u16 = four.iter().map(|p| u16::from(channel(p))).sum();
                ((sum + 2) / 4) as u8
            };
            bytes.extend([mean(|p| p.r), mean(|p| p.g), mean(|p| p.b), mean(|p| p.a)]);
        }
    }
    Some(Pixmap::from_parts(
        bytes,
        half_width,
        half_height,
        PixelMetadata::new(
            ImageAlphaType::AlphaPremultiplied,
            pixels.may_have_transparency(),
        ),
    ))
}

/// Two pictures are equal when they hold the same pixels.
impl PartialEq for Picture {
    fn eq(&self, other: &Picture) -> bool {
        let (a, b) = (&self.levels.full, &other.levels.full);
        Arc::ptr_eq(a, b)
            || (a.width() == b.width() && a.height() == b.height() && a.data() == b.data())
    }
}

/// Its size, not its pixels.
impl fmt::Debug for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Picture({}x{})", self.width(), self.height())
    }
}

/// The formats a picture is read from, as an accepted form: `a PNG or JPEG file`.
fn expected() -> String {
    let names: Vec<&str> = FORMATS.iter().map(|format| format.name).collect();
    match names.split_last() {
        Some((last, [])) => format!("a {last} file"),
        Some((last, rest)) => format!("a {} or {last} file", rest.join(", ")),
        None => "a picture file".to_owned(),
    }
}

/// Reads a JPEG file as opaque RGBA, whatever its own colour model (grey,
/// YCbCr, CMYK or YCCK).
fn jpeg(bytes: &[u8]) -> Result<Image> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let error = |message: String| Diagnostic::new(Kind::Invalid, message).expected("a JPEG");
    let pixels = decoder.decode().map_err(|e| error(format!("JPEG: {e}")))?;
    let info = decoder
        .info()
        .ok_or_else(|| error("JPEG: no frame header".to_owned()))?;
    if decoder.output_colorspace() != Some(ColorSpace::RGBA) {
        return Err(error(format!(
            "JPEG: decoded as {:?}, not RGBA",
            decoder.output_colorspace()
        )));
    }
    Image::new(u32::from(info.width), u32::from(info.height), pixels)
}

#[cfg(test)]
#[path = "../tests/unit/picture.rs"]
mod tests;
