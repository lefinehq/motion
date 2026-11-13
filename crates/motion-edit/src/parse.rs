//! Parsing of one-line edits into typed [`Edit`] values.
//!
//! A side branch working on the raw document from step 1 (not a numbered
//! rendering step). It only reads text; it knows nothing about scenes.
//! Whether a key is valid for its target is checked later, in `apply`.
//!
//! The line is read by a `winnow` grammar:
//!
//! ```text
//! line    = target (ws change)+
//! change  = path "=" value
//! path    = segment ("." segment)*      a segment is letters, digits, `-` and `_`
//! value   = quoted | json | bare
//! quoted  = '"' (char | '\"' | '\\')* '"'
//! json    = a JSON array or object, read by serde_json up to its closing bracket
//! bare    = text up to the next space: `none`, `true`, `false`, a number, or a string
//! ```
//!
//! A quote is only allowed around a whole value, and a quoted or JSON value
//! must end its word. Every syntax failure names a [`Problem`] where it
//! happens, and [`Problem::diagnostic`] is the one place that turns it into a
//! [`Diagnostic`]. Two changes that write the same place are a conflict, not
//! a syntax error, and [`Edit::check_conflicts`] reports them once the line
//! is read.

use std::borrow::Cow;
use std::fmt;

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_grammar::{Failure, Parsed, blank0, blank1, ends_with, parse_all, word};
use serde_json::{Number, Value};
use winnow::combinator::{
    alt, cut_err, delimited, dispatch, empty, eof, fail, opt, peek, preceded, repeat, separated,
    terminated,
};
use winnow::error::{ErrMode, FromExternalError};
use winnow::prelude::*;
use winnow::stream::Stream;
use winnow::token::{any, none_of, one_of, take_till, take_while};

/// A parsed edit: one target and the changes to make to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    /// A layer id, or `scene` for the top level of the document.
    pub target: String,
    /// The changes, in the order they were written. No two of them write
    /// the same place.
    pub changes: Vec<Change>,
}

/// One `key=value` pair of an edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// The key to change, e.g. `fill`, or a dotted path such as `anim.opacity`.
    pub key: KeyPath,
    /// What to do with it.
    pub value: EditValue,
}

/// The new state of a key.
#[derive(Debug, Clone, PartialEq)]
pub enum EditValue {
    /// Set the key to this JSON value.
    Set(Value),
    /// Remove the key (written `none`).
    Remove,
}

/// The key of a change: a dotted path such as `anim.opacity`, kept both as
/// written and as its segments, so neither has to be rebuilt from the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPath {
    /// The key as written, segments joined by `.`.
    text: String,
    /// The names of the path; never empty.
    segments: Vec<String>,
}

impl KeyPath {
    /// The key as written: `anim.opacity`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The names of the path, outermost first: `anim`, `opacity`. There is
    /// always at least one.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// True when one key is the other or a prefix of it, segment by
    /// segment: `anim` and `anim.x`, but not `anim` and `animal`.
    fn overlaps(&self, other: &KeyPath) -> bool {
        self.segments.starts_with(&other.segments) || other.segments.starts_with(&self.segments)
    }
}

impl fmt::Display for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Parses one edit line such as `card fill=#FF0000 in=none`.
///
/// # Errors
/// Returns a [`Diagnostic`] for an empty line, a missing target or changes,
/// a word that is not `key=value`, an unsupported operator, an empty or
/// malformed key, a missing value, a quote that does not go around a whole
/// value, an unclosed quote or bracket, a bad escape, invalid JSON, text
/// straight after a quoted or JSON value, a key given twice, or two keys
/// where one is a dotted prefix of the other.
pub fn parse_edit(line: &str) -> Result<Edit> {
    let edit = parse_all(line, edit, |problem| {
        // Every other failure names its problem where it happens, so a
        // failure without one can only mean the line has no word at all.
        problem.unwrap_or(Problem::Empty).diagnostic()
    })?;
    edit.check_conflicts()?;
    Ok(edit)
}

