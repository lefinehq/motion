//! `nakkash render`: draw frames of a scene into a video file or PNG files.

use std::fs;
use std::ops::Range;
use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use nakkash_encode::{Codec, Container, Soundtrack, VideoQuality, VideoSpec, VideoWriter};
use nakkash_engine::telemetry::timing::timed;
use nakkash_engine::{Diagnostic, Kind, Quality, RenderOptions, Result, Session};

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
    pub fn range(self) -> Range<u32> {
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

/// How a video file is encoded, as asked for; unset parts take the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Encoding {
    /// `--codec`.
    pub codec: Option<Codec>,
    /// `--video-quality`.
    pub quality: Option<VideoQuality>,
}

impl Encoding {
    /// The encoding the arguments name.
    ///
    /// # Errors
    /// An unknown-name diagnostic for a codec or quality this build lacks.
    pub fn named(codec: Option<&str>, quality: Option<&str>) -> Result<Encoding> {
        Ok(Encoding {
            codec: codec.map(Codec::named).transpose()?,
            quality: quality.map(VideoQuality::named).transpose()?,
        })
    }
}

/// What draws the frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Backend {
    /// The CPU: the reference, and the default.
    #[default]
    Cpu,
    /// The GPU where it has kernels, the CPU elsewhere: frames that
    /// approximate the reference ([D74]).
    ///
    /// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
    Gpu,
}

impl Backend {
    /// The backend `--gpu` asks for.
    pub fn from_flag(gpu: bool) -> Backend {
        match gpu {
            true => Backend::Gpu,
            false => Backend::Cpu,
        }
    }
}

/// Hands `session`'s renders to the GPU, saying on stderr that its frames
/// approximate the reference, or why there is none and the CPU draws.
fn use_gpu(session: &mut Session) {
    match nakkash_engine::gpu::open() {
        Ok(opened) => {
            eprintln!(
                "note: --gpu draws on {}: frames approximate the CPU reference within D74's tolerances; render without --gpu to deliver",
                opened.adapter
            );
            session.set_accelerator(Some(opened.accelerator));
        }
        Err(refusal) => eprintln!("warning: --gpu: {refusal}; drawing on the CPU"),
    }
}

/// Says on stderr what the GPU did, and which effects it left to the CPU.
fn report_gpu(session: &Session) {
    let Some(tally) = session.tally() else {
        return;
    };
    let list = |effects: &[(String, u64)]| {
        let runs: u64 = effects.iter().map(|(_, n)| n).sum();
        let names: Vec<String> = effects.iter().map(|(e, n)| format!("{e} {n}")).collect();
        match names.is_empty() {
            true => "0".to_owned(),
            false => format!("{runs} ({})", names.join(", ")),
        }
    };
    let gib = |bytes: u64| bytes as f64 / f64::from(1u32 << 30);
    eprintln!(
        "gpu: effects on the GPU {}; on the CPU, without a kernel {}; {} other operations; {} uploads ({:.2} GiB), {} downloads ({:.2} GiB); {} submissions",
        list(&tally.accelerated),
        list(&tally.fell_back),
        tally.operations,
        tally.uploads.0,
        gib(tally.uploads.1),
        tally.downloads.0,
        gib(tally.downloads.1),
        tally.submissions,
    );
}

/// Draws `frames` of the scene (all of them when `None`) into `out`: a video
/// file encoded as `encoding` asks when its extension names a container,
/// else a folder of PNG files, created if missing, on `backend`. Prints what
/// was written. With no `out`, nothing is written: each frame's digest is
/// printed instead ([`digests`]).
///
/// # Errors
/// As drawing and writing, or an invalid-value diagnostic when `encoding`
/// is set for PNG files.
pub fn run(
    path: &Path,
    frames: Option<Frames>,
    out: Option<&Path>,
    quality: Quality,
    encoding: Encoding,
    backend: Backend,
) -> Result<ExitCode> {
    let Some(out) = out else {
        let mut session = Session::open(path)?;
        if backend == Backend::Gpu {
            use_gpu(&mut session);
        }
        let frames = match frames {
            Some(frames) => frames.range(),
            None => session.frames()?,
        };
        let options = RenderOptions {
            quality,
            ..RenderOptions::default()
        };
        for line in digests(&session, frames, options)? {
            println!("{line}");
        }
        report_gpu(&session);
        return Ok(ExitCode::SUCCESS);
    };
    if is_sound(out) {
        if encoding != Encoding::default() {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "`--codec` or `--video-quality` for the sound file `{}`",
                    out.display()
                ),
            )
            .expected("a video file for `--out`, such as `launch.mp4`"));
        }
        let session = Session::open(path)?;
        let frames = match frames {
            Some(frames) => frames.range(),
            None => session.frames()?,
        };
        println!("{}", sound(&session, frames, out)?);
        return Ok(ExitCode::SUCCESS);
    }
    let is_video = Container::of(out).is_some();
    if !is_video && encoding != Encoding::default() {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "`--codec` or `--video-quality` for the folder `{}`",
                out.display()
            ),
        )
        .expected("a video file for `--out`, such as `launch.mp4`"));
    }
    let mut session = Session::open(path)?;
    if backend == Backend::Gpu {
        use_gpu(&mut session);
    }
    let frames = match frames {
        Some(frames) => frames.range(),
        None => session.frames()?,
    };
    let options = RenderOptions {
        quality,
        ..RenderOptions::default()
    };
    let done = match is_video {
        true => {
            let written = video(&session, frames, options, out, encoding)?;
            println!("{written}");
            Ok(ExitCode::SUCCESS)
        }
        false => pictures(&session, frames, options, out),
    };
    report_gpu(&session);
    done
}

