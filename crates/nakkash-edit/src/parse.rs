//! The `winnow` grammar that reads an edit line into an [`Edit`]. It knows
//! nothing about scenes: whether a key fits its target is checked in `apply`.
//!
//! ```text
//! line    = target (ws change)+
//! target  = "scene" | id ("/" id)*      a layer at any depth: `card/title`
//! change  = path "=" value
//! path    = key select? ("." key select?)*
//! key     = letters, digits, `-` and `_`
//! select  = "[" id "]" | "[" index "]"  a list element by its id or index
//! value   = quoted | json | bare
//! quoted  = '"' (char | escape)* '"'
//! escape  = '\"' | '\\' | '\n' (newline) | '\t' (tab)
//! json    = a JSON array or object, read by serde_json up to its closing bracket
//! bare    = text up to the next space: `none`, `true`, `false`, a number, or a string
//! ```

use nakkash_format::address::{id, key_path, target as layers};
use nakkash_format::edit_word::{FALSE, NONE, TRUE};
use nakkash_format::grammar::{Failure, Parsed, blank0, blank1, ends_with, parse_all, word};
use nakkash_format::{Address, Diagnostic, Kind, Result, Step};
use serde_json::{Number, Value};
use winnow::combinator::{
    alt, cut_err, delimited, dispatch, empty, eof, fail, opt, peek, preceded, repeat, terminated,
};
use winnow::error::{ErrMode, FromExternalError};
use winnow::prelude::*;
use winnow::stream::Stream;
use winnow::token::{any, none_of, one_of, take_till};

use crate::key_path::KeyPath;
use crate::problem::Problem;

/// A parsed edit: one target and the changes to make to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    /// `scene` for the top level of the document, or a layer at any depth:
    /// `title`, `card/title`. It has no keys.
    pub target: Address,
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
    // A line that starts with a slot edits the scene's: `$accent=#fff` is
    // `scene $accent=#fff`.
    let scened;
    let line = match line.trim_start().starts_with('$') {
        true => {
            scened = format!("scene {line}");
            &scened
        }
        false => line,
    };
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
        let (outer, inner) = if a.steps().len() <= b.steps().len() {
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
    .try_map(
        |((written, target), changes): ((&str, Address), Vec<Change>)| {
            if changes.is_empty() {
                Err(Problem::NoChanges(written))
            } else {
                Ok(Edit { target, changes })
            }
        },
    )
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

/// The target, as written and read: `scene`, or a layer address.
fn target<'s>(input: &mut &'s str) -> Parsed<(&'s str, Address), Problem<'s>> {
    dispatch! {(peek(opt(word)), take_till(0.., ends_key), opt(one_of(['=', '"'])));
        // The one failure without a problem: the line is empty.
        (None, _, _) => fail,
        (Some(word), _, Some('=')) => cut_err(fail).context(Problem::NoTarget(word)),
        (_, before, Some('"')) => cut_err(fail).context(Problem::StrayQuote(before)),
        (_, name, _) => cut_err(empty.value(name).try_map(|name: &'s str| {
            let mut rest = name;
            match terminated(layers::<()>, eof).parse_next(&mut rest) {
                Ok(ids) => Ok((name, Address::layer(ids))),
                Err(_) => Err(Problem::InvalidTarget(name)),
            }
        })),
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
    if text.starts_with('$') {
        // A slot is one key of its own: `$accent`, never `$accent.x`.
        return terminated(('$', id).take(), eof)
            .context(Problem::InvalidSlot(text))
            .map(|slot: &str| KeyPath::new(slot, vec![Step::Key(slot.to_owned())]))
            .parse_next(input);
    }
    preceded(
        peek(opt(ends_with(one_of(OPERATORS)))).try_map(|op| match op {
            Some(op) => Err(Problem::Operator(op)),
            None => Ok(()),
        }),
        terminated(key_path, eof).context(Problem::InvalidKey(text)),
    )
    .map(|steps| KeyPath::new(text, steps))
    .parse_next(input)
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

/// A quoted string, with its [`ESCAPES`] decoded.
fn quoted<'s>(input: &mut &'s str) -> Parsed<String, Problem<'s>> {
    delimited('"', repeat(0.., alt((none_of(['"', '\\']), escape))), '"').parse_next(input)
}

/// The escapes of a quoted value, as the character written after its `\`
/// and the character it stands for.
pub(crate) const ESCAPES: &[(char, char)] = &[('"', '"'), ('\\', '\\'), ('n', '\n'), ('t', '\t')];

/// An escape in a quoted value: a `\` and a character from [`ESCAPES`].
fn escape<'s>(input: &mut &'s str) -> Parsed<char, Problem<'s>> {
    preceded(
        '\\',
        cut_err(any.context(Problem::DanglingEscape).try_map(|c| {
            ESCAPES
                .iter()
                .find_map(|&(written, meant)| (written == c).then_some(meant))
                .ok_or(Problem::Escape(c))
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
