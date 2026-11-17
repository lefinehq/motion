//! Files that read through decoding filters, and the byte reads every file
//! operator shares.
//!
//! The decoding filters are the ones images and inline data are written
//! with: hexadecimal, base 85, run length, deflate, and the sub-file filter
//! that cuts a stretch out of the program itself. A filter pulls its input
//! one byte at a time, so it never reads past its own end of data in the
//! file under it, and the program goes on scanning from exactly there.

use std::rc::Rc;

use flate2::{Decompress, FlushDecompress, Status};

use crate::interp::{Fault, Interp, PsError, R};
use crate::object::{Object, Value};
use crate::vm::File;

/// Where a filter reads from.
#[derive(Debug)]
pub(crate) enum Source {
    File(u32),
    String {
        data: Rc<[u8]>,
        pos: usize,
    },
    /// A procedure that returns a string each call, the empty string at
    /// the end, with the string it returned last.
    Proc {
        proc: Object,
        buffer: Vec<u8>,
        pos: usize,
        ended: bool,
    },
}

/// How a filter decodes.
#[derive(Debug)]
pub(crate) enum Kind {
    AsciiHex,
    Ascii85,
    RunLength,
    /// Passes bytes through until the end-of-data string has been seen
    /// `count + 1` times, or, with an empty string, for `count` bytes (to
    /// the end of the source when the count is negative).
    SubFile {
        count: i64,
        eod: Vec<u8>,
        held: Vec<u8>,
    },
    Flate(Box<Decompress>),
}

/// A decoding filter.
#[derive(Debug)]
pub(crate) struct Filter {
    pub source: Source,
    pub kind: Kind,
    /// Decoded bytes not yet read.
    pub out: Vec<u8>,
    pub out_pos: usize,
    /// Whether the filter has reached its end of data.
    pub done: bool,
}

impl Filter {
    pub fn new(source: Source, kind: Kind) -> Self {
        Self {
            source,
            kind,
            out: Vec::new(),
            out_pos: 0,
            done: false,
        }
    }
}

impl Interp {
    /// The next byte of file `file`, or `None` at its end.
    pub fn read_byte(&mut self, file: u32) -> R<Option<u8>> {
        loop {
            match &mut self.vm.files[file as usize] {
                File::Bytes { data, pos } => {
                    let byte = data.get(*pos).copied();
                    if byte.is_some() {
                        *pos += 1;
                    }
                    return Ok(byte);
                }
                File::Filter(filter) => {
                    if filter.out_pos < filter.out.len() {
                        let byte = filter.out[filter.out_pos];
                        filter.out_pos += 1;
                        return Ok(Some(byte));
                    }
                    if filter.done {
                        return Ok(None);
                    }
                }
                File::Sink | File::Closed => return Ok(None),
            }
            // Refill with the filter taken out of its slot, so that reading
            // its source may use the interpreter freely.
            let taken = std::mem::replace(&mut self.vm.files[file as usize], File::Closed);
            let File::Filter(mut filter) = taken else {
                return Ok(None);
            };
            filter.out.clear();
            filter.out_pos = 0;
            let result = self.refill(&mut filter);
            self.vm.files[file as usize] = File::Filter(filter);
            result?;
            self.tick(1)?;
        }
    }

    /// Reads a filter on to its end of data when nothing but the end is
    /// left in it, so the marker that closes inline data (`~>`, `>`) is not
    /// left for the scanner after an image has read all it needed. Data
    /// the filter still holds stays in it, to be read later.
    pub fn settle(&mut self, file: u32) -> R {
        let File::Filter(filter) = &self.vm.files[file as usize] else {
            return Ok(());
        };
        if filter.done || filter.out_pos < filter.out.len() {
            return Ok(());
        }
        if let Some(byte) = self.read_byte(file)? {
            // Not the end: put the byte back for the next read.
            let _ = byte;
            self.unread_byte(file);
        }
        Ok(())
    }

    /// Pushes a byte back onto file `file`, as `token` does after a
    /// number that a delimiter ended.
    pub fn unread_byte(&mut self, file: u32) {
        match &mut self.vm.files[file as usize] {
            File::Bytes { pos, .. } => *pos = pos.saturating_sub(1),
            File::Filter(filter) => filter.out_pos = filter.out_pos.saturating_sub(1),
            File::Sink | File::Closed => {}
        }
    }

    /// The next byte of a filter's source.
    fn source_byte(&mut self, source: &mut Source) -> R<Option<u8>> {
        match source {
            Source::File(file) => self.read_byte(*file),
            Source::String { data, pos } => {
                let byte = data.get(*pos).copied();
                if byte.is_some() {
                    *pos += 1;
                }
                Ok(byte)
            }
            Source::Proc {
                proc,
                buffer,
                pos,
                ended,
            } => loop {
                if *pos < buffer.len() {
                    *pos += 1;
                    return Ok(Some(buffer[*pos - 1]));
                }
                if *ended {
                    return Ok(None);
                }
                self.call(*proc)?;
                let result = self.pop()?;
                let Value::String(s) = result.value else {
                    return Err(PsError::TypeCheck.into());
                };
                buffer.clear();
                buffer.extend_from_slice(self.vm.str_bytes(s));
                *pos = 0;
                *ended = buffer.is_empty();
            },
        }
    }

