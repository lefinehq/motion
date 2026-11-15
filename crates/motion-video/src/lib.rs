//! Video clips read without system libraries: the MP4 and QuickTime
//! containers ([`container`]), a decoder per codec behind one `Decoder` trait, and
//! [`Clip`], which finds the frame showing at a time and decodes it,
//! keeping its decoder and the frames it decoded last so playing forward
//! decodes each frame once.
//!
//! Frames come out as straight-alpha RGBA, which the caller turns into
//! whatever it draws with; this crate draws nothing.

pub mod container;
mod jpeg;
mod qtrle;

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use motion_format::{Diagnostic, Kind, Result};

use container::{FourCc, Track, name};

/// One decoded frame: straight-alpha RGBA, row by row from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Four bytes a pixel: red, green, blue, alpha.
    pub pixels: Vec<u8>,
}

/// Turns one codec's samples into frames, in decode order.
pub(crate) trait Decoder: Send {
    /// Forgets every frame before, so the next sample must be a sync one.
    fn reset(&mut self);
    /// Decodes the next sample.
    fn decode(&mut self, sample: &[u8]) -> Result<Frame>;
}

/// A codec this crate decodes: its four-letter code, its name, and how to
/// make a decoder for a track of it.
struct Codec {
    code: FourCc,
    name: &'static str,
    make: fn(&Track) -> Result<Box<dyn Decoder>>,
}

/// Every codec there is a decoder for.
const CODECS: &[Codec] = &[
    Codec {
        code: *b"rle ",
        name: "QuickTime Animation",
        make: |track| {
            Ok(Box::new(qtrle::Animation::new(
                track.width,
                track.height,
                track.depth,
            )?))
        },
    },
    Codec {
        code: *b"jpeg",
        name: "Photo JPEG",
        make: |_| Ok(Box::new(jpeg::Photo)),
    },
];

/// The name of the codec `code`, as reports say it: ours, a known one, or
/// the code itself.
pub fn codec_name(code: FourCc) -> String {
    const OTHERS: &[(FourCc, &str)] = &[
        (*b"avc1", "H.264"),
        (*b"avc3", "H.264"),
        (*b"hvc1", "H.265"),
        (*b"hev1", "H.265"),
        (*b"ap4h", "ProRes 4444"),
        (*b"apch", "ProRes 422 HQ"),
        (*b"apcn", "ProRes 422"),
        (*b"mp4v", "MPEG-4 Part 2"),
        (*b"png ", "PNG"),
    ];
    CODECS
        .iter()
        .map(|c| (c.code, c.name))
        .chain(OTHERS.iter().copied())
        .find(|(c, _)| *c == code)
        .map_or_else(|| format!("`{}`", name(code)), |(_, n)| n.to_owned())
}

/// Whether there is a decoder for the codec `code`.
pub fn decodes(code: FourCc) -> bool {
    CODECS.iter().any(|c| c.code == code)
}

/// How many decoded frames a clip keeps, so render workers asking for
/// nearby frames out of order find them without decoding again.
const KEPT: usize = 16;

/// A video clip, open for reading frames.
pub struct Clip {
    path: PathBuf,
    track: Track,
    /// Sample indices in the order they show, for finding a time's frame.
    shown: Vec<usize>,
    state: Mutex<State>,
}

/// What decoding has reached.
struct State {
    file: File,
    decoder: Box<dyn Decoder>,
    /// The next sample the decoder can take without starting again.
    next: usize,
    /// The frames decoded last, by sample index, oldest first.
    kept: VecDeque<(usize, Arc<Frame>)>,
}

