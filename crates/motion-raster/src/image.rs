//! A rendered frame's pixels, and their PNG form.

use std::io::Cursor;

use motion_format::{Diagnostic, Kind, Result};

/// An RGBA8 image: rows top to bottom, four bytes per pixel, alpha not
/// premultiplied (what PNG files and most tools expect).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Image {
    /// An image of `width` by `height` pixels from straight-alpha RGBA8 bytes.
    ///
    /// # Errors
    /// An invalid-value diagnostic if `pixels` does not hold exactly
    /// `width * height * 4` bytes.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Image> {
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("{} bytes for a {width}x{height} image", pixels.len()),
            )
            .expected(format!("{expected} bytes: four per pixel")));
        }
        Ok(Image {
            width,
            height,
            pixels,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The RGBA8 bytes, rows top to bottom.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The pixel at `(x, y)` as `[r, g, b, a]`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[at..at + 4].try_into().ok()
    }

    /// The image as a PNG file: 8-bit RGBA, sRGB, no metadata that varies
    /// between runs, so the same pixels always give the same bytes.
    ///
    /// # Errors
    /// An invalid-value diagnostic if the encoder fails.
    pub fn to_png(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = encoder.write_header().map_err(png_error)?;
        writer.write_image_data(&self.pixels).map_err(png_error)?;
        writer.finish().map_err(png_error)?;
        Ok(bytes)
    }

    /// Reads an 8-bit RGBA PNG file, such as a golden image.
    ///
    /// # Errors
    /// An invalid-value diagnostic for bytes that are not such a PNG.
    pub fn from_png(bytes: &[u8]) -> Result<Image> {
        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info().map_err(png_error)?;
        let info = reader.info();
        if (info.color_type, info.bit_depth) != (png::ColorType::Rgba, png::BitDepth::Eight) {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("a {:?} PNG of depth {:?}", info.color_type, info.bit_depth),
            )
            .expected("an 8-bit RGBA PNG"));
        }
        let size = reader.output_buffer_size().ok_or_else(|| {
            Diagnostic::new(Kind::Invalid, "a PNG too large to decode")
                .expected("an 8-bit RGBA PNG")
        })?;
        let mut pixels = vec![0; size];
        let frame = reader.next_frame(&mut pixels).map_err(png_error)?;
        pixels.truncate(frame.buffer_size());
        Image::new(frame.width, frame.height, pixels)
    }
}

/// A PNG encoder or decoder failure as a diagnostic.
fn png_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("PNG: {error}")).expected("an 8-bit RGBA PNG")
}

#[cfg(test)]
#[path = "../tests/unit/image.rs"]
mod tests;
