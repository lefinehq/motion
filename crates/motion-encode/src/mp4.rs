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
    Atom, Audio, Av01, Avc1, Co64, Colr, Decode, Dinf, Dref, Edts, Elst, ElstEntry, Encode, Esds,
    FixedPoint, Ftyp, Hdlr, Hvc1, Jpeg, Mdhd, Mdia, Minf, Moov, Mp4a, Mvhd, Smhd, Sowt, Stbl, Stco,
    Stsc, StscEntry, Stsd, Stss, Stsz, StszSamples, Stts, SttsEntry, Tkhd, Trak, Url, Visual, Vmhd,
    esds,
};

use crate::sound::{AAC_FRAMES, EncodedSound, frequency_index};
use crate::video::{Container, DecoderConfig, Muxer, Packet, VideoSpec};

/// The ticks of a frame: the track's timescale is the frame rate times
/// this, so a rate to a thousandth of a frame per second is exact.
const FRAME_TICKS: u32 = 1000;
/// An `mdat` header with a 64-bit size, so samples may pass 4 GiB.
const MDAT_HEADER: u64 = 16;

/// An MP4 file being written.
pub(crate) struct Mp4 {
    spec: VideoSpec,
    /// MP4, or QuickTime: the same boxes under another file type.
    container: Container,
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
    pub(crate) fn create(path: &Path, spec: VideoSpec, container: Container) -> Result<Mp4> {
        let mut scratch_path = path.as_os_str().to_owned();
        scratch_path.push(".mdat");
        let scratch_path = PathBuf::from(scratch_path);
        let scratch =
            File::create(&scratch_path).map_err(|e| Diagnostic::io("create", &scratch_path, &e))?;
        Ok(Mp4 {
            spec,
            container,
            path: path.to_owned(),
            scratch: BufWriter::new(scratch),
            scratch_path,
            sizes: Vec::new(),
            keyframes: Vec::new(),
            written: 0,
        })
    }

    /// The index of the samples, for samples that start `offset` bytes
    /// into the file: the video's, then the sound's after them.
    fn moov(
        &self,
        config: &DecoderConfig,
        sound: Option<&EncodedSound>,
        offset: u64,
    ) -> Result<Moov> {
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
            DecoderConfig::ProRes => prores(visual),
            DecoderConfig::Hevc { record } => Hvc1 {
                visual,
                hvcc: record_box(b"hvcC", record)?,
                colr: vec![bt709()],
                ..Hvc1::default()
            }
            .into(),
        };
        let (stco, co64) = chunk_offset(offset);
        let mut trak = vec![Trak {
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
        }];
        let mut movie = duration;
        if let Some(sound) = sound {
            let heard = sound_trak(sound, timescale, offset + self.written)?;
            movie = movie.max(heard.tkhd.duration);
            trak.push(heard);
        }
        Ok(Moov {
            mvhd: Mvhd {
                timescale,
                duration: movie,
                next_track_id: trak.len() as u32 + 1,
                ..Mvhd::default()
            },
            trak,
            ..Moov::default()
        })
    }

    /// Lays the file out at `self.path`: `ftyp`, `moov`, then the samples
    /// copied from the scratch file.
    fn lay_out(&self, config: &DecoderConfig, sound: Option<&EncodedSound>) -> Result<()> {
        let mut head = Vec::new();
        encode(&ftyp(self.container, config), &mut head)?;
        // The index holds the samples' offset, which follows the index:
        // measure it, then write it with the offset it gives. An offset
        // that passes 4 GiB takes 64 bits and lengthens the index, so
        // measure again until the length holds.
        let heard = sound.map_or_else(Vec::new, sound_bytes);
        let mut moov = Vec::new();
        let mut offset = 0;
        loop {
            moov.clear();
            encode(&self.moov(config, sound, offset)?, &mut moov)?;
            let start = (head.len() + moov.len()) as u64 + MDAT_HEADER;
            if start == offset {
                break;
            }
            offset = start;
        }
        if *config == DecoderConfig::ProRes && !rename_entry(&mut moov, b"jpeg", b"ap4h") {
            return Err(Diagnostic::new(
                Kind::Invalid,
                "the ProRes sample entry is not where the index puts it",
            ));
        }
        head.extend_from_slice(&moov);
        head.extend_from_slice(&1u32.to_be_bytes());
        head.extend_from_slice(b"mdat");
        let samples = self.written + heard.len() as u64;
        head.extend_from_slice(&(MDAT_HEADER + samples).to_be_bytes());

        let io = |e: io::Error| Diagnostic::io("write", &self.path, &e);
        let mut out = BufWriter::new(File::create(&self.path).map_err(io)?);
        out.write_all(&head).map_err(io)?;
        let mut samples = File::open(&self.scratch_path)
            .map_err(|e| Diagnostic::io("read", &self.scratch_path, &e))?;
        io::copy(&mut samples, &mut out).map_err(io)?;
        out.write_all(&heard).map_err(io)?;
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

    fn finish(
        mut self: Box<Self>,
        config: &DecoderConfig,
        sound: Option<&EncodedSound>,
    ) -> Result<()> {
        self.scratch
            .flush()
            .map_err(|e| Diagnostic::io("write", &self.scratch_path, &e))?;
        let laid_out = self.lay_out(config, sound);
        // The scratch file is ours and of no use now, whatever happened.
        let removed = fs::remove_file(&self.scratch_path);
        laid_out?;
        removed.map_err(|e| Diagnostic::io("remove", &self.scratch_path, &e))
    }
}