impl Clip {
    /// Opens the clip at `path` and reads its first video track.
    ///
    /// # Errors
    /// An I/O diagnostic naming the file when it cannot be read, an
    /// invalid-value one when it is not a clip, and an unsupported one
    /// naming the codec when there is no decoder for it.
    pub fn open(path: &Path) -> Result<Clip> {
        let named = |d: Diagnostic| Diagnostic {
            message: format!("`{}`: {}", path.display(), d.message),
            ..d
        };
        let mut file = File::open(path).map_err(|e| Diagnostic::io("open", path, &e))?;
        let track = container::read(&mut file).map_err(named)?;
        let codec = CODECS
            .iter()
            .find(|c| c.code == track.codec)
            .ok_or_else(|| {
                named(
                    Diagnostic::new(
                        Kind::Invalid,
                        format!(
                            "video in {}, which motion cannot decode yet",
                            codec_name(track.codec)
                        ),
                    )
                    .expected(format!(
                        "a clip in {}",
                        CODECS
                            .iter()
                            .map(|c| c.name)
                            .collect::<Vec<_>>()
                            .join(" or ")
                    )),
                )
            })?;
        let decoder = (codec.make)(&track).map_err(named)?;
        let mut shown: Vec<usize> = (0..track.samples.len()).collect();
        shown.sort_by_key(|&i| track.samples[i].show);
        Ok(Clip {
            path: path.to_path_buf(),
            track,
            shown,
            state: Mutex::new(State {
                file,
                decoder,
                next: 0,
                kept: VecDeque::new(),
            }),
        })
    }

    /// Its frame size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.track.width, self.track.height)
    }

    /// Its length in seconds.
    pub fn seconds(&self) -> f64 {
        self.track.seconds()
    }

    /// The frame showing `seconds` into the clip, or `None` before it starts
    /// and from its end on.
    ///
    /// # Errors
    /// The decoder's diagnostic, naming the file, for a sample it cannot
    /// decode, and an I/O one when the file cannot be read.
    pub fn frame_at(&self, seconds: f64) -> Result<Option<Arc<Frame>>> {
        let Some(sample) = self.showing(seconds) else {
            return Ok(None);
        };
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        self.decode_to(&mut state, sample)
            .map(Some)
            .map_err(|d| Diagnostic {
                message: format!("`{}`: {}", self.path.display(), d.message),
                ..d
            })
    }

    /// The sample showing at `seconds`, by its index in decode order.
    fn showing(&self, seconds: f64) -> Option<usize> {
        let ticks = seconds * f64::from(self.track.timescale) - self.track.shift as f64;
        let samples = &self.track.samples;
        // The last sample to show at or before `ticks`, if it still shows.
        let at = self
            .shown
            .partition_point(|&i| samples[i].show as f64 <= ticks);
        let i = *self.shown.get(at.checked_sub(1)?)?;
        let s = samples[i];
        (ticks < (s.show + i64::from(s.duration)) as f64).then_some(i)
    }

    /// Decodes up to sample `target` and returns its frame, starting again
    /// from the sync sample before it unless decoding forward reaches it.
    fn decode_to(&self, state: &mut State, target: usize) -> Result<Arc<Frame>> {
        if let Some((_, frame)) = state.kept.iter().find(|(i, _)| *i == target) {
            return Ok(Arc::clone(frame));
        }
        let sync = self.track.sync_before(target);
        if state.next > target || state.next < sync {
            state.decoder.reset();
            state.next = sync;
        }
        let mut last = None;
        while state.next <= target {
            let sample = self.track.samples[state.next];
            let mut bytes = vec![0; sample.size as usize];
            state
                .file
                .seek(SeekFrom::Start(sample.offset))
                .and_then(|_| state.file.read_exact(&mut bytes))
                .map_err(|e| Diagnostic::io("read", &self.path, &e))?;
            let frame = Arc::new(state.decoder.decode(&bytes)?);
            state.kept.push_back((state.next, Arc::clone(&frame)));
            if state.kept.len() > KEPT {
                state.kept.pop_front();
            }
            state.next += 1;
            last = Some(frame);
        }
        Ok(last.expect("the target was decoded"))
    }
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
