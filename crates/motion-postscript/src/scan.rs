//! The scanner: bytes to objects, as the language's syntax defines them.
//!
//! It reads numbers (with radix forms), names (literal `/a`, immediately
//! evaluated `//a`, executable `a`), strings in parentheses with their
//! escapes, hexadecimal `<…>` and base-85 `<~…~>` strings, procedures in
//! braces, and the self-delimiting `[ ] << >>`. Comments are skipped; in
//! the program file the scanner reads the document-structure comments it
//! needs: `%%EOF` ends the program, and a declared binary section
//! (`%%BeginBinary`, or `%%BeginData` of binary data) that the program
//! does not read itself is stepped over rather than scanned as text.

use std::rc::Rc;

use crate::filter::{ascii85_all, hex_all};
use crate::interp::{Fault, Halt, Interp, PsError, R};
use crate::object::{Object, Value};
use crate::vm::File;

/// The deepest procedures may nest in the source.
const MAX_BRACES: usize = 256;

/// Whether `byte` is white space.
pub(crate) const fn is_space(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\0' | b'\x04'
    )
}

/// Whether `byte` ends a regular token.
pub(crate) const fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

impl Interp {
    /// The next object of file `file`, or `None` at its end.
    pub fn scan_file(&mut self, file: u32) -> R<Option<Object>> {
        let (data, mut pos) = match &self.vm.files[file as usize] {
            File::Bytes { data, pos } => (Rc::clone(data), *pos),
            File::Filter(_) => {
                let file = self.executable_file(file)?;
                return self.scan_file(file);
            }
            File::Sink | File::Closed => return Ok(None),
        };
        let program = (file == self.program).then_some(file);
        let token = self.scan(&data, &mut pos, program);
        if let File::Bytes { pos: at, .. } = &mut self.vm.files[file as usize] {
            *at = pos;
        }
        token
    }

    /// Makes file `file` executable token by token: a filter is drained
    /// into memory, in its own place, so `currentfile` reads go on from
    /// where the scanner stops.
    pub fn executable_file(&mut self, file: u32) -> R<u32> {
        if matches!(self.vm.files[file as usize], File::Filter(_)) {
            let mut bytes = Vec::new();
            while let Some(byte) = self.read_byte(file)? {
                bytes.push(byte);
                if bytes.len() % 4096 == 0 {
                    self.tick(64)?;
                    self.vm.charge(4096)?;
                }
            }
            self.vm.files[file as usize] = File::Bytes {
                data: bytes.into(),
                pos: 0,
            };
        }
        Ok(file)
    }

    /// The next object in `data` from `pos`, or `None` at its end.
    /// `program` is the program file's id when `data` is the program,
    /// whose structure comments the scanner reads.
    pub fn scan(
        &mut self,
        data: &[u8],
        pos: &mut usize,
        program: Option<u32>,
    ) -> R<Option<Object>> {
        // Procedures being built, innermost last.
        let mut open: Vec<Vec<Object>> = Vec::new();
        loop {
            let Some(token) = self.token(data, pos, program.is_some())? else {
                if open.is_empty() {
                    return Ok(None);
                }
                return Err(PsError::SyntaxError.into());
            };
            let object = match token {
                Token::Object(object) => object,
                Token::Open => {
                    if open.len() >= MAX_BRACES {
                        return Err(PsError::LimitCheck.into());
                    }
                    open.push(Vec::new());
                    continue;
                }
                Token::Close => {
                    let Some(body) = open.pop() else {
                        return Err(PsError::SyntaxError.into());
                    };
                    let arr = self.vm.new_array(body)?;
                    Object::exe(Value::Array(arr))
                }
                Token::Immediate(name) => match self.lookup(name) {
                    Some(value) => value,
                    None => {
                        self.current = Object::lit(Value::Name(name));
                        return Err(PsError::Undefined.into());
                    }
                },
            };
            match open.last_mut() {
                Some(body) => body.push(object),
                None => return Ok(Some(object)),
            }
        }
    }