/// The sample table's chunk offset for one chunk at `offset`: 32 bits, or
/// 64 past 4 GiB.
fn chunk_offset(offset: u64) -> (Option<Stco>, Option<Co64>) {
    match u32::try_from(offset) {
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
    }
}

/// The bytes of `sound`'s samples, as they follow the video's.
fn sound_bytes(sound: &EncodedSound) -> Vec<u8> {
    match sound {
        EncodedSound::Aac { packets, .. } => packets.concat(),
        EncodedSound::Pcm16 { data, .. } => data.clone(),
    }
}

/// The sound's track: its samples in one chunk at `offset`, timed in its
/// own rate, its length given in the movie's `timescale`. AAC's priming is
/// skipped by an edit, so the sound starts with the first frame.
fn sound_trak(sound: &EncodedSound, timescale: u32, offset: u64) -> Result<Trak> {
    let rate = sound.rate();
    let heard = (sound.frames() * u64::from(timescale)).div_ceil(u64::from(rate));
    let audio = |channels: u16| Audio {
        data_reference_index: 1,
        channel_count: channels,
        sample_size: 16,
        sample_rate: FixedPoint::new(rate as u16, 0),
    };
    let (codec, stts, stsz, media, edts) = match sound {
        EncodedSound::Aac {
            packets,
            leading,
            channels,
            bitrate,
            ..
        } => {
            let freq_index = frequency_index(rate).ok_or_else(|| {
                Diagnostic::new(Kind::OutOfRange, format!("AAC at {rate} Hz"))
                    .expected("a rate AAC takes, such as 48000 Hz")
            })?;
            let largest = packets.iter().map(Vec::len).max().unwrap_or(0) as u32;
            let codec = Mp4a {
                audio: audio(*channels),
                esds: Esds {
                    es_desc: esds::EsDescriptor {
                        es_id: 2,
                        dec_config: esds::DecoderConfig {
                            // MPEG-4 audio, an audio stream.
                            object_type_indication: 0x40,
                            stream_type: 0x05,
                            buffer_size_db: [
                                (largest >> 16) as u8,
                                (largest >> 8) as u8,
                                largest as u8,
                            ]
                            .into(),
                            max_bitrate: *bitrate,
                            avg_bitrate: *bitrate,
                            dec_specific: Some(esds::DecoderSpecific {
                                // AAC-LC.
                                profile: 2,
                                freq_index,
                                chan_conf: *channels as u8,
                                raw: Vec::new(),
                            }),
                            ..esds::DecoderConfig::default()
                        },
                        ..esds::EsDescriptor::default()
                    },
                },
                btrt: None,
                taic: None,
            }
            .into();
            let count = packets.len() as u32;
            let edts = Edts {
                elst: Some(Elst {
                    entries: vec![ElstEntry {
                        segment_duration: heard,
                        media_time: Some(u64::from(*leading)),
                        media_rate: FixedPoint::new(1, 0),
                    }],
                }),
            };
            (
                codec,
                SttsEntry {
                    sample_count: count,
                    sample_delta: AAC_FRAMES,
                },
                Stsz {
                    samples: StszSamples::Different {
                        sizes: packets.iter().map(|p| p.len() as u32).collect(),
                    },
                },
                u64::from(count) * u64::from(AAC_FRAMES),
                Some(edts),
            )
        }
        EncodedSound::Pcm16 {
            frames, channels, ..
        } => {
            let count = u32::try_from(*frames).map_err(|_| {
                Diagnostic::new(Kind::OutOfRange, format!("{frames} frames of sound"))
                    .expected("under 2^32 frames of sound")
            })?;
            let codec = Sowt {
                audio: audio(*channels),
                pcmc: None,
                chnl: None,
                btrt: None,
            }
            .into();
            (
                codec,
                SttsEntry {
                    sample_count: count,
                    sample_delta: 1,
                },
                Stsz {
                    samples: StszSamples::Identical {
                        count,
                        size: 2 * u32::from(*channels),
                    },
                },
                *frames,
                None,
            )
        }
    };
    let (stco, co64) = chunk_offset(offset);
    let samples = stts.sample_count;
    Ok(Trak {
        tkhd: Tkhd {
            track_id: 2,
            duration: heard,
            enabled: true,
            in_movie: true,
            volume: FixedPoint::new(1, 0),
            ..Tkhd::default()
        },
        edts,
        mdia: Mdia {
            mdhd: Mdhd {
                timescale: rate,
                duration: media,
                language: "und".into(),
                ..Mdhd::default()
            },
            hdlr: Hdlr {
                handler: b"soun".into(),
                name: "motion".into(),
            },
            minf: Minf {
                smhd: Some(Smhd::default()),
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
                        entries: vec![stts],
                    },
                    stsc: Stsc {
                        entries: vec![StscEntry {
                            first_chunk: 1,
                            samples_per_chunk: samples,
                            sample_description_index: 1,
                        }],
                    },
                    stsz,
                    stco,
                    co64,
                    ..Stbl::default()
                },
                ..Minf::default()
            },
        },
        ..Trak::default()
    })
}

