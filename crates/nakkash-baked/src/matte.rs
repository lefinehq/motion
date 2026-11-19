//! [`Matte`]: one baked alpha per frame of a clip.

use std::io::Cursor;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use nakkash_format::{Diagnostic, Kind, Result, Sha256};

use crate::{Asset, Made, Manifest, Store};

/// A matte: for each frame of a clip, in the order the frames show, the
/// hash of its alpha, a 16-bit greyscale PNG at the clip's size ([D85]).
/// Frames with the same alpha share one file.
///
/// [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matte {
    /// Each frame's alpha, by hash.
    pub frames: Vec<Sha256>,
}

impl Asset for Matte {
    const KIND: &'static str = "matte";
}

/// One frame's alpha: 0 is clear, 65535 opaque, row by row from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alpha {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// One value a pixel.
    pub values: Vec<u16>,
}

impl Alpha {
    /// An alpha of `width` by `height` from values between 0 and 1, rounded
    /// to the nearest of 65,536 levels; values outside are clamped.
    ///
    /// # Panics
    /// When `values` does not hold one value a pixel.
    pub fn from_unit(width: u32, height: u32, values: &[f32]) -> Self {
        assert_eq!(
            values.len(),
            width as usize * height as usize,
            "one value a pixel"
        );
        let values = values
            .iter()
            .map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
            .collect();
        Self {
            width,
            height,
            values,
        }
    }

    /// The value of the pixel at `index` as 8 bits, rounded.
    pub fn level8(&self, index: usize) -> u8 {
        // 65535 / 255 = 257 exactly, so this rounds to the nearest level.
        ((u32::from(self.values[index]) + 128) / 257) as u8
    }

    /// Its PNG: 16-bit greyscale, one way only, so the same alpha always
    /// gives the same bytes.
    fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let failed = |e: png::EncodingError| {
            Diagnostic::new(Kind::Invalid, format!("an alpha that does not encode: {e}"))
        };
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Sixteen);
            encoder.set_compression(png::Compression::Balanced);
            encoder.set_filter(png::Filter::Paeth);
            let mut writer = encoder.write_header().map_err(failed)?;
            let data: Vec<u8> = self.values.iter().flat_map(|v| v.to_be_bytes()).collect();
            writer.write_image_data(&data).map_err(failed)?;
        }
        Ok(bytes)
    }

    /// The alpha a PNG of [`Alpha::encode`] holds.
    fn decode(bytes: &[u8]) -> Result<Self> {
        let bad = |why: String| {
            Diagnostic::new(
                Kind::Invalid,
                format!("a baked alpha that does not decode: {why}"),
            )
        };
        let mut reader = png::Decoder::new(Cursor::new(bytes))
            .read_info()
            .map_err(|e| bad(e.to_string()))?;
        let info = reader.info();
        let (width, height) = (info.width, info.height);
        if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Sixteen
        {
            return Err(bad(format!(
                "{:?} at {:?} bits, not 16-bit greyscale",
                info.color_type, info.bit_depth
            )));
        }
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| bad("too large".into()))?;
        let mut data = vec![0; size];
        reader
            .next_frame(&mut data)
            .map_err(|e| bad(e.to_string()))?;
        let values = data
            .chunks_exact(2)
            .take(width as usize * height as usize)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        Ok(Self {
            width,
            height,
            values,
        })
    }
}

impl Manifest<Matte> {
    /// The number of frames.
    pub fn frames(&self) -> usize {
        self.content.frames.len()
    }

    /// The alpha of clip frame `frame`, read from `store` with its hash
    /// checked, or `None` past the matte's last frame.
    ///
    /// # Errors
    /// A [`Kind::Missing`] diagnostic when the store lacks the frame's file,
    /// a [`Kind::Invalid`] one when it has changed, does not decode, or is
    /// not of the clip's size.
    pub fn alpha(&self, store: &Store, frame: usize) -> Result<Option<Alpha>> {
        let Some(hash) = self.content.frames.get(frame) else {
            return Ok(None);
        };
        let bytes = store.get(hash)?.ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!(
                    "frame {frame} of a baked matte ({hash}) is not in the cache at {}; run `{}` on the scene",
                    store.root().display(),
                    crate::BAKE_COMMAND
                ),
            )
        })?;
        let alpha = Alpha::decode(&bytes)?;
        if [alpha.width, alpha.height] != self.size {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "frame {frame} of a baked matte is {}x{}, not the clip's {}x{}",
                    alpha.width, alpha.height, self.size[0], self.size[1]
                ),
            ));
        }
        Ok(Some(alpha))
    }
}

/// Writes a matte frame by frame into a store, then its manifest.
#[derive(Debug)]
pub struct MatteWriter<'a> {
    store: &'a Store,
    size: [u32; 2],
    frames: Vec<Sha256>,
}

impl<'a> MatteWriter<'a> {
    /// A writer of mattes of clips of `size` into `store`.
    pub fn new(store: &'a Store, size: [u32; 2]) -> Self {
        Self {
            store,
            size,
            frames: Vec::new(),
        }
    }

    /// Keeps the next frame's alpha.
    ///
    /// # Errors
    /// A [`Kind::Invalid`] diagnostic when it is not of the clip's size, and
    /// a [`Kind::Io`] one when it cannot be written.
    pub fn push(&mut self, alpha: &Alpha) -> Result<()> {
        if [alpha.width, alpha.height] != self.size
            || alpha.values.len() != (alpha.width * alpha.height) as usize
        {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "an alpha of {}x{} for a clip of {}x{}",
                    alpha.width, alpha.height, self.size[0], self.size[1]
                ),
            ));
        }
        self.frames.push(self.store.put(&alpha.encode()?)?);
        Ok(())
    }

    /// Keeps the manifest of every frame pushed, answering `request` for
    /// the clip hashing to `clip`, and returns the matte's digest.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when it cannot be written.
    pub fn finish(self, request: Value, clip: Sha256, made: Made) -> Result<Sha256> {
        Manifest::new(
            request,
            clip,
            self.size,
            made,
            Matte {
                frames: self.frames,
            },
        )
        .put(self.store)
    }
}

#[cfg(test)]
#[path = "../tests/unit/matte.rs"]
mod tests;