    /// The next token.
    fn token(&mut self, data: &[u8], pos: &mut usize, program: bool) -> R<Option<Token>> {
        loop {
            while *pos < data.len() && is_space(data[*pos]) {
                *pos += 1;
            }
            let Some(&byte) = data.get(*pos) else {
                return Ok(None);
            };
            if program
                && self.in_binary(*pos)
                && !is_text(byte)
                && let Some((_, end)) = self.structure.binary.take()
            {
                self.unsupported("binary data the program did not read (skipped)");
                *pos = end.max(*pos + 1);
                continue;
            }
            match byte {
                b'%' => {
                    let start = *pos;
                    while *pos < data.len() && !matches!(data[*pos], b'\n' | b'\r') {
                        *pos += 1;
                    }
                    if program && data[start..].starts_with(b"%%") {
                        let at_line_start = start == 0 || matches!(data[start - 1], b'\n' | b'\r');
                        if at_line_start && self.structure_comment(data, start, pos)? {
                            return Ok(None);
                        }
                    }
                }
                b'(' => {
                    *pos += 1;
                    let bytes = literal_string(data, pos)?;
                    return self.string_token(bytes).map(Some);
                }
                b'<' => {
                    *pos += 1;
                    match data.get(*pos) {
                        Some(b'<') => {
                            *pos += 1;
                            return Ok(Some(self.exec_name(b"<<")));
                        }
                        Some(b'~') => {
                            *pos += 1;
                            let start = *pos;
                            let end = find(data, start, b"~>")
                                .ok_or(Fault::Error(PsError::SyntaxError))?;
                            *pos = end + 2;
                            let bytes = ascii85_all(&data[start..end])
                                .ok_or(Fault::Error(PsError::SyntaxError))?;
                            return self.string_token(bytes).map(Some);
                        }
                        _ => {
                            let start = *pos;
                            let end = data[start..]
                                .iter()
                                .position(|&b| b == b'>')
                                .map(|i| start + i)
                                .ok_or(Fault::Error(PsError::SyntaxError))?;
                            *pos = end + 1;
                            let bytes = hex_all(&data[start..end])
                                .ok_or(Fault::Error(PsError::SyntaxError))?;
                            return self.string_token(bytes).map(Some);
                        }
                    }
                }
                b'>' => {
                    *pos += 1;
                    if data.get(*pos) == Some(&b'>') {
                        *pos += 1;
                        return Ok(Some(self.exec_name(b">>")));
                    }
                    return Err(PsError::SyntaxError.into());
                }
                b'[' | b']' => {
                    *pos += 1;
                    return Ok(Some(self.exec_name(&[byte])));
                }
                b'{' => {
                    *pos += 1;
                    return Ok(Some(Token::Open));
                }
                b'}' => {
                    *pos += 1;
                    return Ok(Some(Token::Close));
                }
                b')' => {
                    *pos += 1;
                    return Err(PsError::SyntaxError.into());
                }
                b'/' => {
                    *pos += 1;
                    let immediate = data.get(*pos) == Some(&b'/');
                    if immediate {
                        *pos += 1;
                    }
                    let word = regular(data, pos);
                    let name = self.vm.name(word);
                    consume_space(data, pos);
                    if immediate {
                        return Ok(Some(Token::Immediate(name)));
                    }
                    return Ok(Some(Token::Object(Object::lit(Value::Name(name)))));
                }
                0x80..=0x9F => {
                    // Binary tokens and binary object sequences.
                    self.unsupported("binary tokens");
                    *pos += 1;
                    return Err(PsError::SyntaxError.into());
                }
                _ => {
                    let start = *pos;
                    let word = regular(data, pos);
                    if word.is_empty() {
                        *pos = start + 1;
                        return Err(PsError::SyntaxError.into());
                    }
                    let token = match number(word) {
                        Some(number) => number,
                        None => Object::exe(Value::Name(self.vm.name(word))),
                    };
                    consume_space(data, pos);
                    return Ok(Some(Token::Object(token)));
                }
            }
        }
    }

