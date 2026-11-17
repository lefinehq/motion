//! Photo JPEG (`jpeg`): every frame is a JPEG of its own, so any frame
//! decodes without the ones before it, and only when it is asked for.

use nakkash_format::{Diagnostic, Kind, Result};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use crate::{Decoder, Frame, Picture};

/// The Photo JPEG decoder; it keeps nothing between frames.
pub(crate) struct Photo;

impl Decoder for Photo {
    fn reset(&mut self) {}

    fn decode(&mut self, sample: &[u8]) -> Result<Box<dyn Picture>> {
        Ok(Box::new(Jpeg(sample.to_vec())))
    }
}

/// One frame's JPEG, decoded when its frame is asked for.
struct Jpeg(Vec<u8>);

impl Picture for Jpeg {
    fn frame(self: Box<Self>) -> Result<Frame> {
        let sample = &self.0;
        let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(sample), options);
        let error = |message: String| {
            Diagnostic::new(Kind::Invalid, format!("Photo JPEG: {message}"))
                .expected("a JPEG in every frame")
        };
        let pixels = decoder.decode().map_err(|e| error(e.to_string()))?;
        let info = decoder
            .info()
            .ok_or_else(|| error("no frame header".to_owned()))?;
        if decoder.output_colorspace() != Some(ColorSpace::RGBA) {
            return Err(error(format!(
                "decoded as {:?}, not RGBA",
                decoder.output_colorspace()
            )));
        }
        Ok(Frame {
            width: u32::from(info.width),
            height: u32::from(info.height),
            pixels,
        })
    }
}
