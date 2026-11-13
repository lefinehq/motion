//! Parsing of one-line edits into typed [`Edit`] values.
//!
//! A side branch working on the raw document from step 1 (not a numbered rendering
//! step). It only reads text; it knows
//! nothing about scenes. Whether a key is valid for its target is checked
//! later, in `apply`.
//!
//! Grammar: `<target> <key>=<value> [<key>=<value> ...]`, where a value is a
//! quoted string (`\"` and `\\` escapes), `none` (remove the key), a number,
//! `true`/`false`, or any other bare word (a string). A quote is only
//! allowed as the first character of a value. A key may be a dotted path
//! (`anim.opacity`), and a value starting with `[` or `{` is JSON, read up
//! to its balanced end (strings inside may contain spaces and brackets).

use motion_diagnostic::{Diagnostic, Kind, Result};
use serde_json::{Number, Value};

/// A parsed edit: one target and the changes to make to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    /// A layer id, or `scene` for the top level of the document.
    pub target: String,
    /// The changes, in the order they were written.
    pub changes: Vec<Change>,
}

/// One `key=value` pair of an edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// The key to change, e.g. `fill`, or a dotted path such as `anim.opacity`.
    pub key: String,
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
/// Returns a [`Diagnostic`] for an empty line, an unclosed quote, a quote
/// that does not start a value, a missing target or changes, a word that is
/// not `key=value`, an unsupported operator, an empty key, an empty path
/// segment, an empty value, invalid or unbalanced JSON, a key given twice, or
/// two keys where one is a dotted prefix of the other.
pub fn parse_edit(line: &str) -> Result<Edit> {
    let mut words = split_words(line)?.into_iter();
    let target = words
        .next()
        .ok_or_else(|| Diagnostic::syntax("empty edit; expected `<target> <key>=<value>`"))?;
    if target.head.contains('=') {
        return Err(Diagnostic::syntax(format!(
            "expected a target (a layer id or `scene`) before `{}`",
            target.shown()
        )));
    }
    let target = target.head;
    let mut changes: Vec<Change> = Vec::new();
    for word in words {
        let change = parse_change(&word)?;
        if changes.iter().any(|c| c.key == change.key) {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!("key `{}` given twice", change.key),
            ));
        }
        // `anim` and `anim.x` would write the same place twice, and which
        // one wins would depend on their order.
        if let Some(other) = changes.iter().find(|c| overlaps(&c.key, &change.key)) {
            let (outer, inner) = if other.key.len() <= change.key.len() {
                (&other.key, &change.key)
            } else {
                (&change.key, &other.key)
            };
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!(
                    "keys `{outer}` and `{inner}` overlap; set them in separate edits or merge them into one value"
                ),
            ));
        }
        changes.push(change);
    }
    if changes.is_empty() {
        return Err(Diagnostic::syntax(format!(
            "no changes after `{target}`; expected `<key>=<value>`"
        )));
    }
    Ok(Edit { target, changes })
}