impl Edit {
    /// Rejects two changes that write the same place: `anim` and `anim.x`
    /// would, and which one wins would depend on their order.
    fn check_conflicts(&self) -> Result<()> {
        for (i, later) in self.changes.iter().enumerate() {
            let Some(earlier) = self.changes[..i]
                .iter()
                .find(|earlier| earlier.key.overlaps(&later.key))
            else {
                continue;
            };
            let (outer, inner) = if earlier.key.segments.len() <= later.key.segments.len() {
                (&earlier.key, &later.key)
            } else {
                (&later.key, &earlier.key)
            };
            let message = if outer == inner {
                format!("key `{outer}` given twice")
            } else {
                format!(
                    "keys `{outer}` and `{inner}` overlap; set them in separate edits or merge them into one value"
                )
            };
            return Err(Diagnostic::new(Kind::Conflict, message));
        }
        Ok(())
    }
}

/// What the edit grammar fails with.
type Error<'s> = ErrMode<Failure<Problem<'s>>>;

/// The accepted form of a whole line.
const LINE_FORM: &str = "`<target> <key>=<value>`";
/// The accepted form of one change.
const CHANGE_FORM: &str = "`<key>=<value>`";
/// The accepted form of a key.
const PATH_FORM: &str = "`<key>[.<key>…]=<value>`";
/// The accepted form of a JSON value.
const JSON_FORM: &str = "`<key>=[…]` or `<key>={…}`, valid JSON";
/// The escapes a quoted value accepts.
const ESCAPES: &str = "`\\\"` or `\\\\`";
/// The operators people try before `=`; only `=` is supported for now.
const OPERATORS: [char; 7] = ['+', '-', '*', '/', '<', '>', '!'];

