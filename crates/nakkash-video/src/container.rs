//! The ISO base media file format (`.mp4`) and QuickTime (`.mov`), which
//! share their structure: a tree of boxes, each a size and a four-letter
//! type. Only what finding a video track's frames takes is read: the first
//! video track's sample description and sample table, its timescale, and
//! the edit list that places it on the movie's timeline.
//!
//! Sample data stays in the file; a sample is where it starts and how long
//! it is.

use std::io::{Read, Seek, SeekFrom};

use nakkash_format::{Diagnostic, Kind, Result};

/// A four-letter box or codec type.
pub type FourCc = [u8; 4];

/// The first video track of a clip, as its sample table describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// The codec of its samples, from the sample description.
    pub codec: FourCc,
    /// Frame size in pixels, from the sample description.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Bits per pixel the sample description gives (24 or 32 for RGB and
    /// ARGB QuickTime Animation).
    pub depth: u16,
    /// The boxes after the sample description's fixed fields, such as the
    /// decoder configuration of a codec that needs one.
    pub extensions: Vec<(FourCc, Vec<u8>)>,
    /// Ticks per second of the track's times.
    pub timescale: u32,
    /// Every sample in decode order.
    pub samples: Vec<Sample>,
    /// Where the track's media time 0 falls on the clip's own time, in
    /// track ticks: its edit list's leading empty edits, less the media time
    /// its first edit starts at; without an edit list, less the time its
    /// first frame shows at.
    pub shift: i64,
}

/// One coded frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// Where its bytes start in the file.
    pub offset: u64,
    /// How many bytes it has.
    pub size: u32,
    /// When it is decoded, in track ticks.
    pub decode: i64,
    /// When it shows, in track ticks: its decode time and its composition
    /// offset.
    pub show: i64,
    /// How long it shows, in track ticks.
    pub duration: u32,
    /// Whether decoding can start at it.
    pub sync: bool,
}

impl Track {
    /// The length of the track in seconds: from the start of its first
    /// shown frame to the end of its last, on the clip's own time.
    pub fn seconds(&self) -> f64 {
        let end = self
            .samples
            .iter()
            .map(|s| s.show + i64::from(s.duration))
            .max()
            .unwrap_or(0);
        (end + self.shift).max(0) as f64 / f64::from(self.timescale)
    }

    /// The sync sample at or before sample `i`.
    pub fn sync_before(&self, i: usize) -> usize {
        (0..=i).rev().find(|&j| self.samples[j].sync).unwrap_or(0)
    }

    /// Where decoding sample `i` starts: the last sync sample at or before
    /// it after which nothing up to `i` shows before it. A picture decoded
    /// after a sync sample but shown before it (a leading picture of an
    /// open group of pictures) may refer to pictures before that sync
    /// sample, so decoding through it starts from an earlier one.
    pub fn start_for(&self, i: usize) -> usize {
        // The earliest time shown by the samples after `j` up to `i`.
        let mut earliest = self.samples[i].show;
        for j in (0..=i).rev() {
            let sample = self.samples[j];
            if sample.sync && sample.show <= earliest {
                return j;
            }
            earliest = earliest.min(sample.show);
        }
        0
    }
}

/// Reads the first video track of the clip in `file`.
///
/// # Errors
/// An invalid-value diagnostic when the file has no movie box, no video
/// track, or a box that runs past its parent.
pub fn read<F: Read + Seek>(file: &mut F) -> Result<Track> {
    let end = file.seek(SeekFrom::End(0)).map_err(io)?;
    let mut at = 0;
    while at < end {
        let (kind, start, size) = header(file, at, end)?;
        if &kind == b"moov" {
            let mut body = vec![0; usize::try_from(size).map_err(|_| too_big())?];
            file.seek(SeekFrom::Start(start)).map_err(io)?;
            file.read_exact(&mut body).map_err(io)?;
            return movie(&body);
        }
        at = start + size;
    }
    Err(invalid("no movie box (`moov`)"))
}

