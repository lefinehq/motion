//! Pictures: image files decoded once into premultiplied pixels that
//! [`ImageDraw`](crate::ImageDraw) leaves paint, shared by every frame that
//! draws them. The codecs and the rasteriser's pixel buffer stay private
//! ([D37]); a caller sees only [`Picture`].
//!
//! The formats are rows of one table, each recognised by how its files
//! start, never by a file name, so a renamed file still decodes and an
//! unknown one is told which formats there are.
//!
//! [D37]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d37-engines-stay-behind-our-own-types

use std::fmt;
use std::sync::Arc;

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
    pixels: Arc<Pixmap>,
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
            pixels: Arc::new(pixels),
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        u32::from(self.pixels.width())
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        u32::from(self.pixels.height())
    }

    /// The paint that draws it, sampled bilinearly, with its edge pixels
    /// repeated past its sides.
    pub(crate) fn to_vello(&self) -> vello_cpu::Image {
        vello_cpu::Image {
            image: ImageSource::Pixmap(Arc::clone(&self.pixels)),
            sampler: ImageSampler::new().with_quality(ImageQuality::Medium),
        }
    }
}

/// Two pictures are equal when they hold the same pixels.
impl PartialEq for Picture {
    fn eq(&self, other: &Picture) -> bool {
        Arc::ptr_eq(&self.pixels, &other.pixels)
            || (self.pixels.width() == other.pixels.width()
                && self.pixels.height() == other.pixels.height()
                && self.pixels.data() == other.pixels.data())
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