/// True when one dotted key is a prefix of the other, segment by segment.
fn overlaps(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    long == short
        || long
            .strip_prefix(short)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// One whitespace-separated word of an edit line, split at its first `=`.
#[derive(Debug, Default)]
struct Word {
    /// The text up to and including the first `=`; the whole word if it has
    /// no `=` (a target, or a mistake).
    head: String,
    /// The text after the `=`, already unescaped when `quoted`.
    value: String,
    /// True if the value was written in double quotes. A quoted value is
    /// always a string, so `"none"` and `"60"` do not become a removal or a
    /// number.
    quoted: bool,
    /// The parsed value when it was written as JSON (`[…]` or `{…}`); `value`
    /// then holds the raw text.
    json: Option<Value>,
}

impl Word {
    /// The word as the author wrote it, for error messages.
    fn shown(&self) -> String {
        if self.quoted {
            format!("{}\"{}\"", self.head, self.value)
        } else {
            format!("{}{}", self.head, self.value)
        }
    }
}

/// Splits a line into [`Word`]s. Whitespace separates words except inside a
/// quoted value, and a quote is only allowed as the first character of a
/// value (right after the first `=`). This is the only place that decodes a
/// quoted value.
fn split_words(line: &str) -> Result<Vec<Word>> {
    let mut words = Vec::new();
    let mut current: Option<Word> = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            words.extend(current.take());
            continue;
        }
        let word = current.get_or_insert_with(Word::default);
        let value_started = word.head.ends_with('=');
        match c {
            // A quote opens a value only straight after the first `=`.
            '"' if value_started && word.value.is_empty() && !word.quoted => {
                word.value = read_quoted(&mut chars, &word.head)?;
                word.quoted = true;
                // Nothing may follow the closing quote but a space.
                if let Some(next) = chars.peek().filter(|n| !n.is_whitespace()) {
                    return Err(Diagnostic::syntax(format!(
                        "unexpected text after the closing quote in `{}{next}`; \
                         put a space after the quote, or quote the whole value",
                        word.shown()
                    )));
                }
            }
            // `[` or `{` opens a JSON value only straight after the first `=`.
            '[' | '{' if value_started && word.value.is_empty() && !word.quoted => {
                let (raw, value) = read_json(c, &mut chars, &word.head)?;
                word.value = raw;
                word.json = Some(value);
                // Nothing may follow the balanced end but a space.
                if let Some(next) = chars.peek().filter(|n| !n.is_whitespace()) {
                    return Err(Diagnostic::syntax(format!(
                        "unexpected text after the end of the JSON value in `{}{next}`; \
                         put a space after it; {JSON_FORM}",
                        word.shown()
                    )));
                }
            }
            '"' => return Err(stray_quote(word, &mut chars)),
            c if word.head.contains('=') => word.value.push(c),
            c => word.head.push(c),
        }
    }
    words.extend(current);
    Ok(words)
}

/// The expected form shown with every JSON value error.
const JSON_FORM: &str = "expected `<key>=[…]` or `<key>={…}`, valid JSON";

/// Reads a JSON value whose opening bracket `open` was just consumed, up to
/// its balanced end, skipping brackets inside JSON strings (with escapes).
/// Returns the raw text and the parsed value. `head` (`key=`) is for messages.
fn read_json(
    open: char,
    chars: &mut impl Iterator<Item = char>,
    head: &str,
) -> Result<(String, Value)> {
    let mut raw = String::from(open);
    let (mut depth, mut in_string, mut escaped) = (1usize, false, false);
    for c in chars {
        raw.push(c);
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    let key = head.strip_suffix('=').unwrap_or(head);
                    let value = serde_json::from_str(&raw).map_err(|e| {
                        Diagnostic::syntax(format!("invalid JSON in `{key}`: {e}; {JSON_FORM}"))
                    })?;
                    return Ok((raw, value));
                }
            }
            _ => {}
        }
    }
    Err(Diagnostic::syntax(format!(
        "unclosed `{open}` in `{head}{raw}`; {JSON_FORM}"
    )))
}

/// Reads a quoted value after its opening quote, up to the closing quote,
/// decoding `\"` and `\\`. `head` (`key=`) is only used in messages.
fn read_quoted(chars: &mut impl Iterator<Item = char>, head: &str) -> Result<String> {
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(e @ ('"' | '\\')) => out.push(e),
                Some(other) => {
                    return Err(Diagnostic::syntax(format!(
                        "unknown escape `\\{other}` in a quoted value; only `\\\"` and `\\\\` are supported"
                    )));
                }
                None => {
                    return Err(Diagnostic::syntax(
                        "dangling `\\` at the end of a quoted value",
                    ));
                }
            },
            '"' => return Ok(out),
            c => out.push(c),
        }
    }
    Err(Diagnostic::syntax(format!(
        "unclosed quote in `{head}\"{out}`; add a closing `\"`"
    )))
}