/// What is wrong with the syntax of an edit line, with the text it is
/// wrong about.
#[derive(Debug, Clone)]
enum Problem<'s> {
    /// The line has no word at all.
    Empty,
    /// The first word is already a change: there is no target.
    NoTarget(&'s str),
    /// The target has no changes after it.
    NoChanges(&'s str),
    /// A word after the target has no `=`.
    NotAChange(&'s str),
    /// A quote in a target or a key: the text before it.
    StrayQuote(&'s str),
    /// A quote inside a bare value. `meant` is the whole value as its author
    /// meant it, for the suggestion.
    Misquoted {
        written: &'s str,
        key: &'s str,
        meant: String,
    },
    /// An operator such as `+` before the `=`.
    Operator(char),
    /// A change with nothing before its `=`.
    MissingKey(&'s str),
    /// A key that is not a dotted path of names.
    InvalidKey(&'s str),
    /// A change with nothing after its `=`.
    MissingValue(&'s str),
    /// A quoted value without its closing quote; `rest` is the line from the
    /// value on.
    UnclosedQuote { key: &'s str, rest: &'s str },
    /// A quoted value followed by more text; `text` is the value with it.
    TextAfterQuote { key: &'s str, text: &'s str },
    /// A `\` followed by something other than `"` or `\`.
    Escape(char),
    /// A `\` at the very end of the line.
    DanglingEscape,
    /// A JSON value cut short by the end of the line; `rest` is the line
    /// from the value on.
    UnclosedJson {
        key: &'s str,
        open: char,
        rest: &'s str,
    },
    /// A JSON value serde_json rejects, with its reason.
    InvalidJson { key: &'s str, reason: String },
    /// A JSON value followed by more text; `text` is the value with it.
    TextAfterJson { key: &'s str, text: &'s str },
}

impl Problem<'_> {
    /// The diagnostic for this problem: the one place the edit grammar's
    /// failures become messages. Every problem names its text and the form
    /// expected there.
    fn diagnostic(self) -> Diagnostic {
        let (message, expected): (String, Cow<'static, str>) = match self {
            Problem::Empty => ("empty edit".into(), LINE_FORM.into()),
            Problem::NoTarget(word) => (
                format!("no target before `{word}`"),
                "a target first: a layer id or `scene`".into(),
            ),
            Problem::NoChanges(target) => {
                (format!("no changes after `{target}`"), CHANGE_FORM.into())
            }
            Problem::NotAChange(word) => (
                format!("`{word}` is not a change"),
                "`<key>=<value>`, with quotes around a value that has spaces".into(),
            ),
            Problem::StrayQuote(before) => (
                format!("unexpected quote in `{before}\"`"),
                "quotes only around a value, as in `<key>=\"<value>\"`".into(),
            ),
            Problem::Misquoted {
                written,
                key,
                meant,
            } => {
                // Escaped, so the suggestion can be pasted back as it is.
                let meant = meant.replace('\\', "\\\\").replace('"', "\\\"");
                (
                    format!("unexpected quote in `{written}`"),
                    format!("the whole value quoted: {key}=\"{meant}\"").into(),
                )
            }
            Problem::Operator(op) => (
                format!("operator `{op}=` is not supported yet"),
                CHANGE_FORM.into(),
            ),
            Problem::MissingKey(word) => (
                format!("missing key before `=` in `{word}`"),
                CHANGE_FORM.into(),
            ),
            Problem::InvalidKey(key) => (
                format!(
                    "invalid key `{key}`: each dot-separated part must be non-empty letters, digits, `-` or `_`"
                ),
                PATH_FORM.into(),
            ),
            Problem::MissingValue(key) => (
                format!("missing value for `{key}`"),
                format!("`{key}=<value>`, or `{key}=\"\"` for an empty string").into(),
            ),
            Problem::UnclosedQuote { key, rest } => (
                format!("unclosed quote in `{key}={rest}`"),
                "a closing `\"`".into(),
            ),
            Problem::TextAfterQuote { key, text } => (
                format!("unexpected text after the closing quote in `{key}={text}`"),
                "a space after the quote, or the whole value quoted".into(),
            ),
            Problem::Escape(other) => (
                format!("unknown escape `\\{other}` in a quoted value"),
                ESCAPES.into(),
            ),
            Problem::DanglingEscape => (
                "dangling `\\` at the end of a quoted value".into(),
                ESCAPES.into(),
            ),
            Problem::UnclosedJson { key, open, rest } => (
                format!("unclosed `{open}` in `{key}={rest}`"),
                JSON_FORM.into(),
            ),
            Problem::InvalidJson { key, reason } => (
                format!("invalid JSON in `{key}`: {reason}"),
                JSON_FORM.into(),
            ),
            Problem::TextAfterJson { key, text } => (
                format!("unexpected text after the end of the JSON value in `{key}={text}`"),
                "a space after the closing bracket".into(),
            ),
        };
        Diagnostic::syntax(message).expected(expected)
    }
}

/// The whole line: the target, then each change after a space.
fn edit<'s>(input: &mut &'s str) -> Parsed<Edit, Problem<'s>> {
    delimited(
        blank0,
        (
            target,
            // A change starts at the next word; trailing space is not one.
            repeat(0.., preceded((blank1, peek(any)), cut_err(change))),
        ),
        blank0,
    )
    .try_map(|(target, changes): (&str, Vec<Change>)| {
        if changes.is_empty() {
            Err(Problem::NoChanges(target))
        } else {
            Ok(Edit {
                target: target.to_owned(),
                changes,
            })
        }
    })
    .parse_next(input)
}

/// True for the characters that end a target or a key: a space, the `=`
/// before a value, or a quote, which only goes around a value.
fn ends_key(c: char) -> bool {
    c.is_whitespace() || c == '=' || c == '"'
}

/// The target: a layer id or `scene`, a word with no `=` or `"` in it.
fn target<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem<'s>> {
    dispatch! {(peek(opt(word)), take_till(0.., ends_key), opt(one_of(['=', '"'])));
        // The line is empty: the one failure without a problem.
        (None, _, _) => fail,
        (Some(word), _, Some('=')) => cut_err(fail).context(Problem::NoTarget(word)),
        (_, before, Some('"')) => cut_err(fail).context(Problem::StrayQuote(before)),
        (_, name, _) => empty.value(name),
    }
    .parse_next(input)
}

/// One change, `path=value`. Any other word is a problem that says what it
/// is instead.
fn change<'s>(input: &mut &'s str) -> Parsed<Change, Problem<'s>> {
    dispatch! {(peek(word), take_till(0.., ends_key), opt(one_of(['=', '"'])));
        (written, "", Some('=')) => cut_err(fail).context(Problem::MissingKey(written)),
        // The key was read up to its `=`; `and_then` reads it as a path.
        (written, key, Some('=')) => (empty.value(key).and_then(path), value(key, written))
            .map(|(key, value)| Change { key, value }),
        (_, before, Some('"')) => cut_err(fail).context(Problem::StrayQuote(before)),
        (_, word, _) => cut_err(fail).context(Problem::NotAChange(word)),
    }
    .parse_next(input)
}

/// A key, the whole of the input: a dotted path of names. A key that ends
/// in an operator (`delay+` of `delay+=10f`) is that operator, even `-`,
/// which a name may otherwise contain.
fn path<'s>(input: &mut &'s str) -> Parsed<KeyPath, Problem<'s>> {
    let text = *input;
    preceded(
        peek(opt(ends_with(one_of(OPERATORS)))).try_map(|op| match op {
            Some(op) => Err(Problem::Operator(op)),
            None => Ok(()),
        }),
        terminated(separated(1.., segment.map(str::to_owned), '.'), eof)
            .context(Problem::InvalidKey(text)),
    )
    .map(|segments| KeyPath {
        text: text.to_owned(),
        segments,
    })
    .parse_next(input)
}

