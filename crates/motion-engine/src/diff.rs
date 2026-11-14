//! Fidelity against a reference render ([D35]): each reference frame is
//! compared with the scene drawn at the reference frame's exact time, and the
//! error is measured twice: where the scene draws in that frame, and over
//! everything it draws anywhere in the compared frames, so an element that
//! is missing or late still counts against it. What the scene does not draw,
//! such as footage a template leaves to its user, is not measured.
//!
//! Errors are on 8-bit sRGB values scaled to 0..1. A pixel fails when any
//! channel differs by more than the tolerance (Blender's render tests use
//! 0.016); the per-frame numbers are pooled by mean, 99th percentile and
//! worst frame, since a mean hides a few bad frames.
//!
//! [D35]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d35-fidelity-is-measured-against-reference-renders

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use motion_format::{Diagnostic, Kind, Result};
use motion_raster::{Frame, Image, RenderOptions};
use serde::Serialize;

use crate::Session;

/// What to compare with, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffOptions {
    /// The reference frames, in order: PNG files of the scene's size.
    pub frames: Vec<PathBuf>,
    /// The reference's frame rate: frame `i` is at `start + i / fps` seconds.
    pub fps: f64,
    /// The scene time of the first reference frame, in seconds.
    pub start: f64,
    /// How far a channel may differ, from 0 to 1, before its pixel fails.
    pub tolerance: f64,
    /// A folder to write one error image per frame into, if any.
    pub images: Option<PathBuf>,
}

impl DiffOptions {
    /// Blender's per-channel tolerance for its render tests.
    pub const TOLERANCE: f64 = 0.016;

    /// The PNG files in `dir`, sorted by name, as reference frames.
    ///
    /// # Errors
    /// An I/O diagnostic when the folder cannot be read, or a missing-value
    /// diagnostic when it holds no PNG.
    pub fn frames_in(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut frames: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| Diagnostic::io("read", dir, &e))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            })
            .collect();
        frames.sort();
        match frames.is_empty() {
            true => Err(Diagnostic::new(
                Kind::Missing,
                format!("no PNG frames in {}", dir.display()),
            )
            .expected("a folder of reference frames, such as `frame-0001.png`, in order")),
            false => Ok(frames),
        }
    }
}

/// The error over one set of pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Error {
    /// How many pixels were measured.
    pub pixels: u64,
    /// The mean absolute difference over their channels, 0 to 1.
    pub mean: f64,
    /// The peak signal-to-noise ratio in dB; `None` when they are equal.
    pub psnr: Option<f64>,
    /// The share of them with a channel past the tolerance, 0 to 1.
    pub failing: f64,
}

/// One reference frame against the scene.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameDiff {
    /// The reference frame's file name.
    pub file: String,
    /// The scene time it was compared at, in seconds.
    pub time: f64,
    /// The error where the scene draws in this frame.
    pub drawn: Error,
    /// The error over everything the scene draws in any compared frame.
    pub union: Error,
}

/// One error measure pooled over the frames.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Pooled {
    /// The mean over the frames.
    pub mean: f64,
    /// The 99th percentile over the frames.
    pub p99: f64,
    /// The worst frame's value.
    pub worst: f64,
    /// The worst frame's file name.
    pub worst_frame: String,
}

/// The whole comparison.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diff {
    /// The tolerance used, 0 to 1.
    pub tolerance: f64,
    /// Every frame, in order.
    pub frames: Vec<FrameDiff>,
    /// The mean error where the scene draws, pooled.
    pub drawn_mean: Pooled,
    /// The failing share where the scene draws, pooled.
    pub drawn_failing: Pooled,
    /// The mean error over all it draws, pooled.
    pub union_mean: Pooled,
    /// The failing share over all it draws, pooled.
    pub union_failing: Pooled,
}

/// One frame drawn and read: our pixels, where we draw, and the reference's.
struct Pair {
    /// Our frame, as `render` draws it.
    ours: Image,
    /// Whether we draw each pixel, before the background.
    coverage: Vec<bool>,
    /// The reference frame.
    reference: Image,
}