/// The error for a `"` that does not start a value: shows the word up to the
/// next space and how to write the whole thing as one quoted value.
fn stray_quote(word: &Word, chars: &mut impl Iterator<Item = char>) -> Diagnostic {
    let so_far = format!("{}{}", word.head, word.value);
    // Without a `=` there is no key to build a suggestion from.
    let Some((key, _)) = so_far.split_once('=') else {
        return Diagnostic::syntax(format!(
            "unexpected quote in `{so_far}\"`; quotes only go around a value, as in `<key>=\"<value>\"`"
        ));
    };
    // Walk the rest of the word, following its quotes, to build the
    // suggestion. `shown` stops at the first space so the message points at
    // the exact token; `whole` keeps going to the real end of the word.
    let mut shown = format!("{so_far}\"");
    let mut whole = so_far[key.len() + 1..].to_string();
    let (mut shown_open, mut inside) = (true, true);
    while let Some(c) = chars.next() {
        match c {
            '\\' if inside => {
                if let Some(e) = chars.next() {
                    whole.push(e);
                    if shown_open {
                        shown.extend([c, e]);
                    }
                }
            }
            '"' => {
                inside = !inside;
                if shown_open {
                    shown.push(c);
                }
            }
            c if c.is_whitespace() => {
                if !inside {
                    break;
                }
                shown_open = false;
                whole.push(c);
            }
            c => {
                whole.push(c);
                if shown_open {
                    shown.push(c);
                }
            }
        }
    }
    // Escape the suggestion so it can be pasted back as-is.
    let whole = whole.replace('\\', "\\\\").replace('"', "\\\"");
    Diagnostic::syntax(format!(
        "unexpected quote in `{shown}`; quote the whole value: {key}=\"{whole}\""
    ))
}

/// Parses one `key=value` word.
fn parse_change(word: &Word) -> Result<Change> {
    let Some(key) = word.head.strip_suffix('=') else {
        return Err(Diagnostic::syntax(format!(
            "expected `<key>=<value>`, got `{}`; quote values that contain spaces",
            word.head
        )));
    };
    // `delay+=10f` splits as key `delay+`; only `=` is supported for now.
    if let Some(op) = key.chars().last().filter(|c| "+-*/<>!".contains(*c)) {
        return Err(Diagnostic::syntax(format!(
            "operator `{op}=` is not supported yet; use `=`"
        )));
    }
    if key.is_empty() {
        return Err(Diagnostic::syntax(format!(
            "missing key before `=` in `{}`; expected `<key>=<value>`",
            word.shown()
        )));
    }
    // Every segment of a dotted path must be a plain name.
    if key
        .split('.')
        .any(|seg| seg.is_empty() || !seg.chars().all(|c| c.is_alphanumeric() || "-_".contains(c)))
    {
        return Err(Diagnostic::syntax(format!(
            "invalid key `{key}`: each dot-separated part must be non-empty letters, digits, `-` or `_`; expected `<key>[.<key>…]=<value>`"
        )));
    }
    if word.value.is_empty() && !word.quoted {
        return Err(Diagnostic::syntax(format!(
            "missing value for `{key}`; use `{key}=\"\"` for an empty string"
        )));
    }
    Ok(Change {
        key: key.to_string(),
        value: if let Some(json) = &word.json {
            EditValue::Set(json.clone())
        } else if word.quoted {
            EditValue::Set(Value::String(word.value.clone()))
        } else {
            parse_bare(&word.value)
        },
    })
}

/// Turns an unquoted value into an [`EditValue`].
fn parse_bare(raw: &str) -> EditValue {
    match raw {
        "none" => EditValue::Remove,
        "true" => EditValue::Set(Value::Bool(true)),
        "false" => EditValue::Set(Value::Bool(false)),
        _ => EditValue::Set(parse_number(raw).unwrap_or_else(|| Value::String(raw.to_string()))),
    }
}

/// Reads a number token. Integers stay JSON integers (so `fps=60` is `60`,
/// not `60.0`); tokens with a `.` or exponent become floats. NaN and
/// infinity are not numbers here and fall back to strings, as do words like
/// `12f` or `#FFF`.
fn parse_number(raw: &str) -> Option<Value> {
    let first = raw.chars().next()?;
    if !(first.is_ascii_digit() || matches!(first, '-' | '+' | '.')) {
        return None;
    }
    if let Ok(i) = raw.parse::<i64>() {
        return Some(Value::from(i));
    }
    let f: f64 = raw.parse().ok()?;
    Number::from_f64(f).map(Value::Number)
}

#[cfg(test)]
#[path = "../tests/unit/parse.rs"]
mod tests;
