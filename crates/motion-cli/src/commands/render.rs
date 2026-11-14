//! `motion render`: draw frames of a scene into PNG files.

use std::fs;
use std::ops::Range;
use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use motion_engine::{Diagnostic, Quality, RenderOptions, Result, Session};

/// The frames to draw, as typed: `N` or `A-B`, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frames {
    /// The first frame.
    pub first: u32,
    /// The last frame, included.
    pub last: u32,
}

impl Frames {
    /// One frame.
    pub fn one(frame: u32) -> Frames {
        Frames {
            first: frame,
            last: frame,
        }
    }

    /// The frames as a half-open range.
    fn range(self) -> Range<u32> {
        self.first..self.last.saturating_add(1)
    }
}

impl FromStr for Frames {
    type Err = String;

    /// Parses `N` or `A-B` with `A` at most `B`.
    fn from_str(text: &str) -> std::result::Result<Frames, String> {
        let frame = |s: &str| {
            s.trim()
                .parse::<u32>()
                .map_err(|_| format!("`{s}` is not a frame number; expected `N` or `A-B`"))
        };
        let frames = match text.split_once('-') {
            None => Frames::one(frame(text)?),
            Some((first, last)) => Frames {
                first: frame(first)?,
                last: frame(last)?,
            },
        };
        match frames.first <= frames.last {
            true => Ok(frames),
            false => Err(format!("`{text}` ends before it starts")),
        }
    }
}

/// The name of frame `frame`'s file: `frame-00042.png`.
pub fn file_name(frame: u32) -> String {
    format!("frame-{frame:05}.png")
}

/// Draws `frames` of the scene (all of them when `None`) into `out`, which is
/// created if missing, and prints how many were written.
pub fn run(path: &Path, frames: Option<Frames>, out: &Path, quality: Quality) -> Result<ExitCode> {
    let session = Session::open(path)?;
    let frames = match frames {
        Some(frames) => frames.range(),
        None => session.frames()?,
    };
    fs::create_dir_all(out).map_err(|e| Diagnostic::io("create", out, &e))?;
    let options = RenderOptions {
        quality,
        ..RenderOptions::default()
    };
    session.render_frames(frames.clone(), options, |frame, image| {
        let file = out.join(file_name(frame));
        fs::write(&file, image.to_png()?).map_err(|e| Diagnostic::io("write", &file, &e))
    })?;
    let count = frames.len();
    let noun = if count == 1 { "frame" } else { "frames" };
    println!("{count} {noun} written to {}", out.display());
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
#[path = "../../tests/unit/commands/render.rs"]
mod tests;
