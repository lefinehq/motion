//! The MP4 container ([D46]), written by us on `mp4-atom`'s boxes so that
//! nothing but the sample tables waits in memory: each packet goes to disk
//! as it comes, and the index of where they are is written at the end.
//!
//! The samples are written to a scratch file first. At the end the file is
//! laid out again with the index (`moov`) ahead of the samples (`mdat`), so
//! a player can start before the whole file has arrived ("fast start").
//! That costs one copy of the samples on disk, never in memory.
//!
//! [D46]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use motion_format::{Diagnostic, Kind, Result};
use mp4_atom::{
    Atom, Av01, Avc1, Co64, Colr, Decode, Dinf, Dref, Encode, Ftyp, Hdlr, Hvc1, Mdhd, Mdia, Minf,
    Moov, Mvhd, Stbl, Stco, Stsc, StscEntry, Stsd, Stss, Stsz, StszSamples, Stts, SttsEntry, Tkhd,
    Trak, Url, Visual, Vmhd,
};

use crate::video::{DecoderConfig, Muxer, Packet, VideoSpec};

/// The ticks of a frame: the track's timescale is the frame rate times
/// this, so a rate to a thousandth of a frame per second is exact.
const FRAME_TICKS: u32 = 1000;
/// An `mdat` header with a 64-bit size, so samples may pass 4 GiB.
const MDAT_HEADER: u64 = 16;

/// An MP4 file being written.
pub(crate) struct Mp4 {
    spec: VideoSpec,
    /// Where the file goes once whole.
    path: PathBuf,
    /// The samples, in order, until the file is laid out.
    scratch: BufWriter<File>,
    scratch_path: PathBuf,
    /// Each sample's size in bytes.
    sizes: Vec<u32>,
    /// The keyframes, counted from 1, as `stss` counts them.
    keyframes: Vec<u32>,
    /// The bytes of samples written so far.
    written: u64,
}

impl Mp4 {
    /// A new MP4 file at `path` for a `spec` video.
    pub(crate) fn create(path: &Path, spec: VideoSpec) -> Result<Mp4> {
        let mut scratch_path = path.as_os_str().to_owned();
        scratch_path.push(".mdat");
        let scratch_path = PathBuf::from(scratch_path);
        let scratch =
            File::create(&scratch_path).map_err(|e| Diagnostic::io("create", &scratch_path, &e))?;
        Ok(Mp4 {
            spec,
            path: path.to_owned(),
            scratch: BufWriter::new(scratch),
            scratch_path,
            sizes: Vec::new(),
            keyframes: Vec::new(),
            written: 0,
        })
    }

    /// The index of the samples, for samples that start `offset` bytes
    /// into the file.
    fn moov(&self, config: &DecoderConfig, offset: u64) -> Result<Moov> {
        let VideoSpec { width, height, fps } = self.spec;
        let count = self.sizes.len() as u32;
        let timescale = (fps * f64::from(FRAME_TICKS)).round() as u32;
        let duration = u64::from(count) * u64::from(FRAME_TICKS);
        let visual = Visual {
            data_reference_index: 1,
            width: width as u16,
            height: height as u16,
            ..Visual::default()
        };
        let codec = match config {
            DecoderConfig::Av1 { record } => Av01 {
                visual,
                av1c: record_box(b"av1C", record)?,
                colr: vec![bt709()],
                ..Av01::default()
            }
            .into(),
            DecoderConfig::Avc { record } => Avc1 {
                visual,
                avcc: record_box(b"avcC", record)?,
                colr: vec![bt709()],
                ..Avc1::default()
            }
            .into(),
            DecoderConfig::Hevc { record } => Hvc1 {
                visual,
                hvcc: record_box(b"hvcC", record)?,
                colr: vec![bt709()],
                ..Hvc1::default()
            }
            .into(),
        };
        // One chunk, so its start is the only offset: 64 bits only past 4 GiB.
        let (stco, co64) = match u32::try_from(offset) {
            Ok(offset) => (
                Some(Stco {
                    entries: vec![offset],
                }),
                None,
            ),
            Err(_) => (
                None,
                Some(Co64 {
                    entries: vec![offset],
                }),
            ),
        };
        Ok(Moov {
            mvhd: Mvhd {
                timescale,
                duration,
                next_track_id: 2,
                ..Mvhd::default()
            },
            trak: vec![Trak {
                tkhd: Tkhd {
                    track_id: 1,
                    duration,
                    enabled: true,
                    in_movie: true,
                    width: (width as u16).into(),
                    height: (height as u16).into(),
                    ..Tkhd::default()
                },
                mdia: Mdia {
                    mdhd: Mdhd {
                        timescale,
                        duration,
                        language: "und".into(),
                        ..Mdhd::default()
                    },
                    hdlr: Hdlr {
                        handler: b"vide".into(),
                        name: "motion".into(),
                    },
                    minf: Minf {
                        vmhd: Some(Vmhd::default()),
                        dinf: Dinf {
                            dref: Dref {
                                entries: vec![Url::default().into()],
                            },
                        },
                        stbl: Stbl {
                            stsd: Stsd {
                                codecs: vec![codec],
                            },
                            stts: Stts {
                                entries: vec![SttsEntry {
                                    sample_count: count,
                                    sample_delta: FRAME_TICKS,
                                }],
                            },
                            stss: Some(Stss {
                                entries: self.keyframes.clone(),
                            }),
                            // Every sample in one chunk, in order.
                            stsc: Stsc {
                                entries: vec![StscEntry {
                                    first_chunk: 1,
                                    samples_per_chunk: count,
                                    sample_description_index: 1,
                                }],
                            },
                            stsz: Stsz {
                                samples: StszSamples::Different {
                                    sizes: self.sizes.clone(),
                                },
                            },
                            stco,
                            co64,
                            ..Stbl::default()
                        },
                        ..Minf::default()
                    },
                },
                ..Trak::default()
            }],
            ..Moov::default()
        })
    }