impl Session {
    /// Compares the scene with the reference frames in `options`.
    ///
    /// # Errors
    /// A diagnostic when a reference frame cannot be read, is not the
    /// scene's size, or falls past the scene's end; or as
    /// [`Session::render_with`].
    pub fn diff(&self, options: &DiffOptions) -> Result<Diff> {
        let fps = self.scene().fps;
        let duration = self.graph()?.duration_frames;
        let times: Vec<f64> = (0..options.frames.len())
            .map(|i| options.start + i as f64 / options.fps)
            .collect();
        if let Some((path, time)) = options
            .frames
            .iter()
            .zip(&times)
            .find(|(_, time)| **time * fps > duration)
        {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!(
                    "reference frame {} is at {time:.3}s, past the scene's end",
                    path.display()
                ),
            )
            .expected("as many reference frames as the scene lasts, or a later `--start`"));
        }
        let union = self.union(options, &times)?;
        let frames = self.each_pair(options, &times, &|i, pair| {
            let file = file_name(&options.frames[i]);
            let drawn = error(pair, &pair.coverage, options.tolerance);
            let union_error = error(pair, &union, options.tolerance);
            if let Some(dir) = &options.images {
                write_error_image(dir, &file, pair, &union, options.tolerance)?;
            }
            Ok(FrameDiff {
                file,
                time: times[i],
                drawn,
                union: union_error,
            })
        })?;
        let pool = |pick: fn(&FrameDiff) -> f64| pooled(&frames, pick);
        Ok(Diff {
            tolerance: options.tolerance,
            drawn_mean: pool(|f| f.drawn.mean),
            drawn_failing: pool(|f| f.drawn.failing),
            union_mean: pool(|f| f.union.mean),
            union_failing: pool(|f| f.union.failing),
            frames,
        })
    }

    /// Where the scene draws in any of the frames.
    fn union(&self, options: &DiffOptions, times: &[f64]) -> Result<Vec<bool>> {
        let masks = self.each_pair(options, times, &|_, pair| Ok(pair.coverage.clone()))?;
        Ok(masks.into_iter().fold(Vec::new(), |mut union, mask| {
            if union.is_empty() {
                return mask;
            }
            union.iter_mut().zip(mask).for_each(|(u, m)| *u |= m);
            union
        }))
    }

    /// Calls `each` on every frame drawn and read, on as many threads as the
    /// processor has, and returns the results in frame order, or the first
    /// frame's error.
    fn each_pair<T: Send>(
        &self,
        options: &DiffOptions,
        times: &[f64],
        each: &(dyn Fn(usize, &Pair) -> Result<T> + Sync),
    ) -> Result<Vec<T>> {
        let threads = thread::available_parallelism().map_or(1, usize::from);
        let indices: Vec<usize> = (0..times.len()).collect();
        let chunk = indices.len().div_ceil(threads).max(1);
        let results: Vec<Vec<(usize, Result<T>)>> = thread::scope(|scope| {
            let handles: Vec<_> = indices
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|&i| {
                                (
                                    i,
                                    self.pair(options, times[i], i)
                                        .and_then(|pair| each(i, &pair)),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a diff thread does not panic"))
                .collect()
        });
        let mut ordered: Vec<(usize, Result<T>)> = results.into_iter().flatten().collect();
        ordered.sort_by_key(|(i, _)| *i);
        ordered.into_iter().map(|(_, result)| result).collect()
    }

    /// Reference frame `index` and the scene drawn at `time` seconds.
    fn pair(&self, options: &DiffOptions, time: f64, index: usize) -> Result<Pair> {
        let path = &options.frames[index];
        let bytes = fs::read(path).map_err(|e| Diagnostic::io("read", path, &e))?;
        let reference = Image::from_png(&bytes).map_err(|d| d.at(path.display().to_string()))?;
        let list = self.display_list(time * self.scene().fps)?;
        let graph = self.graph()?;
        let (width, height) = (pixels(graph.width), pixels(graph.height));
        if (reference.width(), reference.height()) != (width, height) {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "reference frame {} is {}x{}, the scene {width}x{height}",
                    path.display(),
                    reference.width(),
                    reference.height()
                ),
            )
            .expected("reference frames of the scene's size"));
        }
        let options = RenderOptions::default();
        let ours = motion_raster::render_with(&list, width, height, options)?;
        let bare = Frame {
            background: None,
            ..list
        };
        let coverage = motion_raster::render_with(&bare, width, height, options)?
            .pixels()
            .chunks_exact(4)
            .map(|p| p[3] > 0)
            .collect();
        Ok(Pair {
            ours,
            coverage,
            reference,
        })
    }
}