/// A box header at `at` in a file of `end` bytes: its type, where its body
/// starts and how long the body is.
fn header<F: Read + Seek>(file: &mut F, at: u64, end: u64) -> Result<(FourCc, u64, u64)> {
    let mut head = [0; 8];
    file.seek(SeekFrom::Start(at)).map_err(io)?;
    file.read_exact(&mut head).map_err(io)?;
    let kind = [head[4], head[5], head[6], head[7]];
    let (body, size) = match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
        0 => (at + 8, end - at),
        1 => {
            let mut large = [0; 8];
            file.read_exact(&mut large).map_err(io)?;
            (at + 16, u64::from_be_bytes(large))
        }
        n => (at + 8, u64::from(n)),
    };
    let header = body - at;
    if size < header || at + size > end {
        return Err(invalid(format!(
            "the `{}` box at byte {at} runs past the end of the file",
            name(kind)
        )));
    }
    Ok((kind, body, size - header))
}

/// The boxes directly inside `bytes`, each with its type and body.
fn boxes(bytes: &[u8]) -> Result<Vec<(FourCc, &[u8])>> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while rest.len() >= 8 {
        let size = be32(rest, 0)?;
        let kind = [rest[4], rest[5], rest[6], rest[7]];
        let (header, size) = match size {
            0 => (8, rest.len()),
            1 => (16, usize::try_from(be64(rest, 8)?).map_err(|_| too_big())?),
            n => (8, usize::try_from(n).map_err(|_| too_big())?),
        };
        if size < header || size > rest.len() {
            return Err(invalid(format!(
                "the `{}` box runs past the box around it",
                name(kind)
            )));
        }
        out.push((kind, &rest[header..size]));
        rest = &rest[size..];
    }
    Ok(out)
}

/// The first box of type `kind` directly inside `bytes`.
fn child<'b>(bytes: &'b [u8], kind: &FourCc) -> Result<Option<&'b [u8]>> {
    Ok(boxes(bytes)?
        .into_iter()
        .find(|(k, _)| k == kind)
        .map(|(_, body)| body))
}

/// The box at the end of `path` under `bytes`.
fn find<'b>(bytes: &'b [u8], path: &[&FourCc]) -> Result<Option<&'b [u8]>> {
    let mut at = bytes;
    for kind in path {
        match child(at, kind)? {
            Some(body) => at = body,
            None => return Ok(None),
        }
    }
    Ok(Some(at))
}

/// The movie box: its timescale, and the first track whose handler is video.
fn movie(moov: &[u8]) -> Result<Track> {
    let movie_scale = match child(moov, b"mvhd")? {
        Some(mvhd) => timescale(mvhd)?,
        None => return Err(invalid("no movie header (`mvhd`)")),
    };
    for (kind, trak) in boxes(moov)? {
        if &kind != b"trak" {
            continue;
        }
        let handler = find(trak, &[b"mdia", b"hdlr"])?;
        if handler.is_some_and(|h| h.get(8..12) == Some(b"vide")) {
            return track(trak, movie_scale);
        }
    }
    Err(invalid("no video track"))
}

/// The timescale of a movie or media header, either version.
fn timescale(header: &[u8]) -> Result<u32> {
    match header.first() {
        Some(1) => be32(header, 20),
        _ => be32(header, 12),
    }
}

fn track(trak: &[u8], movie_scale: u32) -> Result<Track> {
    let mdhd = find(trak, &[b"mdia", b"mdhd"])?.ok_or_else(|| invalid("no media header"))?;
    let scale = timescale(mdhd)?;
    if scale == 0 {
        return Err(invalid("a track whose timescale is 0"));
    }
    let stbl = find(trak, &[b"mdia", b"minf", b"stbl"])?
        .ok_or_else(|| invalid("no sample table (`stbl`)"))?;
    let need = |kind: &FourCc| -> Result<&[u8]> {
        child(stbl, kind)?
            .ok_or_else(|| invalid(format!("no `{}` in the sample table", name(*kind))))
    };
    let description = description(need(b"stsd")?)?;
    let sizes = match child(stbl, b"stsz")? {
        Some(stsz) => sample_sizes(stsz)?,
        None => compact_sizes(need(b"stz2")?)?,
    };
    let offsets = chunk_offsets(stbl)?;
    let places = places(need(b"stsc")?, &offsets, &sizes)?;
    let decode = runs(need(b"stts")?, sizes.len())?;
    let shows = match child(stbl, b"ctts")? {
        Some(ctts) => runs_signed(ctts, sizes.len())?,
        None => vec![0; sizes.len()],
    };
    let sync = match child(stbl, b"stss")? {
        Some(stss) => Some(numbers(stss)?),
        None => None,
    };
    let mut time = 0i64;
    let samples: Vec<Sample> = (0..sizes.len())
        .map(|i| {
            let start = time;
            time += i64::from(decode[i]);
            Sample {
                offset: places[i],
                size: sizes[i],
                decode: start,
                show: start + shows[i],
                duration: decode[i],
                sync: sync
                    .as_ref()
                    .is_none_or(|s| s.binary_search(&(i as u32 + 1)).is_ok()),
            }
        })
        .collect();
    Ok(Track {
        codec: description.codec,
        width: description.width,
        height: description.height,
        depth: description.depth,
        extensions: description.extensions,
        timescale: scale,
        shift: shift(trak, movie_scale, scale)?
            .unwrap_or_else(|| -samples.iter().map(|s| s.show).min().unwrap_or(0)),
        samples,
    })
}