    /// Decodes at least one byte into `filter.out`, or marks its end.
    fn refill(&mut self, filter: &mut Filter) -> R {
        let Filter {
            source,
            kind,
            out,
            done,
            ..
        } = filter;
        match kind {
            Kind::AsciiHex => {
                let mut high: Option<u8> = None;
                while out.len() < 256 {
                    let Some(byte) = self.source_byte(source)? else {
                        *done = true;
                        break;
                    };
                    if byte == b'>' {
                        *done = true;
                        break;
                    }
                    let Some(nibble) = hex_digit(byte) else {
                        if crate::scan::is_space(byte) {
                            continue;
                        }
                        return Err(PsError::IoError.into());
                    };
                    match high.take() {
                        Some(h) => out.push((h << 4) | nibble),
                        None => high = Some(nibble),
                    }
                }
                if let Some(h) = high {
                    out.push(h << 4);
                }
            }
            Kind::Ascii85 => {
                let mut group = [0u8; 5];
                let mut n = 0;
                loop {
                    let Some(byte) = self.source_byte(source)? else {
                        *done = true;
                        break;
                    };
                    match byte {
                        b'~' => {
                            // The `>` that follows ends the data too.
                            let _ = self.source_byte(source)?;
                            *done = true;
                            break;
                        }
                        b'z' if n == 0 => {
                            out.extend_from_slice(&[0; 4]);
                        }
                        b'!'..=b'u' => {
                            group[n] = byte - b'!';
                            n += 1;
                            if n == 5 {
                                out.extend_from_slice(&base85_group(&group, 5));
                                n = 0;
                            }
                        }
                        _ if crate::scan::is_space(byte) => continue,
                        _ => return Err(PsError::IoError.into()),
                    }
                    if out.len() >= 256 {
                        break;
                    }
                }
                if *done && n > 1 {
                    out.extend_from_slice(&base85_group(&group, n));
                }
            }
            Kind::RunLength => {
                let Some(length) = self.source_byte(source)? else {
                    *done = true;
                    return Ok(());
                };
                match length {
                    128 => *done = true,
                    0..=127 => {
                        for _ in 0..=length {
                            match self.source_byte(source)? {
                                Some(byte) => out.push(byte),
                                None => {
                                    *done = true;
                                    break;
                                }
                            }
                        }
                    }
                    _ => match self.source_byte(source)? {
                        Some(byte) => {
                            out.extend(std::iter::repeat_n(byte, 257 - usize::from(length)))
                        }
                        None => *done = true,
                    },
                }
            }
            Kind::SubFile { count, eod, held } => {
                if eod.is_empty() {
                    // A negative count reads to the end of the source.
                    while *count != 0 && out.len() < 4096 {
                        match self.source_byte(source)? {
                            Some(byte) => {
                                out.push(byte);
                                *count = count.saturating_sub(i64::from(*count > 0));
                            }
                            None => {
                                *done = true;
                                break;
                            }
                        }
                    }
                    if *count == 0 || out.is_empty() {
                        *done = true;
                    }
                    return Ok(());
                }
                while out.len() < 4096 {
                    let Some(byte) = self.source_byte(source)? else {
                        out.append(held);
                        *done = true;
                        break;
                    };
                    held.push(byte);
                    // Hand out the bytes that can no longer start a match.
                    while !held.is_empty() && !eod.starts_with(held) {
                        out.push(held.remove(0));
                    }
                    if held.len() == eod.len() {
                        if *count <= 0 {
                            held.clear();
                            *done = true;
                            break;
                        }
                        *count -= 1;
                        out.append(held);
                    }
                }
            }
            Kind::Flate(inflate) => {
                let mut chunk = [0u8; 4096];
                while out.is_empty() {
                    let Some(byte) = self.source_byte(source)? else {
                        *done = true;
                        break;
                    };
                    let before = inflate.total_out();
                    let status = inflate
                        .decompress(&[byte], &mut chunk, FlushDecompress::None)
                        .map_err(|_| Fault::Error(PsError::IoError))?;
                    let made = usize::try_from(inflate.total_out() - before).unwrap_or(0);
                    out.extend_from_slice(&chunk[..made]);
                    if status == Status::StreamEnd {
                        *done = true;
                        break;
                    }
                }
            }
        }
        if out.is_empty() {
            *done = true;
        }
        Ok(())
    }
}

/// The value of a hexadecimal digit.
const fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The bytes of a base-85 group of `n` digits (5 for a whole group).
fn base85_group(digits: &[u8; 5], n: usize) -> Vec<u8> {
    let mut padded = *digits;
    for digit in padded.iter_mut().skip(n) {
        *digit = 84;
    }
    let value = padded
        .iter()
        .fold(0u64, |acc, &digit| acc * 85 + u64::from(digit));
    #[allow(clippy::cast_possible_truncation)]
    let bytes = (value as u32).to_be_bytes();
    bytes[..n - 1].to_vec()
}

/// The bytes a whole hexadecimal string spells, white space ignored and an
/// odd final digit padded with zero.
pub(crate) fn hex_all(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut high: Option<u8> = None;
    for &byte in text {
        if crate::scan::is_space(byte) {
            continue;
        }
        let nibble = hex_digit(byte)?;
        match high.take() {
            Some(h) => out.push((h << 4) | nibble),
            None => high = Some(nibble),
        }
    }
    if let Some(h) = high {
        out.push(h << 4);
    }
    Some(out)
}

/// The bytes a whole base-85 string spells.
pub(crate) fn ascii85_all(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 4 / 5);
    let mut group = [0u8; 5];
    let mut n = 0;
    for &byte in text {
        match byte {
            b'z' if n == 0 => out.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                group[n] = byte - b'!';
                n += 1;
                if n == 5 {
                    out.extend_from_slice(&base85_group(&group, 5));
                    n = 0;
                }
            }
            _ if crate::scan::is_space(byte) => {}
            _ => return None,
        }
    }
    if n == 1 {
        return None;
    }
    if n > 1 {
        out.extend_from_slice(&base85_group(&group, n));
    }
    Some(out)
}
