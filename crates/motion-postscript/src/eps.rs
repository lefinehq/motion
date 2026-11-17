//! The EPS container: where the PostScript program is, and the box it
//! declares.
//!
//! A plain EPS file is the program itself. A binary EPS file starts with a
//! 30-byte header (the bytes `C5 D0 D3 C6`, then little-endian offsets and
//! lengths) that says where the program lies among preview pictures; the
//! previews are never read. The box comes from the document-structure
//! comments at the head of the program, `%%HiResBoundingBox` before
//! `%%BoundingBox`, or from the trailer when the head defers it with
//! `(atend)`.

use kurbo::Rect;

use crate::Error;

/// The program inside an EPS file.
pub(crate) fn program(bytes: &[u8]) -> Result<&[u8], Error> {
    if bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        let word = |at: usize| -> Option<usize> {
            let b = bytes.get(at..at + 4)?;
            usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok()
        };
        let (Some(start), Some(len)) = (word(4), word(8)) else {
            return Err(Error::Truncated);
        };
        let end = start.checked_add(len).ok_or(Error::Truncated)?;
        let program = bytes.get(start..end).ok_or(Error::Truncated)?;
        if !program.starts_with(b"%!") {
            return Err(Error::NotPostScript);
        }
        return Ok(program);
    }
    if bytes.starts_with(b"%!") {
        return Ok(bytes);
    }
    Err(Error::NotPostScript)
}

/// The four numbers after a structure comment's colon.
fn four_numbers(rest: &[u8]) -> Option<[f64; 4]> {
    let text = std::str::from_utf8(rest).ok()?;
    let mut numbers = text.split_whitespace().map(str::parse::<f64>);
    let mut out = [0.0; 4];
    for slot in &mut out {
        *slot = numbers.next()?.ok()?;
    }
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// The lines of `program`, whichever line ends it uses.
fn lines(program: &[u8]) -> impl Iterator<Item = &[u8]> {
    program.split(|&b| b == b'\n' || b == b'\r')
}

/// The boxes the structure comments declare.
#[derive(Default)]
struct Boxes {
    high: Option<[f64; 4]>,
    low: Option<[f64; 4]>,
    /// Whether a box was deferred to the trailer with `(atend)`.
    deferred: bool,
}

impl Boxes {
    fn read(&mut self, line: &[u8]) {
        let (slot, rest) = if let Some(rest) = line.strip_prefix(b"%%HiResBoundingBox:") {
            (&mut self.high, rest)
        } else if let Some(rest) = line.strip_prefix(b"%%BoundingBox:") {
            (&mut self.low, rest)
        } else {
            return;
        };
        match four_numbers(rest) {
            Some(b) => *slot = Some(b),
            None => self.deferred = true,
        }
    }
}

/// The box the program declares, in default user space.
pub(crate) fn bounding_box(program: &[u8]) -> Result<Declared, Error> {
    let mut boxes = Boxes::default();
    // The header: structure comments up to the first line that is not one.
    for line in lines(program) {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(b"%%EndComments") || !line.starts_with(b"%") {
            break;
        }
        boxes.read(line);
    }
    if boxes.deferred {
        // `(atend)`: the trailer holds the box.
        if let Some(at) = find_last(program, b"%%Trailer") {
            for line in lines(&program[at..]) {
                boxes.read(line);
            }
        }
    }
    let usable = |b: Option<[f64; 4]>| {
        b.map(|[x0, y0, x1, y1]| Rect::new(x0, y0, x1, y1).abs())
            .filter(|r| r.width() > 0.0 && r.height() > 0.0)
    };
    let (low, high) = (usable(boxes.low), usable(boxes.high));
    let page = low.or(high).ok_or(Error::NoBoundingBox)?;
    Ok(Declared { page, art: high })
}

/// The boxes a program declares, in default user space.
pub(crate) struct Declared {
    /// The page: the whole-point box, or the precise one when only that
    /// is given.
    pub page: Rect,
    /// The precise box, `%%HiResBoundingBox`, when there is one.
    pub art: Option<Rect>,
}

/// The last offset of `needle` in `data`.
fn find_last(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).rposition(|w| w == needle)
}