/// A scene size in whole pixels, as the rasteriser takes it.
fn pixels(size: f64) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a canvas size"
    )]
    let rounded = size.round().clamp(0.0, f64::from(u32::MAX)) as u32;
    rounded
}

/// The error of `pair` over the pixels `mask` holds.
fn error(pair: &Pair, mask: &[bool], tolerance: f64) -> Error {
    let limit = tolerance * 255.0;
    let (mut pixels, mut sum, mut squares, mut failing) = (0u64, 0.0, 0.0, 0u64);
    pair.ours
        .pixels()
        .chunks_exact(4)
        .zip(pair.reference.pixels().chunks_exact(4))
        .zip(mask)
        .filter(|(_, inside)| **inside)
        .for_each(|((a, b), _)| {
            pixels += 1;
            let differences = [0, 1, 2].map(|i| (f64::from(a[i]) - f64::from(b[i])).abs());
            sum += differences.iter().sum::<f64>();
            squares += differences.iter().map(|d| d * d).sum::<f64>();
            if differences.iter().any(|d| *d > limit) {
                failing += 1;
            }
        });
    if pixels == 0 {
        return Error::default();
    }
    #[expect(clippy::cast_precision_loss, reason = "pixel counts")]
    let (count, channels) = (pixels as f64, pixels as f64 * 3.0);
    let mse = squares / channels;
    Error {
        pixels,
        mean: sum / channels / 255.0,
        psnr: (mse > 0.0).then(|| 10.0 * (255.0 * 255.0 / mse).log10()),
        #[expect(clippy::cast_precision_loss, reason = "pixel counts")]
        failing: failing as f64 / count,
    }
}

/// `pick` of every frame, pooled.
fn pooled(frames: &[FrameDiff], pick: fn(&FrameDiff) -> f64) -> Pooled {
    let mut values: Vec<(f64, &str)> = frames.iter().map(|f| (pick(f), f.file.as_str())).collect();
    if values.is_empty() {
        return Pooled::default();
    }
    #[expect(clippy::cast_precision_loss, reason = "frame counts")]
    let mean = values.iter().map(|(v, _)| v).sum::<f64>() / values.len() as f64;
    values.sort_by(|a, b| a.0.total_cmp(&b.0));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "an index into the frames"
    )]
    let p99 = values[((values.len() - 1) as f64 * 0.99).round() as usize].0;
    let (worst, worst_frame) = values[values.len() - 1];
    Pooled {
        mean,
        p99,
        worst,
        worst_frame: worst_frame.to_owned(),
    }
}

/// The file name of `path`.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Writes the error image of one frame into `dir`: the reference dimmed to
/// grey, with each measured pixel's error in red and the failing ones bright.
fn write_error_image(
    dir: &Path,
    file: &str,
    pair: &Pair,
    union: &[bool],
    tolerance: f64,
) -> Result<()> {
    fs::create_dir_all(dir).map_err(|e| Diagnostic::io("create", dir, &e))?;
    let limit = tolerance * 255.0;
    let pixels = pair
        .ours
        .pixels()
        .chunks_exact(4)
        .zip(pair.reference.pixels().chunks_exact(4))
        .zip(union)
        .flat_map(|((a, b), inside)| {
            let grey = ((u16::from(b[0]) + u16::from(b[1]) + u16::from(b[2])) / 12) as u8;
            let error = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0);
            match (*inside, f64::from(error) > limit) {
                (true, true) => [255, grey, grey, 255],
                (true, false) => [
                    grey.saturating_add(error.saturating_mul(4)),
                    grey,
                    grey,
                    255,
                ],
                (false, _) => [grey, grey, grey.saturating_add(24), 255],
            }
        })
        .collect();
    let image = Image::new(pair.ours.width(), pair.ours.height(), pixels)?;
    let path = dir.join(file);
    fs::write(&path, image.to_png()?).map_err(|e| Diagnostic::io("write", &path, &e))
}

#[cfg(test)]
#[path = "../tests/unit/diff.rs"]
mod tests;