/// The file type: QuickTime's, or ISO base media with the codec's brand
/// where it has one.
fn ftyp(container: Container, config: &DecoderConfig) -> Ftyp {
    if container == Container::Mov {
        return Ftyp {
            major_brand: b"qt  ".into(),
            minor_version: 512,
            compatible_brands: vec![b"qt  ".into()],
        };
    }
    let brand: Option<&[u8; 4]> = match config {
        DecoderConfig::Av1 { .. } => Some(b"av01"),
        DecoderConfig::Avc { .. } => Some(b"avc1"),
        DecoderConfig::Hevc { .. } | DecoderConfig::ProRes => None,
    };
    let brands = [b"isom", b"iso2", b"mp41"].into_iter().chain(brand);
    Ftyp {
        major_brand: b"isom".into(),
        minor_version: 512,
        compatible_brands: brands.map(|&brand| brand.into()).collect(),
    }
}

/// A stand-in for the sample entry of ProRes 4444 (`ap4h`), which
/// `mp4-atom` has no type for and will not write as an unknown entry:
/// QuickTime's `jpeg` entry has the same fields, the visual ones, 32 bits
/// deep for alpha, then the colour. [`rename_entry`] gives it its name once
/// encoded.
fn prores(mut visual: Visual) -> mp4_atom::Codec {
    visual.depth = 32;
    visual.compressor = "Apple ProRes 4444".into();
    Jpeg {
        visual,
        colr: vec![Colr::Nclc {
            colour_primaries: 1,
            transfer_characteristics: 1,
            matrix_coefficients: 1,
        }],
        ..Jpeg::default()
    }
    .into()
}

/// The path from `moov` to the sample description, box by box.
const STSD_PATH: [&[u8; 4]; 5] = [b"trak", b"mdia", b"minf", b"stbl", b"stsd"];

/// Renames the first sample entry of an encoded `moov` from `from` to `to`,
/// which have the same size. False when the boxes do not lead to such an
/// entry.
fn rename_entry(moov: &mut [u8], from: &[u8; 4], to: &[u8; 4]) -> bool {
    /// The first child of kind `kind` in `range` of `bytes`, as the range of
    /// its body.
    fn child(
        bytes: &[u8],
        range: std::ops::Range<usize>,
        kind: &[u8; 4],
    ) -> Option<std::ops::Range<usize>> {
        let mut at = range.start;
        while at + 8 <= range.end {
            let size = u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?) as usize;
            let (header, size) = match size {
                1 => (
                    16,
                    u64::from_be_bytes(bytes.get(at + 8..at + 16)?.try_into().ok()?) as usize,
                ),
                0 => (8, range.end - at),
                size => (8, size),
            };
            let end = at
                .checked_add(size)
                .filter(|&end| end <= range.end && size >= header)?;
            if &bytes[at + 4..at + 8] == kind {
                return Some(at + header..end);
            }
            at = end;
        }
        None
    }
    let mut body = 8..moov.len();
    for kind in STSD_PATH {
        match child(moov, body, kind) {
            Some(inner) => body = inner,
            None => return false,
        }
    }
    // The description's version, flags and entry count, then the entry.
    let kind = body.start + 8 + 4;
    match moov.get_mut(kind..kind + 4) {
        Some(name) if name == from => {
            name.copy_from_slice(to);
            true
        }
        _ => false,
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