/// Whether `out` names a sound file, which gets the mix alone: a `.wav`.
fn is_sound(out: &Path) -> bool {
    out.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}

/// Mixes the sound of `frames` into the WAV file `out`, as 32-bit float
/// stereo at the mix rate, and says what was written: how long, and how
/// loud its loudest sample is.
///
/// # Errors
/// As mixing and writing.
pub fn sound(session: &Session, frames: Range<u32>, out: &Path) -> Result<String> {
    let range = f64::from(frames.start)..f64::from(frames.end);
    let mix = session.mix(range)?;
    fs::write(out, nakkash_audio::wav::encode(&mix))
        .map_err(|e| Diagnostic::io("write", out, &e))?;
    let peak = mix
        .channels()
        .iter()
        .flatten()
        .fold(0.0_f32, |peak, s| peak.max(s.abs()));
    let level = match peak > 0.0 {
        true => format!("peak {:.1} dBFS", 20.0 * f64::from(peak).log10()),
        false => "silent".into(),
    };
    let over = match peak > 1.0 {
        true => ", above full scale: lower a `volume`",
        false => "",
    };
    let seconds = (mix.seconds() * 100.0).round() / 100.0;
    Ok(format!(
        "mix written to {} ({seconds} s, {} Hz stereo, {level}{over})",
        out.display(),
        mix.rate()
    ))
}

/// A video file written: where, how, and how long.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// The file.
    pub out: std::path::PathBuf,
    /// How many frames it holds.
    pub frames: u64,
    /// Its codec.
    pub codec: Codec,
    /// Its quality.
    pub quality: VideoQuality,
    /// Its size in pixels.
    pub size: (u32, u32),
    /// Its frames per second.
    pub fps: f64,
    /// Whether it holds the scene's sound.
    pub sound: bool,
}

impl std::fmt::Display for Written {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let noun = if self.frames == 1 { "frame" } else { "frames" };
        let (width, height) = self.size;
        let seconds = (self.frames as f64 / self.fps * 100.0).round() / 100.0;
        write!(
            f,
            "{} {noun} written to {} ({}, {}, {width}x{height}, {} fps, {seconds} s{})",
            self.frames,
            self.out.display(),
            self.codec.label(),
            self.quality,
            self.fps,
            if self.sound { ", with sound" } else { "" },
        )
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

/// One line per frame of `frames`, in frame order: its number and the
/// SHA-256 of its RGBA bytes, the pixels a PNG of it holds. Nothing is
/// written anywhere, so renders compare bit for bit without a disk.
///
/// # Errors
/// As drawing.
pub fn digests(
    session: &Session,
    frames: Range<u32>,
    options: RenderOptions,
) -> Result<Vec<String>> {
    use sha2::{Digest, Sha256};
    let mut lines = Vec::with_capacity(frames.len());
    session.render_in_order(frames, options, |frame, image| {
        let hash = Sha256::digest(image.pixels());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        lines.push(format!(
            "frame {frame} {}x{} sha256:{hex}",
            image.width(),
            image.height()
        ));
        Ok(())
    })?;
    Ok(lines)
}

/// Draws `frames` into the video file `out`, encoded as `encoding` asks.
///
/// # Errors
/// As drawing and writing, or an out-of-range diagnostic for no frames.
pub fn video(
    session: &Session,
    frames: Range<u32>,
    options: RenderOptions,
    out: &Path,
    encoding: Encoding,
) -> Result<Written> {
    let fps = session.scene().root.fps;
    let codec = encoding.codec.unwrap_or_default();
    let quality = encoding.quality.unwrap_or(codec.default_quality());
    // The sound of the frames drawn, when the scene plays any.
    let sound = match session.plays_sound()? {
        true => {
            let range = f64::from(frames.start)..f64::from(frames.end);
            let mix = session.mix(range)?;
            Some(Soundtrack {
                rate: mix.rate(),
                channels: mix.channels().to_vec(),
            })
        }
        false => None,
    };
    // The writer takes its size from the first frame, as drawn.
    let mut writer: Option<VideoWriter> = None;
    let mut size = (0, 0);
    session.render_in_order(frames, options, |_, image| {
        timed("encode", || {
            let writer = match &mut writer {
                Some(writer) => writer,
                None => {
                    size = (image.width(), image.height());
                    let spec = VideoSpec {
                        width: size.0,
                        height: size.1,
                        fps,
                    };
                    let mut created = VideoWriter::create(out, spec, codec, quality)?;
                    if let Some(sound) = &sound {
                        created.set_sound(sound)?;
                    }
                    writer.insert(created)
                }
            };
            writer.push(image.pixels())
        })
    })?;
    let Some(writer) = writer else {
        return Err(
            Diagnostic::new(Kind::OutOfRange, "no frames to write a video of")
                .expected("a range of at least one frame"),
        );
    };
    let frames = timed("encode", || writer.finish())?;
    Ok(Written {
        out: out.to_path_buf(),
        frames,
        codec,
        quality,
        size,
        fps,
        sound: sound.is_some(),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/commands/render.rs"]
mod tests;