    /// Reads one structure comment that starts at `start` and ends at
    /// `end`; true when it ends the program.
    fn structure_comment(&mut self, data: &[u8], start: usize, end: &mut usize) -> R<bool> {
        let line = &data[start..*end];
        if line.starts_with(b"%%EOF") && self.structure.documents == 0 {
            *end = data.len();
            return Err(Fault::Halt(Halt::Quit));
        }
        if line.starts_with(b"%%BeginDocument") {
            self.structure.documents += 1;
        } else if line.starts_with(b"%%EndDocument") {
            self.structure.documents = self.structure.documents.saturating_sub(1);
        } else if let Some(rest) = line.strip_prefix(b"%%BeginBinary:") {
            let body = line_after(data, *end);
            if let Some(count) = first_number(rest) {
                self.structure.binary = Some((body, body.saturating_add(count).min(data.len())));
            }
        } else if let Some(rest) = line.strip_prefix(b"%%BeginData:") {
            let words: Vec<&[u8]> = rest
                .split(|b| is_space(*b))
                .filter(|w| !w.is_empty())
                .collect();
            let count = words.first().and_then(|w| first_number(w));
            let binary = words
                .get(1)
                .is_some_and(|w| w.eq_ignore_ascii_case(b"Binary"));
            let lines = words
                .get(2)
                .is_some_and(|w| w.eq_ignore_ascii_case(b"Lines"));
            if let (Some(count), true) = (count, binary) {
                let body = line_after(data, *end);
                let stop = if lines {
                    skip_lines(data, body, count)
                } else {
                    body.saturating_add(count).min(data.len())
                };
                self.structure.binary = Some((body, stop));
            }
        } else if line.starts_with(b"%%EndBinary") || line.starts_with(b"%%EndData") {
            self.structure.binary = None;
        }
        Ok(false)
    }

    /// Whether `pos` lies in the declared binary section.
    fn in_binary(&self, pos: usize) -> bool {
        self.structure
            .binary
            .is_some_and(|(start, end)| pos >= start && pos < end)
    }

    /// A string token, allocated.
    fn string_token(&mut self, bytes: Vec<u8>) -> R<Token> {
        let s = self.vm.new_string(bytes)?;
        Ok(Token::Object(Object::lit(Value::String(s))))
    }

    /// An executable name token.
    fn exec_name(&mut self, bytes: &[u8]) -> Token {
        Token::Object(Object::exe(Value::Name(self.vm.name(bytes))))
    }
}

/// One token.
enum Token {
    Object(Object),
    /// `{`
    Open,
    /// `}`
    Close,
    /// `//name`, replaced by its value as it is scanned.
    Immediate(u32),
}

/// Whether `byte` can appear in program text.
const fn is_text(byte: u8) -> bool {
    is_space(byte) || (byte >= 0x20 && byte < 0x7F)
}

/// Steps over the one white-space character that ends a name or a
/// number, as the scanner does, so a program reading its own file after a
/// token starts at the next character. A carriage return and line feed
/// count as one.
fn consume_space(data: &[u8], pos: &mut usize) {
    match data.get(*pos) {
        Some(b'\r') => {
            *pos += 1;
            if data.get(*pos) == Some(&b'\n') {
                *pos += 1;
            }
        }
        Some(&byte) if is_space(byte) => *pos += 1,
        _ => {}
    }
}

/// The regular characters from `pos`, up to a delimiter or white space.
fn regular<'a>(data: &'a [u8], pos: &mut usize) -> &'a [u8] {
    let start = *pos;
    while *pos < data.len() && !is_space(data[*pos]) && !is_delimiter(data[*pos]) {
        *pos += 1;
    }
    &data[start..*pos]
}

/// The first offset of `needle` in `data` from `from`.
fn find(data: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    data.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| from + i)
}

