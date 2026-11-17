//! Files: reading the program itself (`currentfile`), strings read as
//! files, decoding filters, and outputs that go nowhere.
//!
//! A program cannot open files on the machine: `file` knows only the
//! standard streams, and `run`, `deletefile` and `renamefile` fail as on a
//! device without storage.

use std::rc::Rc;

use flate2::Decompress;

use crate::filter::{Filter, Kind, Source};
use crate::interp::{Interp, PsError, R};
use crate::object::{Key, Object, Str, Value};
use crate::vm::File;

/// The file operand `depth` places below the top.
fn file_at(i: &Interp, depth: usize) -> R<u32> {
    match i.peek(depth)?.value {
        Value::File(file) => Ok(file),
        _ => Err(PsError::TypeCheck.into()),
    }
}

pub(crate) fn currentfile(i: &mut Interp) -> R {
    let file = match i.current_file() {
        Some(file) => file,
        None => i.vm.new_file(File::Closed)?,
    };
    i.push(Object::lit(Value::File(file)))
}

pub(crate) fn file(i: &mut Interp) -> R {
    let name = i.string(1)?;
    i.string(0)?;
    let file = match i.vm.str_bytes(name) {
        b"%stdin" => File::Bytes {
            data: Rc::from(&b""[..]),
            pos: 0,
        },
        b"%stdout" | b"%stderr" | b"%lineedit" | b"%statementedit" => File::Sink,
        _ => return Err(PsError::UndefinedFilename.into()),
    };
    let file = i.vm.new_file(file)?;
    i.drop_n(2);
    i.push(Object::lit(Value::File(file)))
}

/// `filter`: decoding filters over a file, a string or a procedure.
pub(crate) fn filter(i: &mut Interp) -> R {
    let name = match i.peek(0)?.value {
        Value::Name(name) => i.vm.name_text(name),
        _ => return Err(PsError::TypeCheck.into()),
    };
    // Some filters take parameters before the name, others an optional
    // dictionary; `used` counts the operands taken.
    let mut used = 1;
    let params = match i.peek(1).map(|o| o.value) {
        Ok(Value::Dict(dict)) if name != "SubFileDecode" => {
            used += 1;
            Some(dict)
        }
        _ => None,
    };
    let kind = match name.as_str() {
        "ASCIIHexDecode" => Kind::AsciiHex,
        "ASCII85Decode" => Kind::Ascii85,
        "RunLengthDecode" => Kind::RunLength,
        "FlateDecode" => {
            if let Some(dict) = params {
                let key = Key::Name(i.vm.name(b"Predictor"));
                if let Some(predictor) = i.vm.dict_get(dict, &key).and_then(Object::as_f64)
                    && predictor > 1.0
                {
                    i.unsupported("deflate data with a predictor");
                }
            }
            Kind::Flate(Box::new(Decompress::new(true)))
        }
        "SubFileDecode" => {
            // source count string /SubFileDecode, or source dict /SubFileDecode.
            let (count, eod) = match i.peek(1)?.value {
                Value::Dict(dict) => {
                    used += 1;
                    let count_key = Key::Name(i.vm.name(b"EODCount"));
                    let string_key = Key::Name(i.vm.name(b"EODString"));
                    let count = i.vm.dict_get(dict, &count_key).and_then(Object::as_f64);
                    let eod = match i.vm.dict_get(dict, &string_key).map(|o| o.value) {
                        Some(Value::String(s)) => i.vm.str_bytes(s).to_vec(),
                        _ => Vec::new(),
                    };
                    (count.unwrap_or(0.0), eod)
                }
                Value::String(s) => {
                    used += 2;
                    let count = i.num(2)?;
                    (count, i.vm.str_bytes(s).to_vec())
                }
                _ => return Err(PsError::TypeCheck.into()),
            };
            // Counts are whole numbers the program wrote. With an empty
            // string a count of zero means no limit: the whole source.
            #[allow(clippy::cast_possible_truncation)]
            let count = count as i64;
            let count = if eod.is_empty() && count == 0 {
                -1
            } else {
                count
            };
            Kind::SubFile {
                count,
                eod,
                held: Vec::new(),
            }
        }
        "NullEncode" | "ASCIIHexEncode" | "ASCII85Encode" | "RunLengthEncode" | "FlateEncode"
        | "LZWEncode" | "DCTEncode" | "CCITTFaxEncode" => {
            // An encoding filter writes; nothing here reads what it writes.
            let source = i.peek(used)?;
            let _ = source;
            let file = i.vm.new_file(File::Sink)?;
            i.drop_n(used + 1);
            return i.push(Object::lit(Value::File(file)));
        }
        "LZWDecode" | "DCTDecode" | "CCITTFaxDecode" | "ReusableStreamDecode" => {
            i.unsupported(format!("the {name} filter"));
            return Err(PsError::Undefined.into());
        }
        _ => return Err(PsError::Undefined.into()),
    };
    let source = i.peek(used)?;
    let source = match source.value {
        Value::File(file) => Source::File(file),
        Value::String(s) if !source.exec => Source::String {
            data: i.vm.str_bytes(s).into(),
            pos: 0,
        },
        Value::Array(_) | Value::Packed(_) | Value::String(_) => Source::Proc {
            proc: source,
            buffer: Vec::new(),
            pos: 0,
            ended: false,
        },
        _ => return Err(PsError::TypeCheck.into()),
    };
    let file =
        i.vm.new_file(File::Filter(Box::new(Filter::new(source, kind))))?;
    i.drop_n(used + 1);
    i.push(Object::lit(Value::File(file)))
}

