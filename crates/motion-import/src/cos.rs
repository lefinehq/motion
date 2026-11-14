//! The object syntax of a text document's body (`LIST:btdk`): PDF-style
//! dictionaries, arrays, names, numbers, booleans and strings
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#cos-syntax)).
//!
//! [`parse`] reads a body into a [`Cos`] tree with a `winnow` grammar; the
//! [`de`] module then reads typed records out of the tree with serde, so the
//! text-document model names its keys in field attributes, not in code.

pub(crate) mod de;

use motion_format::grammar::{Failure, Parsed, parse_at};
use motion_format::{Diagnostic, Kind, Result};
use winnow::ascii::{digit0, multispace0};
use winnow::combinator::{
    alt, cut_err, delimited, eof, fail, not, opt, preceded, repeat, terminated,
};
use winnow::error::{ErrMode, FromExternalError};
use winnow::prelude::*;
use winnow::token::{any, none_of, one_of, take_while};

/// How deep dictionaries and arrays may nest. The samples nest about 15
/// deep; the limit keeps a corrupted body from exhausting the stack.
const MAX_DEPTH: usize = 128;

/// The byte-order mark that starts a UTF-16 string.
const UTF16_BOM: [u8; 2] = [0xFE, 0xFF];

/// One value of the object syntax.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cos {
    /// `<< /key value ... >>`: entries in the order written, which the
    /// format does not sort.
    Dict(Vec<(String, Cos)>),
    /// `[ value ... ]`.
    Array(Vec<Cos>),
    /// `/Name`: an object's type, or `/nil` for no value.
    Name(String),
    /// A number written without a decimal point.
    Integer(i64),
    /// A number written with a decimal point; kept apart from integers
    /// because the format never writes one as the other.
    Real(f64),
    /// `true` or `false`.
    Bool(bool),
    /// `( ... )`.
    String(CosString),
}

/// A string's bytes, with its escapes removed, and how they encode text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CosString {
    /// The bytes, without the byte-order mark of a UTF-16 string.
    pub bytes: Vec<u8>,
    /// How the bytes encode text; it cannot be inferred from the content.
    pub encoding: Encoding,
}

/// How a [`CosString`] encodes its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    /// UTF-16 big-endian, written after a byte-order mark.
    Utf16,
    /// Plain bytes, such as a bit-flag string `(00110)`.
    Bytes,
}

impl CosString {
    /// The string's text, with invalid code units replaced.
    pub fn text(&self) -> String {
        match self.encoding {
            Encoding::Utf16 => {
                let (units, _) = self.bytes.as_chunks::<2>();
                char::decode_utf16(units.iter().map(|&unit| u16::from_be_bytes(unit)))
                    .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect()
            }
            Encoding::Bytes => String::from_utf8_lossy(&self.bytes).into_owned(),
        }
    }

    /// A string from its bytes as written: UTF-16 when they start with the
    /// byte-order mark.
    fn from_written(bytes: Vec<u8>) -> Self {
        match bytes.strip_prefix(&UTF16_BOM) {
            Some(units) => CosString {
                bytes: units.to_vec(),
                encoding: Encoding::Utf16,
            },
            None => CosString {
                bytes,
                encoding: Encoding::Bytes,
            },
        }
    }
}

impl Cos {
    /// The value at `path`, keys and array indices separated by `/`
    /// (`1/1/0`), as the spec writes paths without their leading slash.
    pub fn get(&self, path: &str) -> Option<&Cos> {
        path.split('/')
            .filter(|key| !key.is_empty())
            .try_fold(self, |cos, key| match cos {
                Cos::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                Cos::Array(items) => items.get(key.parse::<usize>().ok()?),
                _ => None,
            })
    }

    /// The value at `path`, as [`Cos::get`], to change.
    pub fn get_mut(&mut self, path: &str) -> Option<&mut Cos> {
        path.split('/')
            .filter(|key| !key.is_empty())
            .try_fold(self, |cos, key| match cos {
                Cos::Dict(entries) => entries.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
                Cos::Array(items) => items.get_mut(key.parse::<usize>().ok()?),
                _ => None,
            })
    }

    /// The items of an array, or none for any other value.
    pub fn items_mut(&mut self) -> &mut [Cos] {
        match self {
            Cos::Array(items) => items,
            _ => &mut [],
        }
    }

    /// `top` laid over `self`: for two dictionaries, every key of `self`,
    /// with `top`'s value where `top` has one, then `top`'s other keys;
    /// otherwise `top`.
    pub fn overlay(&self, top: &Cos) -> Cos {
        match (self, top) {
            (Cos::Dict(base), Cos::Dict(over)) => {
                let find = |key: &str| over.iter().find(|(k, _)| k == key).map(|(_, v)| v);
                let merged = base
                    .iter()
                    .map(|(key, value)| (key.clone(), find(key).unwrap_or(value).clone()));
                let added = over
                    .iter()
                    .filter(|(key, _)| !base.iter().any(|(k, _)| k == key))
                    .cloned();
                Cos::Dict(merged.chain(added).collect())
            }
            _ => top.clone(),
        }
    }
}

/// Why the grammar failed, named in the diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    /// A value was expected and none starts here.
    Value,
    /// A dictionary key, a name, was expected.
    Key,
    /// A dictionary has no closing `>>`.
    DictEnd,
    /// An array has no closing `]`.
    ArrayEnd,
    /// A string has no closing `)`.
    StringEnd,
    /// A number does not fit.
    Number,
    /// Lists nest deeper than [`MAX_DEPTH`].
    Depth,
    /// Something follows the last entry.
    Trailing,
}

