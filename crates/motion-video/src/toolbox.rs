//! H.264 through macOS's VideoToolbox, by way of `motion-videotoolbox`:
//! samples go to the system's decoder in decode order, and each picture it
//! hands back becomes RGBA, with the clip's colour, when it is asked for.

use motion_format::{Diagnostic, Kind, Result};
use motion_videotoolbox::{Biplanar, Error, Record, Samples};

use crate::avc;
use crate::colour::Colour;
use crate::ycbcr::{self, Planes};
use crate::{Decoder, Frame, Picture};

/// One clip's decompression session.
pub(crate) struct Avc {
    decoder: motion_videotoolbox::Decoder,
    /// Bytes of the length before each NAL unit.
    length_size: usize,
    colour: Colour,
}

impl Avc {
    /// A session for the stream `record` configures, whose samples have
    /// NAL unit lengths of `length_size` bytes, `width` by `height`.
    ///
    /// # Errors
    /// The system's failure, when it has no decoder for the stream.
    pub(crate) fn new(
        record: &[u8],
        length_size: usize,
        width: u32,
        height: u32,
        colour: Colour,
    ) -> Result<Avc> {
        let decoder = motion_videotoolbox::Decoder::new(Record::Avc(record), width, height)
            .map_err(failed)?;
        Ok(Avc {
            decoder,
            length_size,
            colour,
        })
    }
}

impl Decoder for Avc {
    /// The next sample after a reset is a sync one, which the decoder
    /// starts again from by itself.
    fn reset(&mut self) {}

    fn disposable(&self, sample: &[u8]) -> bool {
        avc::disposable(sample, self.length_size)
    }

    fn decode(&mut self, sample: &[u8]) -> Result<Box<dyn Picture>> {
        let picture = self
            .decoder
            .decode(sample)
            .map_err(failed)?
            .ok_or_else(|| {
                Diagnostic::new(Kind::Invalid, "H.264: the system's decoder dropped a frame")
                    .expected("a frame it decodes")
            })?;
        Ok(Box::new(Decoded {
            picture,
            colour: self.colour,
        }))
    }
}

/// A picture from the system's decoder, and the colour it is in.
struct Decoded {
    picture: motion_videotoolbox::Picture,
    colour: Colour,
}

impl Picture for Decoded {
    fn frame(self: Box<Self>) -> Result<Frame> {
        let colour = self.colour;
        self.picture
            .read(|samples| match samples {
                Samples::Eight(planes) => ycbcr::to_frame(planes_of(planes, 8), colour),
                Samples::Ten(planes) => ycbcr::to_frame(planes_of(planes, 10), colour),
            })
            .map_err(failed)
    }
}

fn planes_of<T>(planes: Biplanar<'_, T>, bits: u32) -> Planes<'_, T> {
    Planes {
        width: planes.width,
        height: planes.height,
        luma: planes.luma,
        luma_stride: planes.luma_stride,
        chroma: planes.chroma,
        chroma_stride: planes.chroma_stride,
        bits,
    }
}

fn failed(e: Error) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("H.264: {e}")).expected("a stream the system decodes")
}