pub(crate) fn closefile(i: &mut Interp) -> R {
    let file = file_at(i, 0)?;
    if file != i.program {
        i.vm.files[file as usize] = File::Closed;
    }
    i.drop_n(1);
    Ok(())
}

pub(crate) fn read(i: &mut Interp) -> R {
    let file = file_at(i, 0)?;
    i.drop_n(1);
    match i.read_byte(file)? {
        Some(byte) => {
            i.push(Object::int(i32::from(byte)))?;
            i.push(Object::bool(true))
        }
        None => i.push(Object::bool(false)),
    }
}

/// Puts the part of `target` filled, and whether it was filled whole.
fn give_string(i: &mut Interp, target: Str, filled: usize) -> R {
    let whole = filled == target.len as usize;
    let part = Str {
        len: u32::try_from(filled).unwrap_or(0),
        ..target
    };
    i.push(Object::lit(Value::String(part)))?;
    i.push(Object::bool(whole))
}

pub(crate) fn readstring(i: &mut Interp) -> R {
    let (file, target) = (file_at(i, 1)?, i.string(0)?);
    i.drop_n(2);
    let mut filled = 0;
    while filled < target.len as usize {
        let Some(byte) = i.read_byte(file)? else {
            break;
        };
        i.vm.str_bytes_mut(target)[filled] = byte;
        filled += 1;
    }
    i.tick(u64::try_from(filled / 64).unwrap_or(0))?;
    give_string(i, target, filled)
}

pub(crate) fn readhexstring(i: &mut Interp) -> R {
    let (file, target) = (file_at(i, 1)?, i.string(0)?);
    i.drop_n(2);
    let mut filled = 0;
    let mut high: Option<u8> = None;
    while filled < target.len as usize {
        let Some(byte) = i.read_byte(file)? else {
            break;
        };
        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => continue,
        };
        match high.take() {
            Some(h) => {
                i.vm.str_bytes_mut(target)[filled] = (h << 4) | nibble;
                filled += 1;
            }
            None => high = Some(nibble),
        }
    }
    i.tick(u64::try_from(filled / 32).unwrap_or(0))?;
    give_string(i, target, filled)
}

pub(crate) fn readline(i: &mut Interp) -> R {
    let (file, target) = (file_at(i, 1)?, i.string(0)?);
    i.drop_n(2);
    let mut filled = 0;
    let mut ended = false;
    loop {
        let Some(byte) = i.read_byte(file)? else {
            break;
        };
        match byte {
            b'\n' => {
                ended = true;
                break;
            }
            b'\r' => {
                ended = true;
                // A carriage return and line feed end one line.
                match i.read_byte(file)? {
                    Some(b'\n') | None => {}
                    Some(_) => i.unread_byte(file),
                }
                break;
            }
            _ => {
                if filled >= target.len as usize {
                    return Err(PsError::RangeCheck.into());
                }
                i.vm.str_bytes_mut(target)[filled] = byte;
                filled += 1;
            }
        }
    }
    i.tick(u64::try_from(filled / 64).unwrap_or(0))?;
    let part = Str {
        len: u32::try_from(filled).unwrap_or(0),
        ..target
    };
    i.push(Object::lit(Value::String(part)))?;
    i.push(Object::bool(ended || filled > 0))
}