/// A video sample description's codec, size, depth and extension boxes.
struct Description {
    codec: FourCc,
    width: u32,
    height: u32,
    depth: u16,
    extensions: Vec<(FourCc, Vec<u8>)>,
}

/// The first entry of `stsd`.
fn description(stsd: &[u8]) -> Result<Description> {
    // Version and flags, then the entry count, then the entries.
    let entries = stsd
        .get(8..)
        .ok_or_else(|| invalid("an empty sample description"))?;
    let (codec, entry) = boxes(entries)?
        .into_iter()
        .next()
        .ok_or_else(|| invalid("a sample description with no entry"))?;
    // Six reserved bytes and a data reference index, then the video fields.
    let field = |at: usize| be16(entry, at);
    let width = u32::from(field(24)?);
    let height = u32::from(field(26)?);
    let depth = field(74)?;
    let extensions = boxes(entry.get(78..).unwrap_or(&[]))
        .unwrap_or_default()
        .into_iter()
        .map(|(kind, body)| (kind, body.to_vec()))
        .collect();
    Ok(Description {
        codec,
        width,
        height,
        depth,
        extensions,
    })
}

/// Sample sizes from `stsz`: one size for all, or one each.
fn sample_sizes(stsz: &[u8]) -> Result<Vec<u32>> {
    let fixed = be32(stsz, 4)?;
    let count = count(stsz, 8)?;
    match fixed {
        0 => (0..count).map(|i| be32(stsz, 12 + 4 * i)).collect(),
        size => Ok(vec![size; count]),
    }
}

/// Sample sizes from `stz2`, packed in 4, 8 or 16 bits.
fn compact_sizes(stz2: &[u8]) -> Result<Vec<u32>> {
    let bits = *stz2.get(7).ok_or_else(truncated)?;
    let count = count(stz2, 8)?;
    (0..count)
        .map(|i| match bits {
            4 => {
                let byte = *stz2.get(12 + i / 2).ok_or_else(truncated)?;
                Ok(u32::from(if i % 2 == 0 { byte >> 4 } else { byte & 15 }))
            }
            8 => stz2
                .get(12 + i)
                .map(|&b| u32::from(b))
                .ok_or_else(truncated),
            16 => be16(stz2, 12 + 2 * i).map(u32::from),
            other => Err(invalid(format!("`stz2` sizes of {other} bits"))),
        })
        .collect()
}

/// Chunk offsets from `stco` (32-bit) or `co64` (64-bit).
fn chunk_offsets(stbl: &[u8]) -> Result<Vec<u64>> {
    if let Some(stco) = child(stbl, b"stco")? {
        let n = count(stco, 4)?;
        return (0..n)
            .map(|i| be32(stco, 8 + 4 * i).map(u64::from))
            .collect();
    }
    let co64 = child(stbl, b"co64")?.ok_or_else(|| invalid("no chunk offsets"))?;
    let n = count(co64, 4)?;
    (0..n).map(|i| be64(co64, 8 + 8 * i)).collect()
}

/// Where each sample starts: chunks hold runs of samples back to back, and
/// `stsc` says how many samples each chunk holds from a chunk on.
fn places(stsc: &[u8], chunks: &[u64], sizes: &[u32]) -> Result<Vec<u64>> {
    let n = count(stsc, 4)?;
    let runs: Vec<(usize, usize)> = (0..n)
        .map(|i| {
            let first = be32(stsc, 8 + 12 * i)? as usize;
            let per = be32(stsc, 12 + 12 * i)? as usize;
            Ok((first.max(1) - 1, per))
        })
        .collect::<Result<_>>()?;
    let mut places = Vec::with_capacity(sizes.len());
    let mut sample = 0;
    for (chunk, &start) in chunks.iter().enumerate() {
        let per = runs
            .iter()
            .rev()
            .find(|(first, _)| *first <= chunk)
            .map_or(0, |(_, per)| *per);
        let mut at = start;
        for _ in 0..per {
            let Some(&size) = sizes.get(sample) else {
                break;
            };
            places.push(at);
            at += u64::from(size);
            sample += 1;
        }
    }
    if places.len() != sizes.len() {
        return Err(invalid(format!(
            "the chunks hold {} samples, and the sizes give {}",
            places.len(),
            sizes.len()
        )));
    }
    Ok(places)
}