/// One name of a path: letters, digits, `-` and `_`.
fn segment<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem<'s>> {
    take_while(1.., |c: char| c.is_alphanumeric() || c == '-' || c == '_').parse_next(input)
}

/// The value of the change to `key`, told apart by its first character;
/// `written` is the change's word, for messages.
fn value<'s>(key: &'s str, written: &'s str) -> impl Parser<&'s str, EditValue, Error<'s>> {
    move |input: &mut &'s str| {
        let rest = *input;
        dispatch! {peek(opt(any));
            Some('"') => closed(quoted, |text| Problem::TextAfterQuote { key, text })
                .map(|text| EditValue::Set(Value::String(text)))
                .context(Problem::UnclosedQuote { key, rest }),
            Some(open @ ('[' | '{')) => closed(json(key, open), |text| Problem::TextAfterJson { key, text })
                .map(EditValue::Set),
            Some(c) if !c.is_whitespace() => bare(key, written),
            _ => fail.context(Problem::MissingValue(key)),
        }
        .parse_next(input)
    }
}

/// `value`, which must end its word: text straight after its closing quote
/// or bracket is the problem `after` makes from the value with that text.
fn closed<'s, T>(
    value: impl Parser<&'s str, T, Error<'s>>,
    after: impl Fn(&'s str) -> Problem<'s>,
) -> impl Parser<&'s str, T, Error<'s>> {
    (value, opt(word))
        .with_taken()
        .try_map(move |((value, trailing), text)| match trailing {
            None => Ok(value),
            Some(_) => Err(after(text)),
        })
}

/// A quoted string, with `\"` and `\\` decoded.
fn quoted<'s>(input: &mut &'s str) -> Parsed<String, Problem<'s>> {
    delimited('"', repeat(0.., alt((none_of(['"', '\\']), escape))), '"').parse_next(input)
}

