//! QuickTime Animation (`rle `): lossless frames run-length coded line by
//! line, each frame changing only the lines and pixels that differ from the
//! frame before it. At 24 bits a pixel is RGB; at 32 it is ARGB with
//! straight alpha.

use motion_format::{Diagnostic, Kind, Result};

use crate::{Decoder, Frame, Picture};

/// Decodes one clip's frames in order, keeping the last frame each one
/// changes.
pub(crate) struct Animation {
    width: usize,
    height: usize,
    /// Bytes per coded pixel: 3 or 4.
    bytes: usize,
    /// The frame so far, straight RGBA.
    pixels: Vec<u8>,
}

impl Animation {
    /// A decoder for `width` by `height` frames at `depth` bits a pixel.
    ///
    /// # Errors
    /// An unsupported-value diagnostic for a depth other than 24 or 32.
    pub(crate) fn new(width: u32, height: u32, depth: u16) -> Result<Animation> {
        let bytes = match depth {
            24 => 3,
            32 => 4,
            other => {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("QuickTime Animation at {other} bits a pixel"),
                )
                .expected("24-bit RGB or 32-bit ARGB"));
            }
        };
        let (width, height) = (width as usize, height as usize);
        Ok(Animation {
            width,
            height,
            bytes,
            pixels: opaque_black(width * height),
        })
    }

    /// The pixel at `x` in the coded bytes `from`, as straight RGBA.
    fn pixel(&self, from: &[u8]) -> [u8; 4] {
        match self.bytes {
            3 => [from[0], from[1], from[2], 255],
            _ => [from[1], from[2], from[3], from[0]],
        }
    }
}

impl Decoder for Animation {
    fn reset(&mut self) {
        self.pixels = opaque_black(self.width * self.height);
    }

    fn decode(&mut self, sample: &[u8]) -> Result<Box<dyn Picture>> {
        // Under 8 bytes, a frame changes nothing.
        if sample.len() >= 8 {
            self.apply(sample)?;
        }
        Ok(Box::new(Frame {
            width: self.width as u32,
            height: self.height as u32,
            pixels: self.pixels.clone(),
        }))
    }
}

impl Animation {
    fn apply(&mut self, sample: &[u8]) -> Result<()> {
        let mut r = Reader {
            bytes: sample,
            at: 4,
        };
        let header = r.u16()?;
        let (start, lines) = match header & 0x0008 {
            0 => (0, self.height),
            _ => {
                let start = usize::from(r.u16()?);
                r.skip(2)?;
                let lines = usize::from(r.u16()?);
                r.skip(2)?;
                (start, lines)
            }
        };
        if start + lines > self.height {
            return Err(corrupt("lines past the bottom of the frame"));
        }
        let size = self.bytes;
        for line in start..start + lines {
            let row = line * self.width;
            // Each line starts with how many pixels to skip, plus one.
            let mut x = usize::from(r.u8()?).saturating_sub(1);
            loop {
                let code = r.u8()? as i8;
                match code {
                    -1 => break,
                    0 => x += usize::from(r.u8()?).saturating_sub(1),
                    n if n < 0 => {
                        let color = self.pixel(r.take(size)?);
                        for _ in 0..usize::from(n.unsigned_abs()) {
                            self.put(row, &mut x, color)?;
                        }
                    }
                    n => {
                        for _ in 0..n {
                            let color = self.pixel(r.take(size)?);
                            self.put(row, &mut x, color)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Sets the pixel at `x` of the line starting at pixel `row`, and moves on.
    fn put(&mut self, row: usize, x: &mut usize, color: [u8; 4]) -> Result<()> {
        if *x >= self.width {
            return Err(corrupt("a run past the end of its line"));
        }
        let at = (row + *x) * 4;
        self.pixels[at..at + 4].copy_from_slice(&color);
        *x += 1;
        Ok(())
    }
}

/// `n` opaque black pixels, what a clip shows before its first frame.
fn opaque_black(n: usize) -> Vec<u8> {
    [0, 0, 0, 255].repeat(n)
}

/// Reads a sample's bytes in order.
struct Reader<'b> {
    bytes: &'b [u8],
    at: usize,
}

impl<'b> Reader<'b> {
    fn take(&mut self, n: usize) -> Result<&'b [u8]> {
        let out = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or_else(|| corrupt("a frame shorter than its runs"))?;
        self.at += n;
        Ok(out)
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Result<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }
}

fn corrupt(what: &str) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("QuickTime Animation: {what}"))
        .expected("a frame as the codec codes it")
}