/// Durations from `stts`, one per sample.
fn runs(stts: &[u8], samples: usize) -> Result<Vec<u32>> {
    let n = count(stts, 4)?;
    let mut out = Vec::with_capacity(samples);
    for i in 0..n {
        let (times, delta) = (be32(stts, 8 + 8 * i)?, be32(stts, 12 + 8 * i)?);
        out.extend(std::iter::repeat_n(delta, times as usize));
    }
    out.resize(samples, out.last().copied().unwrap_or(0));
    Ok(out)
}

/// Composition offsets from `ctts`, one per sample, read as signed as both
/// versions are in practice.
fn runs_signed(ctts: &[u8], samples: usize) -> Result<Vec<i64>> {
    let n = count(ctts, 4)?;
    let mut out = Vec::with_capacity(samples);
    for i in 0..n {
        let times = be32(ctts, 8 + 8 * i)?;
        let offset = be32(ctts, 12 + 8 * i)? as i32;
        out.extend(std::iter::repeat_n(i64::from(offset), times as usize));
    }
    out.resize(samples, 0);
    Ok(out)
}

/// The numbers of `stss`, 1-based sample numbers in rising order.
fn numbers(stss: &[u8]) -> Result<Vec<u32>> {
    let n = count(stss, 4)?;
    (0..n).map(|i| be32(stss, 8 + 4 * i)).collect()
}

/// The track's media time 0 on the clip's own time, in track ticks, from
/// its edit list: leading empty edits delay it, and the first edit with
/// media skips the media before its start. `None` without an edit list
/// (or with nothing but empty edits): the track then starts at its first
/// frame, as players start one whose composition offsets are all above 0.
fn shift(trak: &[u8], movie_scale: u32, scale: u32) -> Result<Option<i64>> {
    let Some(elst) = find(trak, &[b"edts", b"elst"])? else {
        return Ok(None);
    };
    let wide = elst.first() == Some(&1);
    let n = count(elst, 4)?;
    let (entry, at) = if wide { (20, 8) } else { (12, 8) };
    let mut delay = 0i64;
    for i in 0..n {
        let base = at + entry * i;
        let (duration, media) = match wide {
            true => (be64(elst, base)? as i64, be64(elst, base + 8)? as i64),
            false => (
                i64::from(be32(elst, base)?),
                i64::from(be32(elst, base + 4)? as i32),
            ),
        };
        if media == -1 {
            delay += duration;
            continue;
        }
        let delay = delay * i64::from(scale) / i64::from(movie_scale.max(1));
        return Ok(Some(delay - media));
    }
    Ok(None)
}

fn be16(bytes: &[u8], at: usize) -> Result<u16> {
    bytes
        .get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(truncated)
}

fn be32(bytes: &[u8], at: usize) -> Result<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(truncated)
}

fn be64(bytes: &[u8], at: usize) -> Result<u64> {
    bytes
        .get(at..at + 8)
        .map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        .ok_or_else(truncated)
}

/// An entry count at `at`, which must fit the box.
fn count(bytes: &[u8], at: usize) -> Result<usize> {
    let n = be32(bytes, at)? as usize;
    match n <= bytes.len() {
        true => Ok(n),
        false => Err(invalid(format!(
            "a box that counts {n} entries in {} bytes",
            bytes.len()
        ))),
    }
}

/// A four-letter type as text.
pub fn name(kind: FourCc) -> String {
    kind.iter().map(|&b| char::from(b)).collect()
}

fn invalid(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, message.into()).expected("an MP4 or QuickTime clip")
}

fn truncated() -> Diagnostic {
    invalid("a box shorter than its fields")
}

fn too_big() -> Diagnostic {
    invalid("a box too large for this machine")
}

fn io(e: std::io::Error) -> Diagnostic {
    Diagnostic::new(Kind::Io, format!("reading a clip: {e}"))
}

#[cfg(test)]
#[path = "../tests/unit/container.rs"]
mod tests;
