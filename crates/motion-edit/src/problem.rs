//! What can be wrong with the syntax of an edit line, and the one place it
//! becomes a [`Diagnostic`].
//!
//! Part of the edit side branch (not a numbered rendering step). The grammar
//! in `crate::parse` names a [`Problem`] where it fails, and
//! [`Problem::diagnostic`] writes its message and the form expected there.

use std::borrow::Cow;

use motion_format::Diagnostic;

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

/// What is wrong with the syntax of an edit line, with the text it is
/// wrong about.
#[derive(Debug, Clone)]
pub(crate) enum Problem<'s> {
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
    pub(crate) fn diagnostic(self) -> Diagnostic {
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

#[cfg(test)]
#[path = "../tests/unit/problem.rs"]
mod tests;