/// `token` on a file or a string.
pub(crate) fn token(i: &mut Interp) -> R {
    let source = i.peek(0)?;
    match source.value {
        Value::File(file) => {
            i.drop_n(1);
            let file = i.executable_file(file)?;
            match i.scan_file(file)? {
                Some(object) => {
                    i.push(object)?;
                    i.push(Object::bool(true))
                }
                None => i.push(Object::bool(false)),
            }
        }
        Value::String(s) => {
            let data: Rc<[u8]> = i.vm.str_bytes(s).into();
            let mut pos = 0;
            let token = i.scan(&data, &mut pos, None)?;
            i.drop_n(1);
            match token {
                Some(object) => {
                    // The scanner has used up one white-space character
                    // after the token, as the language says.
                    let rest = Str {
                        id: s.id,
                        start: s.start + u32::try_from(pos).unwrap_or(0),
                        len: s.len - u32::try_from(pos.min(s.len as usize)).unwrap_or(0),
                    };
                    i.push(Object::lit(Value::String(rest)))?;
                    i.push(object)?;
                    i.push(Object::bool(true))
                }
                None => i.push(Object::bool(false)),
            }
        }
        _ => Err(PsError::TypeCheck.into()),
    }
}

pub(crate) fn bytesavailable(i: &mut Interp) -> R {
    let file = file_at(i, 0)?;
    let n = match &i.vm.files[file as usize] {
        File::Bytes { data, pos } => i32::try_from(data.len().saturating_sub(*pos)).unwrap_or(-1),
        _ => -1,
    };
    i.drop_n(1);
    i.push(Object::int(n))
}

/// `flushfile`: on an input file, reads and discards the rest of it. The
/// program file itself is left alone, as a device leaves its input.
pub(crate) fn flushfile(i: &mut Interp) -> R {
    let file = file_at(i, 0)?;
    i.drop_n(1);
    if matches!(i.vm.files[file as usize], File::Filter(_)) {
        let mut n = 0u64;
        while i.read_byte(file)?.is_some() {
            n += 1;
            if n.is_multiple_of(256) {
                i.tick(1)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn resetfile(i: &mut Interp) -> R {
    file_at(i, 0)?;
    i.drop_n(1);
    Ok(())
}

pub(crate) fn status(i: &mut Interp) -> R {
    match i.peek(0)?.value {
        Value::File(file) => {
            let open = !matches!(i.vm.files[file as usize], File::Closed);
            i.drop_n(1);
            i.push(Object::bool(open))
        }
        Value::String(_) => {
            i.drop_n(1);
            i.push(Object::bool(false))
        }
        _ => Err(PsError::TypeCheck.into()),
    }
}

pub(crate) fn write(i: &mut Interp) -> R {
    file_at(i, 1)?;
    i.int(0)?;
    i.drop_n(2);
    Ok(())
}

pub(crate) fn writestring(i: &mut Interp) -> R {
    file_at(i, 1)?;
    i.drop_n(2);
    Ok(())
}

/// `print`, `=` and `==`: output nobody reads.
pub(crate) fn print(i: &mut Interp) -> R {
    i.pop().map(drop)
}

pub(crate) fn run(i: &mut Interp) -> R {
    i.string(0)?;
    Err(PsError::UndefinedFilename.into())
}

pub(crate) fn deletefile(i: &mut Interp) -> R {
    i.string(0)?;
    Err(PsError::UndefinedFilename.into())
}

pub(crate) fn renamefile(i: &mut Interp) -> R {
    i.string(1)?;
    Err(PsError::UndefinedFilename.into())
}

/// `eexec` starts an encrypted section. Encrypted data is never decoded
/// here: the section is stepped over, up to the run of zeros that ends
/// it, and the program goes on with what follows (usually `cleartomark`).
pub(crate) fn eexec(i: &mut Interp) -> R {
    let source = i.pop()?;
    i.unsupported("encrypted sections (`eexec`), skipped");
    let Value::File(file) = source.value else {
        return Ok(());
    };
    let mut zeros = 0usize;
    let mut read = 0u64;
    while let Some(byte) = i.read_byte(file)? {
        read += 1;
        if read.is_multiple_of(256) {
            i.tick(1)?;
        }
        match byte {
            b'0' => {
                zeros += 1;
                if zeros >= 64 {
                    break;
                }
            }
            b'\n' | b'\r' if zeros > 0 => {}
            _ => zeros = 0,
        }
    }
    Ok(())
}