impl Problem {
    /// What the failure is, for people.
    fn describe(self) -> &'static str {
        match self {
            Problem::Value => "expected a value",
            Problem::Key => "expected a `/key`",
            Problem::DictEnd => "a dictionary is not closed with `>>`",
            Problem::ArrayEnd => "an array is not closed with `]`",
            Problem::StringEnd => "a string is not closed with `)`",
            Problem::Number => "a number is out of range",
            Problem::Depth => "values nest too deep",
            Problem::Trailing => "unexpected bytes after the last entry",
        }
    }
}

/// Reads a whole body: a dictionary written without its `<<` and `>>`.
///
/// # Errors
/// A syntax [`Diagnostic`] naming what failed and at which byte.
pub(crate) fn parse(body: &[u8]) -> Result<Cos> {
    let grammar = terminated(
        entries(0),
        (multispace0, cut_err(eof).context(Problem::Trailing)),
    )
    .map(Cos::Dict);
    parse_at(body, grammar, |problem, offset| {
        let problem = problem.unwrap_or(Problem::Value);
        Diagnostic::new(
            Kind::Syntax,
            format!("text document: {} at byte {offset}", problem.describe()),
        )
        .expected("a body of `/key value` entries, as ` /0 << /1 2.0 >> /1 [ (text) ]`")
    })
}

/// The result of a grammar piece over a body.
type Step<T> = Parsed<T, Problem>;

/// A grammar piece that takes its nesting depth.
trait Piece<'i, T>: Parser<&'i [u8], T, ErrMode<Failure<Problem>>> {}

impl<'i, T, P: Parser<&'i [u8], T, ErrMode<Failure<Problem>>>> Piece<'i, T> for P {}

/// `/key value` entries, each after optional white space, up to the first
/// thing that is not a key.
fn entries<'i>(depth: usize) -> impl Piece<'i, Vec<(String, Cos)>> {
    repeat(
        0..,
        preceded(
            multispace0,
            (name, preceded(multispace0, cut_err(value(depth)))),
        ),
    )
}

/// Any value, inside `depth` lists.
fn value<'i>(depth: usize) -> impl Piece<'i, Cos> {
    move |input: &mut &'i [u8]| {
        alt((
            dict(depth),
            array(depth),
            name.map(Cos::Name),
            string.map(Cos::String),
            boolean.map(Cos::Bool),
            number,
            fail.context(Problem::Value),
        ))
        .parse_next(input)
    }
}

/// Fails for good when a list opened inside `depth` lists would pass
/// [`MAX_DEPTH`].
fn room<'i>(depth: usize) -> impl Piece<'i, ()> {
    move |input: &mut &'i [u8]| match depth {
        MAX_DEPTH.. => cut_err(fail).context(Problem::Depth).parse_next(input),
        _ => Ok(()),
    }
}

/// `<< /key value ... >>`.
fn dict<'i>(depth: usize) -> impl Piece<'i, Cos> {
    delimited(
        (b"<<".as_slice(), room(depth)),
        entries(depth + 1),
        (
            multispace0,
            cut_err(b">>".as_slice()).context(Problem::DictEnd),
        ),
    )
    .map(Cos::Dict)
}

/// `[ value ... ]`.
fn array<'i>(depth: usize) -> impl Piece<'i, Cos> {
    delimited(
        (b'[', room(depth)),
        repeat(0.., preceded(multispace0, value(depth + 1))),
        (multispace0, cut_err(b']').context(Problem::ArrayEnd)),
    )
    .map(Cos::Array)
}

/// A byte that may appear in a name: not white space and not a delimiter.
fn regular(byte: u8) -> bool {
    !byte.is_ascii_whitespace() && !b"()<>[]{}/%".contains(&byte)
}

/// `/Name`, returned without its slash.
fn name(input: &mut &[u8]) -> Step<String> {
    preceded(b'/', take_while(1.., regular))
        .map(|bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned())
        .context(Problem::Key)
        .parse_next(input)
}

/// `true` or `false`, ending at a byte that cannot continue a word.
fn boolean(input: &mut &[u8]) -> Step<bool> {
    terminated(
        alt((
            b"true".as_slice().value(true),
            b"false".as_slice().value(false),
        )),
        not(one_of(regular)),
    )
    .parse_next(input)
}

/// An integer, or a real when it has a decimal point (`-2`, `9.0`, `.5`).
/// Once its digits are read, a number that does not fit fails for good.
fn number(input: &mut &[u8]) -> Step<Cos> {
    let start = *input;
    let text = (opt(b'-'), digit0, opt((b'.', digit0)))
        .take()
        .verify(|text: &[u8]| text.iter().any(u8::is_ascii_digit))
        .parse_next(input)?;
    // The grammar took only ASCII digits, a sign and a point.
    let text = String::from_utf8_lossy(text);
    let number = match text.contains('.') {
        true => text.parse().map(Cos::Real).ok(),
        false => text.parse().map(Cos::Integer).ok(),
    };
    number.ok_or_else(|| {
        *input = start;
        ErrMode::Cut(Failure::from_external_error(input, Problem::Number))
    })
}

/// `( ... )`, with `\` escaping the next byte.
fn string(input: &mut &[u8]) -> Step<CosString> {
    let byte = alt((preceded(b'\\', any), none_of([b'(', b')', b'\\'])));
    delimited(
        b'(',
        repeat(0.., byte).fold(Vec::new, |mut bytes, byte| {
            bytes.push(byte);
            bytes
        }),
        cut_err(b')').context(Problem::StringEnd),
    )
    .map(CosString::from_written)
    .parse_next(input)
}

#[cfg(test)]
#[path = "../tests/unit/cos.rs"]
mod tests;