    /// Lays the file out at `self.path`: `ftyp`, `moov`, then the samples
    /// copied from the scratch file.
    fn lay_out(&self, config: &DecoderConfig) -> Result<()> {
        let mut head = Vec::new();
        encode(&ftyp(config), &mut head)?;
        // The index holds the samples' offset, which follows the index:
        // measure it once, then write it with the offset it gives.
        let mut moov = Vec::new();
        encode(&self.moov(config, 0)?, &mut moov)?;
        let offset = (head.len() + moov.len()) as u64 + MDAT_HEADER;
        moov.clear();
        encode(&self.moov(config, offset)?, &mut moov)?;
        head.extend_from_slice(&moov);
        head.extend_from_slice(&1u32.to_be_bytes());
        head.extend_from_slice(b"mdat");
        head.extend_from_slice(&(MDAT_HEADER + self.written).to_be_bytes());

        let io = |e: io::Error| Diagnostic::io("write", &self.path, &e);
        let mut out = BufWriter::new(File::create(&self.path).map_err(io)?);
        out.write_all(&head).map_err(io)?;
        let mut samples = File::open(&self.scratch_path)
            .map_err(|e| Diagnostic::io("read", &self.scratch_path, &e))?;
        io::copy(&mut samples, &mut out).map_err(io)?;
        out.into_inner()
            .map_err(|e| io(e.into_error()))?
            .sync_all()
            .map_err(io)
    }
}

impl Muxer for Mp4 {
    fn write(&mut self, packet: &Packet) -> Result<()> {
        let next = self.sizes.len() as u64;
        if packet.frame != next {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "frame {} came to the MP4 file after frame {}",
                    packet.frame,
                    next.wrapping_sub(1)
                ),
            )
            .expected(format!("frame {next}: packets in frame order")));
        }
        let size = u32::try_from(packet.data.len()).map_err(|_| {
            Diagnostic::new(
                Kind::Invalid,
                format!("a frame of {} bytes", packet.data.len()),
            )
            .expected("a compressed frame under 4 GiB")
        })?;
        self.scratch
            .write_all(&packet.data)
            .map_err(|e| Diagnostic::io("write", &self.scratch_path, &e))?;
        self.sizes.push(size);
        if packet.keyframe {
            self.keyframes.push(self.sizes.len() as u32);
        }
        self.written += u64::from(size);
        Ok(())
    }

    fn finish(mut self: Box<Self>, config: &DecoderConfig) -> Result<()> {
        self.scratch
            .flush()
            .map_err(|e| Diagnostic::io("write", &self.scratch_path, &e))?;
        let laid_out = self.lay_out(config);
        // The scratch file is ours and of no use now, whatever happened.
        let removed = fs::remove_file(&self.scratch_path);
        laid_out?;
        removed.map_err(|e| Diagnostic::io("remove", &self.scratch_path, &e))
    }
}

/// The file type: ISO base media, with the codec's brand where it has one.
fn ftyp(config: &DecoderConfig) -> Ftyp {
    let brand: Option<&[u8; 4]> = match config {
        DecoderConfig::Av1 { .. } => Some(b"av01"),
        DecoderConfig::Avc { .. } => Some(b"avc1"),
        DecoderConfig::Hevc { .. } => None,
    };
    let brands = [b"isom", b"iso2", b"mp41"].into_iter().chain(brand);
    Ftyp {
        major_brand: b"isom".into(),
        minor_version: 512,
        compatible_brands: brands.map(|&brand| brand.into()).collect(),
    }
}

/// The colour the planes are in ([`crate::colour`]): BT.709 primaries,
/// transfer and matrix, limited range.
fn bt709() -> Colr {
    Colr::Nclx {
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        full_range_flag: false,
    }
}

/// The box `kind` (`av1C`, `avcC`, `hvcC`) of a codec's configuration
/// record, as the encoder gave it.
fn record_box<A: Atom>(kind: &[u8; 4], record: &[u8]) -> Result<A> {
    let mut boxed = Vec::with_capacity(record.len() + 8);
    boxed.extend_from_slice(&(record.len() as u32 + 8).to_be_bytes());
    boxed.extend_from_slice(kind);
    boxed.extend_from_slice(record);
    A::decode(&mut boxed.as_slice()).map_err(|e| {
        Diagnostic::new(
            Kind::Invalid,
            format!(
                "the encoder's `{}` configuration does not read: {e}",
                String::from_utf8_lossy(kind)
            ),
        )
    })
}

/// Encodes `atom` onto `buf`.
fn encode(atom: &impl Encode, buf: &mut Vec<u8>) -> Result<()> {
    atom.encode(buf)
        .map_err(|e| Diagnostic::new(Kind::Invalid, format!("an MP4 box does not encode: {e}")))
}

#[cfg(test)]
#[path = "../tests/unit/mp4.rs"]
mod tests;
