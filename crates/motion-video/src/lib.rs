//! Video clips: the MP4 and QuickTime containers ([`container`]), read
//! without system libraries, a decoder per codec behind one `Decoder`
//! trait, and [`Clip`], which finds the frame showing at a time and decodes
//! it, keeping its decoder and the frames it decoded last so playing
//! forward decodes each frame once.
//!
//! QuickTime Animation and Photo JPEG decode in this crate. H.264 decodes
//! through the operating system's decoder where there is one ([D60]): on
//! macOS, VideoToolbox, through `motion-videotoolbox`. The colour of its
//! samples is read here on every platform ([`avc`], [`ycbcr`]), so another
//! backend, such as a decoder loaded at run time, only has to hand back
//! planes.
//!
//! Frames come out as straight-alpha RGBA, which the caller turns into
//! whatever it draws with; this crate draws nothing.
//!
//! [D60]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d60-h264-footage-decodes-through-the-systems-decoder

mod avc;
pub mod container;
mod h264;
mod jpeg;
mod qtrle;
#[cfg(target_os = "macos")]
mod toolbox;
mod ycbcr;

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

/// Turns one codec's samples into pictures, in decode order.
pub(crate) trait Decoder: Send {
    /// Forgets every frame before, so the next sample must be a sync one.
    fn reset(&mut self);
    /// Decodes the next sample into a picture, which becomes a frame only
    /// when one is asked of it: a sample decoded on the way to another
    /// costs no conversion.
    fn decode(&mut self, sample: &[u8]) -> Result<Box<dyn Picture>>;
    /// Whether no other sample refers to `sample`, so it can go undecoded
    /// when its own frame is not wanted.
    fn disposable(&self, _sample: &[u8]) -> bool {
        false
    }
}

/// A decoded picture, not yet in RGBA.
pub(crate) trait Picture: Send {
    /// Its frame.
    fn frame(self: Box<Self>) -> Result<Frame>;
}

/// A decoder that makes RGBA directly hands back its frame.
impl Picture for Frame {
    fn frame(self: Box<Self>) -> Result<Frame> {
        Ok(*self)
    }
}

/// A codec this crate decodes: its four-letter code, its name, whether
/// there is a decoder for it on this platform, and how to make one for a
/// track of it.
struct Codec {
    code: FourCc,
    name: &'static str,
    here: bool,
    make: fn(&Track) -> Result<Box<dyn Decoder>>,
}

/// Every codec there is a decoder for.
const CODECS: &[Codec] = &[
    Codec {
        code: *b"rle ",
        name: "QuickTime Animation",
        here: true,
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
        here: true,
        make: |_| Ok(Box::new(jpeg::Photo)),
    },
    Codec {
        code: *b"avc1",
        name: "H.264",
        here: h264::HERE,
        make: h264::decoder,
    },
    Codec {
        code: *b"avc3",
        name: "H.264",
        here: h264::HERE,
        make: h264::decoder,
    },
];