/// An escape in a quoted value: a `\` and the character it stands for.
fn escape<'s>(input: &mut &'s str) -> Parsed<char, Problem<'s>> {
    preceded(
        '\\',
        cut_err(any.context(Problem::DanglingEscape).try_map(|c| match c {
            '"' | '\\' => Ok(c),
            other => Err(Problem::Escape(other)),
        })),
    )
    .parse_next(input)
}

/// A JSON array or object for `key` that starts with `open`. serde_json
/// reads it and stops at its closing bracket without looking further, so
/// whatever follows is left to the line's grammar.
fn json<'s>(key: &'s str, open: char) -> impl Parser<&'s str, Value, Error<'s>> {
    move |input: &mut &'s str| {
        let rest = *input;
        let mut values = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        let read = match values.next() {
            Some(Ok(value)) => Ok(value),
            Some(Err(error)) if !error.is_eof() => Err(Problem::InvalidJson {
                key,
                reason: error.to_string(),
            }),
            // The end of the line came first. (`None` means there was no
            // value at all, which cannot happen after an opening bracket.)
            _ => Err(Problem::UnclosedJson { key, open, rest }),
        };
        let value = read.map_err(|problem| ErrMode::from_external_error(input, problem).cut())?;
        input.next_slice(values.byte_offset());
        Ok(value)
    }
}

/// A bare value: the text up to the next space. A quote inside it is a
/// mistake, and the problem carries the whole value as its author meant it,
/// so the message can suggest quoting it.
fn bare<'s>(key: &'s str, written: &'s str) -> impl Parser<&'s str, EditValue, Error<'s>> {
    (
        take_till(1.., |c: char| c.is_whitespace() || c == '"'),
        opt(meant),
    )
        .try_map(move |(text, misquoted)| match misquoted {
            None => Ok(scalar(text)),
            Some(rest) => Err(Problem::Misquoted {
                written,
                key,
                meant: format!("{text}{rest}"),
            }),
        })
}

/// The rest of a word from a stray quote, as its author meant it: quotes
/// group text, spaces included. It needs at least one character: after a
/// bare run, only a quote can start one, so a word that simply ends reads
/// as `None`.
fn meant<'s>(input: &mut &'s str) -> Parsed<String, Problem<'s>> {
    let group = preceded('"', terminated(repeat(0.., meant_char), opt('"')));
    let run = take_till(1.., |c: char| c.is_whitespace() || c == '"').map(str::to_owned);
    repeat(1.., alt((group, run)))
        .fold(String::new, |mut whole, part: String| {
            whole.push_str(&part);
            whole
        })
        .parse_next(input)
}

/// One character inside a misquoted group, as its author meant it. Unlike
/// [`escape`], it never fails: a `\` takes whatever character follows as it
/// is, since the suggestion only needs a best reading of the text.
fn meant_char<'s>(input: &mut &'s str) -> Parsed<char, Problem<'s>> {
    alt((preceded('\\', any), none_of(['"', '\\']))).parse_next(input)
}

/// What an unquoted value means: `none` removes the key, `true` and `false`
/// are booleans, a number is a number, and anything else is a string.
fn scalar(text: &str) -> EditValue {
    match text {
        "none" => EditValue::Remove,
        "true" => EditValue::Set(Value::Bool(true)),
        "false" => EditValue::Set(Value::Bool(false)),
        _ => EditValue::Set(number(text).unwrap_or_else(|| Value::from(text))),
    }
}

/// `text` as a JSON number. Integers stay integers (so `fps=60` is `60`, not
/// `60.0`); other numbers become floats. NaN and infinity are not JSON
/// numbers and stay strings, as do words like `12f` or `#FFF`.
fn number(text: &str) -> Option<Value> {
    text.parse::<i64>().map(Value::from).ok().or_else(|| {
        text.parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
    })
}

#[cfg(test)]
#[path = "../tests/unit/parse.rs"]
mod tests;