/// The offset where the line after the one ending at `end` starts.
fn line_after(data: &[u8], end: usize) -> usize {
    let mut at = end;
    if data.get(at) == Some(&b'\r') {
        at += 1;
    }
    if data.get(at) == Some(&b'\n') {
        at += 1;
    }
    at
}

/// The offset `count` lines on from `from`.
fn skip_lines(data: &[u8], from: usize, count: usize) -> usize {
    let mut at = from;
    for _ in 0..count {
        while at < data.len() && !matches!(data[at], b'\n' | b'\r') {
            at += 1;
        }
        at = line_after(data, at);
        if at >= data.len() {
            break;
        }
    }
    at.min(data.len())
}

/// The first unsigned decimal number in `bytes`.
fn first_number(bytes: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.split_whitespace().next()?.parse().ok()
}

/// A literal string's bytes, from just after its `(` to just after its `)`.
fn literal_string(data: &[u8], pos: &mut usize) -> R<Vec<u8>> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    while let Some(&byte) = data.get(*pos) {
        *pos += 1;
        match byte {
            b'(' => {
                depth += 1;
                out.push(byte);
            }
            b')' => {
                if depth == 0 {
                    return Ok(out);
                }
                depth -= 1;
                out.push(byte);
            }
            b'\r' => {
                // An end of line inside a string reads as one newline.
                if data.get(*pos) == Some(&b'\n') {
                    *pos += 1;
                }
                out.push(b'\n');
            }
            b'\\' => {
                let Some(&next) = data.get(*pos) else {
                    break;
                };
                *pos += 1;
                match next {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0c),
                    b'\r' => {
                        if data.get(*pos) == Some(&b'\n') {
                            *pos += 1;
                        }
                    }
                    b'\n' => {}
                    b'0'..=b'7' => {
                        let mut value = u32::from(next - b'0');
                        for _ in 0..2 {
                            match data.get(*pos) {
                                Some(&digit @ b'0'..=b'7') => {
                                    value = value * 8 + u32::from(digit - b'0');
                                    *pos += 1;
                                }
                                _ => break,
                            }
                        }
                        out.push((value & 0xFF) as u8);
                    }
                    other => out.push(other),
                }
            }
            _ => out.push(byte),
        }
    }
    Err(PsError::SyntaxError.into())
}

/// The number `word` spells, if it is one.
pub(crate) fn number(word: &[u8]) -> Option<Object> {
    let text = std::str::from_utf8(word).ok()?;
    let first = *word.first()?;
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return None;
    }
    if let Some((base, digits)) = text.split_once('#') {
        let base: u32 = base.parse().ok()?;
        if !(2..=36).contains(&base) || digits.is_empty() {
            return None;
        }
        let mut value: u64 = 0;
        for c in digits.chars() {
            value = value * u64::from(base) + u64::from(c.to_digit(base)?);
            if value > u64::from(u32::MAX) {
                #[allow(clippy::cast_precision_loss)]
                return Some(Object::real(value as f64));
            }
        }
        // A radix number is the 32 bits it spells, so 16#FFFFFFFF is -1.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        return Some(Object::int(value as u32 as i32));
    }
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if body.is_empty() {
        return None;
    }
    if body.bytes().all(|b| b.is_ascii_digit()) {
        return match text.parse::<i32>() {
            Ok(int) => Some(Object::int(int)),
            Err(_) => text.parse::<f64>().ok().map(Object::real),
        };
    }
    // A real: digits with one point, and an optional exponent.
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let points = mantissa.bytes().filter(|&b| b == b'.').count();
    let digits = mantissa.bytes().filter(u8::is_ascii_digit).count();
    if points > 1 || digits == 0 || !mantissa.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    let mut normal = text.to_owned();
    if normal.ends_with('.') {
        normal.push('0');
    }
    let value: f64 = normal
        .replace(".e", ".0e")
        .replace(".E", ".0E")
        .parse()
        .ok()?;
    value.is_finite().then_some(Object::real(value))
}
