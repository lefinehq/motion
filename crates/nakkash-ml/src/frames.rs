//! Clips as [`Frames`]: a folder of PNG frames, as the measured shots are
//! kept.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use nakkash_format::{Diagnostic, Kind, Result};

use crate::image::Rgb8;
use crate::segment::Frames;

/// The PNG files of a folder, in name order, as the frames of a clip.
#[derive(Debug, Clone)]
pub struct PngFolder {
    paths: Vec<PathBuf>,
    size: (usize, usize),
}

impl PngFolder {
    /// The PNG files in `folder`, the first read for the clip's size.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the folder cannot be read, and a
    /// [`Kind::Invalid`] one when it holds no PNG file or the first does
    /// not decode.
    pub fn open(folder: &Path) -> Result<Self> {
        let entries = fs::read_dir(folder).map_err(|e| Diagnostic::io("read", folder, &e))?;
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
            .collect();
        paths.sort();
        let first = paths.first().ok_or_else(|| {
            Diagnostic::new(
                Kind::Invalid,
                format!("{} holds no PNG frame", folder.display()),
            )
        })?;
        let frame = read(first)?;
        Ok(Self {
            size: (frame.width, frame.height),
            paths,
        })
    }
}

impl Frames for PngFolder {
    fn len(&self) -> usize {
        self.paths.len()
    }

    fn size(&self) -> (usize, usize) {
        self.size
    }

    fn frame(&mut self, index: usize) -> Result<Rgb8> {
        let path = &self.paths[index];
        let frame = read(path)?;
        match (frame.width, frame.height) == self.size {
            true => Ok(frame),
            false => Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "{} is not the size of the clip's first frame",
                    path.display()
                ),
            )),
        }
    }
}

/// The colour of the 8-bit PNG at `path`, its alpha dropped.
pub fn read(path: &Path) -> Result<Rgb8> {
    let bad = |why: String| Diagnostic::new(Kind::Invalid, format!("{}: {why}", path.display()));
    let file = File::open(path).map_err(|e| Diagnostic::io("open", path, &e))?;
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .map_err(|e| bad(e.to_string()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| bad("too large".into()))?;
    let mut data = vec![0; size];
    let info = reader
        .next_frame(&mut data)
        .map_err(|e| bad(e.to_string()))?;
    let (width, height) = (info.width as usize, info.height as usize);
    if info.bit_depth != png::BitDepth::Eight {
        return Err(bad("not 8 bits a channel".into()));
    }
    let data = match info.color_type {
        png::ColorType::Rgb => data[..width * height * 3].to_vec(),
        png::ColorType::Rgba => data[..width * height * 4]
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        other => return Err(bad(format!("{other:?}, not RGB"))),
    };
    Ok(Rgb8 {
        width,
        height,
        data,
    })
}

#[cfg(test)]
#[path = "../tests/unit/frames.rs"]
mod tests;
