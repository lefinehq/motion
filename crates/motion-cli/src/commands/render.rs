//! `motion render`: draw frames of a scene into a video file or PNG files.

use std::fs;
use std::ops::Range;
use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use motion_encode::{Codec, Container, VideoSpec, VideoWriter};
use motion_engine::{Diagnostic, Kind, Quality, RenderOptions, Result, Session};

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

/// Draws `frames` of the scene (all of them when `None`) into `out`: a video
/// file when its extension names a container, else a folder of PNG files,
/// created if missing. Prints what was written.
pub fn run(path: &Path, frames: Option<Frames>, out: &Path, quality: Quality) -> Result<ExitCode> {
    let session = Session::open(path)?;
    let frames = match frames {
        Some(frames) => frames.range(),
        None => session.frames()?,
    };
    let options = RenderOptions {
        quality,
        ..RenderOptions::default()
    };
    match Container::of(out) {
        Some(_) => video(&session, frames, options, out),
        None => pictures(&session, frames, options, out),
    }
}

/// Draws `frames` into PNG files in the folder `out`.
fn pictures(
    session: &Session,
    frames: Range<u32>,
    options: RenderOptions,
    out: &Path,
) -> Result<ExitCode> {
    fs::create_dir_all(out).map_err(|e| Diagnostic::io("create", out, &e))?;
    session.render_frames(frames.clone(), options, |frame, image| {
        let file = out.join(file_name(frame));
        fs::write(&file, image.to_png()?).map_err(|e| Diagnostic::io("write", &file, &e))
    })?;
    let count = frames.len();
    let noun = if count == 1 { "frame" } else { "frames" };
    println!("{count} {noun} written to {}", out.display());
    Ok(ExitCode::SUCCESS)
}

/// Draws `frames` into the video file `out`, in AV1 for now.
fn video(
    session: &Session,
    frames: Range<u32>,
    options: RenderOptions,
    out: &Path,
) -> Result<ExitCode> {
    let fps = session.scene().root.fps;
    let codec = Codec::Av1;
    // The writer takes its size from the first frame, as drawn.
    let mut writer: Option<VideoWriter> = None;
    let mut size = (0, 0);
    session.render_in_order(frames, options, |_, image| {
        let writer = match &mut writer {
            Some(writer) => writer,
            None => {
                size = (image.width(), image.height());
                let spec = VideoSpec {
                    width: size.0,
                    height: size.1,
                    fps,
                };
                writer.insert(VideoWriter::create(out, spec, codec)?)
            }
        };
        writer.push(image.pixels())
    })?;
    let Some(writer) = writer else {
        return Err(
            Diagnostic::new(Kind::OutOfRange, "no frames to write a video of")
                .expected("a range of at least one frame"),
        );
    };
    let count = writer.finish()?;
    let noun = if count == 1 { "frame" } else { "frames" };
    let (width, height) = size;
    let seconds = (count as f64 / fps * 100.0).round() / 100.0;
    println!(
        "{count} {noun} written to {} ({}, {width}x{height}, {fps} fps, {seconds} s)",
        out.display(),
        codec.to_string().to_uppercase(),
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
#[path = "../../tests/unit/commands/render.rs"]
mod tests;