/// The name of the codec `code`, as reports say it: ours, a known one, or
/// the code itself.
pub fn codec_name(code: FourCc) -> String {
    const OTHERS: &[(FourCc, &str)] = &[
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

/// Whether there is a decoder for the codec `code` on this platform.
pub fn decodes(code: FourCc) -> bool {
    CODECS.iter().any(|c| c.code == code && c.here)
}

/// How many decoded pictures a clip keeps, so render workers asking for
/// nearby frames out of order find them without decoding again, and a
/// clip whose frames show in another order than they decode (B-frames)
/// finds the ones decoded early.
const KEPT: usize = 16;

/// How far, in track ticks, a time may fall short of a frame's start and
/// still show it: times come in as seconds in floating point, and a frame
/// time such as 1001/24000 s does not survive the trip exactly.
const SNAP: f64 = 1e-6;

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
    /// The pictures decoded last, by sample index, oldest first.
    kept: VecDeque<(usize, Arc<Kept>)>,
}

/// One kept picture, made a frame the first time it is asked for, outside
/// the clip's lock, so render workers convert frames side by side while
/// samples decode one at a time.
struct Kept {
    /// `None` only while it converts, or after a conversion that panicked.
    slot: Mutex<Option<Slot>>,
}

enum Slot {
    Picture(Box<dyn Picture>),
    Converted(Result<Arc<Frame>>),
}

impl Kept {
    fn new(picture: Box<dyn Picture>) -> Kept {
        Kept {
            slot: Mutex::new(Some(Slot::Picture(picture))),
        }
    }

    /// Its frame, converted on the first call.
    fn frame(&self) -> Result<Arc<Frame>> {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        let frame = match slot.take() {
            Some(Slot::Picture(picture)) => picture.frame().map(Arc::new),
            Some(Slot::Converted(frame)) => frame,
            None => Err(Diagnostic::new(
                Kind::Invalid,
                "a frame whose conversion stopped part-way",
            )),
        };
        *slot = Some(Slot::Converted(frame.clone()));
        frame
    }
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
            .find(|c| c.code == track.codec && c.here)
            .ok_or_else(|| {
                if CODECS.iter().any(|c| c.code == track.codec) {
                    return named(h264::unsupported_here(&codec_name(track.codec)));
                }
                named(
                    Diagnostic::new(
                        Kind::Invalid,
                        format!(
                            "video in {}, which motion cannot decode yet",
                            codec_name(track.codec)
                        ),
                    )
                    .expected(format!("a clip in {}", here_names().join(" or "))),
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
        let named = |d: Diagnostic| Diagnostic {
            message: format!("`{}`: {}", self.path.display(), d.message),
            ..d
        };
        let kept = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            self.decode_to(&mut state, sample).map_err(named)?
        };
        kept.frame().map(Some).map_err(named)
    }

    /// The sample showing at `seconds`, by its index in decode order: the
    /// last to start at or before it, while it lasts.
    fn showing(&self, seconds: f64) -> Option<usize> {
        let ticks = seconds * f64::from(self.track.timescale) - self.track.shift as f64;
        let ticks = match (ticks - ticks.round()).abs() < SNAP {
            true => ticks.round(),
            false => ticks,
        };
        let samples = &self.track.samples;
        // The last sample to show at or before `ticks`, if it still shows.
        let at = self
            .shown
            .partition_point(|&i| samples[i].show as f64 <= ticks);
        let i = *self.shown.get(at.checked_sub(1)?)?;
        let s = samples[i];
        (ticks < (s.show + i64::from(s.duration)) as f64).then_some(i)
    }

    /// Decodes up to sample `target` and returns its picture, starting
    /// again from the sync sample it decodes from unless decoding forward
    /// reaches it. A sample on the way that would leave the kept pictures
    /// before the target is decoded only if another refers to it.
    fn decode_to(&self, state: &mut State, target: usize) -> Result<Arc<Kept>> {
        if let Some((_, kept)) = state.kept.iter().find(|(i, _)| *i == target) {
            return Ok(Arc::clone(kept));
        }
        let start = self.track.start_for(target);
        if state.next > target || state.next < start {
            state.decoder.reset();
            state.next = start;
        }
        let mut bytes = Vec::new();
        while state.next <= target {
            let sample = self.track.samples[state.next];
            bytes.resize(sample.size as usize, 0);
            state
                .file
                .seek(SeekFrom::Start(sample.offset))
                .and_then(|_| state.file.read_exact(&mut bytes))
                .map_err(|e| Diagnostic::io("read", &self.path, &e))?;
            let far = target - state.next >= KEPT;
            if far && state.decoder.disposable(&bytes) {
                state.next += 1;
                continue;
            }
            let picture = state.decoder.decode(&bytes)?;
            state
                .kept
                .push_back((state.next, Arc::new(Kept::new(picture))));
            if state.kept.len() > KEPT {
                state.kept.pop_front();
            }
            state.next += 1;
        }
        let (_, kept) = state.kept.back().expect("the target was decoded");
        Ok(Arc::clone(kept))
    }
}

/// The names of the codecs there is a decoder for on this platform.
fn here_names() -> Vec<&'static str> {
    let mut names: Vec<&str> = CODECS.iter().filter(|c| c.here).map(|c| c.name).collect();
    names.dedup();
    names
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
