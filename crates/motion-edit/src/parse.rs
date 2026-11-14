//! The `winnow` grammar that reads an edit line into an [`Edit`]. It knows
//! nothing about scenes: whether a key fits its target is checked in `apply`.

use motion_format::edit_word::{FALSE, NONE, TRUE};
use motion_format::grammar::{Failure, Parsed, blank0, blank1, ends_with, parse_all, word};
use motion_format::{Diagnostic, Kind, Result};
use serde_json::{Number, Value};
use winnow::combinator::{
    alt, cut_err, delimited, dispatch, empty, eof, fail, opt, peek, preceded, repeat, separated,
    terminated,
};
use winnow::error::{ErrMode, FromExternalError};
use winnow::prelude::*;
use winnow::stream::Stream;
use winnow::token::{any, none_of, one_of, take_till, take_while};

use crate::key_path::KeyPath;
use crate::problem::Problem;

/// A parsed edit: one target and the changes to make to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    /// A layer id, or `scene` for the top level of the document.
    pub target: String,
    /// The changes, in the order written; no two write the same place.
    pub changes: Vec<Change>,
}

/// One `key=value` pair of an edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// The key to change, such as `fill` or `anim.opacity`.
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

/// Parses one edit line such as `card fill=#FF0000 in=none`.
///
/// # Errors
/// A [`Kind::Syntax`] diagnostic for a line that does not follow the grammar,
/// or a [`Kind::Conflict`] one for two changes that write the same place.
pub fn parse_edit(line: &str) -> Result<Edit> {
    // Only an empty line fails without naming a problem.
    let edit = parse_all(line, edit, |problem| {
        problem.unwrap_or(Problem::Empty).diagnostic()
    })?;
    edit.check_conflicts()?;
    Ok(edit)
}

impl Edit {
    /// Rejects two changes that write the same place, such as `anim` and
    /// `anim.x`: which one won would depend on their order.
    fn check_conflicts(&self) -> Result<()> {
        let conflict = self.changes.iter().enumerate().find_map(|(i, later)| {
            self.changes[..i]
                .iter()
                .find(|earlier| earlier.key.overlaps(&later.key))
                .map(|earlier| (&earlier.key, &later.key))
        });
        let Some((a, b)) = conflict else {
            return Ok(());
        };
        let (outer, inner) = if a.segments().len() <= b.segments().len() {
            (a, b)
        } else {
            (b, a)
        };
        let message = if outer == inner {
            format!("key `{outer}` given twice")
        } else {
            format!(
                "keys `{outer}` and `{inner}` overlap; set them in separate edits or merge them into one value"
            )
        };
        Err(Diagnostic::new(Kind::Conflict, message))
    }
}

/// The operators people try before `=`; only `=` is supported for now.
const OPERATORS: [char; 7] = ['+', '-', '*', '/', '<', '>', '!'];

/// What the edit grammar fails with.
type Error<'s> = ErrMode<Failure<Problem<'s>>>;

/// The whole line: the target, then each change after a space.
fn edit<'s>(input: &mut &'s str) -> Parsed<Edit, Problem<'s>> {
    delimited(
        blank0,
        (
            target,
            // Trailing space does not start a change.
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

/// Ends a target or a key; a quote only goes around a value.
fn ends_key(c: char) -> bool {
    c.is_whitespace() || c == '=' || c == '"'
}

/// Ends a bare value.
fn ends_bare(c: char) -> bool {
    c.is_whitespace() || c == '"'
}

/// The target: a layer id or `scene`.
fn target<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem<'s>> {
    dispatch! {(peek(opt(word)), take_till(0.., ends_key), opt(one_of(['=', '"'])));
        // The one failure without a problem: the line is empty.
        (None, _, _) => fail,
        (Some(word), _, Some('=')) => cut_err(fail).context(Problem::NoTarget(word)),
        (_, before, Some('"')) => cut_err(fail).context(Problem::StrayQuote(before)),
        (_, name, _) => empty.value(name),
    }
    .parse_next(input)
}

/// One change, `path=value`; any other word is the problem it is.
fn change<'s>(input: &mut &'s str) -> Parsed<Change, Problem<'s>> {
    dispatch! {(peek(word), take_till(0.., ends_key), opt(one_of(['=', '"'])));
        (written, "", Some('=')) => cut_err(fail).context(Problem::MissingKey(written)),
        (written, key, Some('=')) => (empty.value(key).and_then(path), value(key, written))
            .map(|(key, value)| Change { key, value }),
        (_, before, Some('"')) => cut_err(fail).context(Problem::StrayQuote(before)),
        (_, word, _) => cut_err(fail).context(Problem::NotAChange(word)),
    }
    .parse_next(input)
}

/// A key, the whole of the input: a dotted path of names. A key that ends in
/// an operator (`delay+` of `delay+=10f`) is that operator, even `-`, which a
/// name may otherwise contain.
pub(crate) fn path<'s>(input: &mut &'s str) -> Parsed<KeyPath, Problem<'s>> {
    let text = *input;
    preceded(
        peek(opt(ends_with(one_of(OPERATORS)))).try_map(|op| match op {
            Some(op) => Err(Problem::Operator(op)),
            None => Ok(()),
        }),
        terminated(separated(1.., segment.map(str::to_owned), '.'), eof)
            .context(Problem::InvalidKey(text)),
    )
    .map(|segments| KeyPath::new(text, segments))
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

/// `value`, which must end its word; text straight after it is the problem
/// `after` makes from the value with that text.
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

/// A JSON array or object for `key` that starts with `open`. serde_json stops
/// at its closing bracket, so whatever follows is left to the line's grammar.
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
            // The line ended first; `None` cannot follow an opening bracket.
            _ => Err(Problem::UnclosedJson { key, open, rest }),
        };
        let value = read.map_err(|problem| ErrMode::from_external_error(input, problem).cut())?;
        input.next_slice(values.byte_offset());
        Ok(value)
    }
}

/// A bare value: the text up to the next space. A quote inside it is a
/// mistake whose problem suggests the whole value quoted.
fn bare<'s>(key: &'s str, written: &'s str) -> impl Parser<&'s str, EditValue, Error<'s>> {
    (take_till(1.., ends_bare), opt(meant)).try_map(move |(text, misquoted)| match misquoted {
        None => Ok(scalar(text)),
        Some(rest) => Err(Problem::Misquoted {
            written,
            key,
            meant: format!("{text}{rest}"),
        }),
    })
}

/// The rest of a word from a stray quote, as its author meant it: quotes
/// group text, spaces included. A word that simply ends reads as `None`.
fn meant<'s>(input: &mut &'s str) -> Parsed<String, Problem<'s>> {
    let group = preceded('"', terminated(repeat(0.., meant_char), opt('"')));
    let run = take_till(1.., ends_bare).map(str::to_owned);
    repeat(1.., alt((group, run)))
        .fold(String::new, |mut whole, part: String| {
            whole.push_str(&part);
            whole
        })
        .parse_next(input)
}

/// One character of a misquoted group. Unlike [`escape`] it never fails: the
/// suggestion only needs a best reading of the text.
fn meant_char<'s>(input: &mut &'s str) -> Parsed<char, Problem<'s>> {
    alt((preceded('\\', any), none_of(['"', '\\']))).parse_next(input)
}

/// What a bare value means: `none`, a boolean, a number, or else a string.
fn scalar(text: &str) -> EditValue {
    match text {
        NONE => EditValue::Remove,
        TRUE => EditValue::Set(Value::Bool(true)),
        FALSE => EditValue::Set(Value::Bool(false)),
        _ => EditValue::Set(number(text).unwrap_or_else(|| Value::from(text))),
    }
}

/// `text` as a JSON number: integers stay integers (`fps=60` is not `60.0`).
/// NaN and infinity are not JSON numbers, so they stay strings.
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
