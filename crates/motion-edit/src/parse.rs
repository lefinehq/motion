//! Parsing of one-line edits into typed [`Edit`] values.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). It only reads text; it knows
//! nothing about scenes. Whether a key is valid for its target is checked
//! later, in `apply`.
//!
//! Grammar: `<target> <key>=<value> [<key>=<value> ...]`, where a value is a
//! quoted string (`\"` and `\\` escapes), `none` (remove the key), a number,
//! `true`/`false`, or any other bare word (a string). A quote is only
//! allowed as the first character of a value.

use crate::EditError;
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
    /// The key to change, e.g. `fill`.
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
/// Returns an [`EditError`] for an empty line, an unclosed quote, a quote
/// that does not start a value, a missing target or changes, a word that is
/// not `key=value`, an unsupported operator, an empty key or value, or a key
/// given twice.
pub fn parse_edit(line: &str) -> Result<Edit, EditError> {
    let mut words = split_words(line)?.into_iter();
    let target = words
        .next()
        .ok_or_else(|| EditError::new("empty edit; expected `<target> <key>=<value>`"))?;
    if target.head.contains('=') {
        return Err(EditError::new(format!(
            "expected a target (a layer id or `scene`) before `{}`",
            target.shown()
        )));
    }
    let target = target.head;
    let mut changes: Vec<Change> = Vec::new();
    for word in words {
        let change = parse_change(&word)?;
        if changes.iter().any(|c| c.key == change.key) {
            return Err(EditError::new(format!("key `{}` given twice", change.key)));
        }
        changes.push(change);
    }
    if changes.is_empty() {
        return Err(EditError::new(format!(
            "no changes after `{target}`; expected `<key>=<value>`"
        )));
    }
    Ok(Edit { target, changes })
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
fn split_words(line: &str) -> Result<Vec<Word>, EditError> {
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
                    return Err(EditError::new(format!(
                        "unexpected text after the closing quote in `{}{next}`; \
                         put a space after the quote, or quote the whole value",
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

/// Reads a quoted value after its opening quote, up to the closing quote,
/// decoding `\"` and `\\`. `head` (`key=`) is only used in messages.
fn read_quoted(chars: &mut impl Iterator<Item = char>, head: &str) -> Result<String, EditError> {
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(e @ ('"' | '\\')) => out.push(e),
                Some(other) => {
                    return Err(EditError::new(format!(
                        "unknown escape `\\{other}` in a quoted value; only `\\\"` and `\\\\` are supported"
                    )));
                }
                None => return Err(EditError::new("dangling `\\` at the end of a quoted value")),
            },
            '"' => return Ok(out),
            c => out.push(c),
        }
    }
    Err(EditError::new(format!(
        "unclosed quote in `{head}\"{out}`; add a closing `\"`"
    )))
}

/// The error for a `"` that does not start a value: shows the word up to the
/// next space and how to write the whole thing as one quoted value.
fn stray_quote(word: &Word, chars: &mut impl Iterator<Item = char>) -> EditError {
    let so_far = format!("{}{}", word.head, word.value);
    // Without a `=` there is no key to build a suggestion from.
    let Some((key, _)) = so_far.split_once('=') else {
        return EditError::new(format!(
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
    EditError::new(format!(
        "unexpected quote in `{shown}`; quote the whole value: {key}=\"{whole}\""
    ))
}

/// Parses one `key=value` word.
fn parse_change(word: &Word) -> Result<Change, EditError> {
    let Some(key) = word.head.strip_suffix('=') else {
        return Err(EditError::new(format!(
            "expected `<key>=<value>`, got `{}`; quote values that contain spaces",
            word.head
        )));
    };
    // `delay+=10f` splits as key `delay+`; only `=` is supported for now.
    if let Some(op) = key.chars().last().filter(|c| "+-*/<>!".contains(*c)) {
        return Err(EditError::new(format!(
            "operator `{op}=` is not supported yet; use `=`"
        )));
    }
    if key.is_empty() {
        return Err(EditError::new(format!(
            "missing key before `=` in `{}`; expected `<key>=<value>`",
            word.shown()
        )));
    }
    if word.value.is_empty() && !word.quoted {
        return Err(EditError::new(format!(
            "missing value for `{key}`; use `{key}=\"\"` for an empty string"
        )));
    }
    Ok(Change {
        key: key.to_string(),
        value: if word.quoted {
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
mod tests {
    use super::*;
    use serde_json::json;

    fn set(line: &str) -> Value {
        match &parse_edit(line).unwrap().changes[0].value {
            EditValue::Set(v) => v.clone(),
            EditValue::Remove => panic!("expected Set"),
        }
    }
    fn err(line: &str) -> String {
        parse_edit(line).unwrap_err().to_string()
    }

    #[test]
    fn parses_targets_keys_and_typed_values() {
        let e = parse_edit(r#"title text="Launch \"Week\"" fill=#FFF fps=60 in=none"#).unwrap();
        assert_eq!(e.target, "title");
        assert_eq!(e.changes[0].value, EditValue::Set(json!("Launch \"Week\"")));
        assert_eq!(e.changes[1].value, EditValue::Set(json!("#FFF")));
        assert_eq!(e.changes[2].value, EditValue::Set(json!(60)));
        assert_eq!(e.changes[3].value, EditValue::Remove);
    }

    #[test]
    fn unquoted_spaces_get_a_helpful_error() {
        let m = parse_edit("card at=center 666").unwrap_err().to_string();
        assert!(
            m.contains("expected `<key>=<value>`, got `666`")
                && m.contains("quote values that contain spaces")
        );
    }

    #[test]
    fn rejects_unsupported_operators_and_empty_lines() {
        assert!(
            parse_edit("card delay+=10f")
                .unwrap_err()
                .to_string()
                .contains("operator `+=` is not supported yet; use `=`")
        );
        assert!(
            parse_edit("   ")
                .unwrap_err()
                .to_string()
                .contains("empty edit")
        );
        assert!(
            parse_edit("card")
                .unwrap_err()
                .to_string()
                .contains("no changes")
        );
        assert!(
            parse_edit(r#"card text="open"#)
                .unwrap_err()
                .to_string()
                .contains("unclosed quote")
        );
    }

    #[test]
    fn backslash_escape_gives_one_backslash() {
        assert_eq!(set(r#"a text="C:\\dir""#), json!("C:\\dir"));
    }

    #[test]
    fn escaped_quote_does_not_end_the_string_or_split_the_word() {
        assert_eq!(
            set(r#"a text="say \"hi there\" now""#),
            json!("say \"hi there\" now")
        );
    }

    #[test]
    fn quoted_value_may_contain_equals_and_spaces() {
        let e = parse_edit(r#"a text="x = y" fill=red"#).unwrap();
        assert_eq!(e.changes[0].value, EditValue::Set(json!("x = y")));
        assert_eq!(e.changes[1].key, "fill");
    }

    #[test]
    fn quoted_keywords_and_numbers_stay_strings() {
        assert_eq!(set(r#"a text="none""#), json!("none"));
        assert_eq!(set(r#"a text="60""#), json!("60"));
        assert_eq!(set(r#"a text="true""#), json!("true"));
    }

    #[test]
    fn parses_booleans() {
        assert_eq!(set("a x=true"), json!(true));
        assert_eq!(set("a x=false"), json!(false));
    }

    #[test]
    fn parses_floats_and_negatives() {
        assert_eq!(set("a x=1.5"), json!(1.5));
        assert_eq!(set("a x=-2"), json!(-2));
        assert_eq!(set("a x=-0.25"), json!(-0.25));
        assert_eq!(set("a x=1e3"), json!(1000.0));
    }

    #[test]
    fn non_numbers_stay_strings() {
        assert_eq!(set("a x=12f"), json!("12f"));
        assert_eq!(set("a x=NaN"), json!("NaN"));
        assert_eq!(set("a x=inf"), json!("inf"));
        assert_eq!(set("a x=-inf"), json!("-inf"));
        assert_eq!(set("a x=1e999"), json!("1e999"));
        assert_eq!(set("a x=center"), json!("center"));
    }

    #[test]
    fn repeated_key_is_an_error() {
        assert!(err("a fill=red fill=blue").contains("key `fill` given twice"));
    }

    #[test]
    fn empty_key_is_an_error() {
        assert!(err("a =x").contains("missing key before `=`"));
    }

    #[test]
    fn empty_value_is_an_error_with_a_hint() {
        assert_eq!(
            err("a fill="),
            "missing value for `fill`; use `fill=\"\"` for an empty string"
        );
        assert_eq!(set(r#"a fill="""#), json!(""));
    }

    #[test]
    fn missing_target_is_an_error() {
        assert!(err("fill=red").contains("expected a target"));
    }

    #[test]
    fn a_quote_inside_a_bare_value_is_rejected_with_the_whole_value_suggested() {
        assert_eq!(
            err(r#"a x=ab"c d""#),
            r#"unexpected quote in `x=ab"c`; quote the whole value: x="abc d""#
        );
        // Quote straight after the closing one, and a second `=` before it.
        assert_eq!(
            err(r#"a x=ab"c""#),
            r#"unexpected quote in `x=ab"c"`; quote the whole value: x="abc""#
        );
        assert_eq!(
            err(r#"a x=1="2 3" y=4"#),
            r#"unexpected quote in `x=1="2`; quote the whole value: x="1=2 3""#
        );
    }

    #[test]
    fn a_quote_outside_a_value_names_the_expected_form() {
        assert_eq!(
            err(r#"a"b x=1"#),
            r#"unexpected quote in `a"`; quotes only go around a value, as in `<key>="<value>"`"#
        );
        assert_eq!(
            err(r#"a "x"=1"#),
            r#"unexpected quote in `"`; quotes only go around a value, as in `<key>="<value>"`"#
        );
    }

    #[test]
    fn text_after_a_closing_quote_is_rejected() {
        assert_eq!(
            err(r#"a x="b"c"#),
            "unexpected text after the closing quote in `x=\"b\"c`; put a space after the quote, or quote the whole value"
        );
    }

    #[test]
    fn unclosed_quote_shows_what_was_read() {
        assert_eq!(
            err(r#"a text="open"#),
            r#"unclosed quote in `text="open`; add a closing `"`"#
        );
    }

    #[test]
    fn well_formed_quotes_still_work_next_to_other_words() {
        let e = parse_edit(r#"a x="b c" y="" z=3"#).unwrap();
        assert_eq!(e.changes[0].value, EditValue::Set(json!("b c")));
        assert_eq!(e.changes[1].value, EditValue::Set(json!("")));
        assert_eq!(e.changes[2].value, EditValue::Set(json!(3)));
    }

    #[test]
    fn bad_quoting_and_escapes_are_errors() {
        assert!(err(r#"a text="x"y"#).contains("after the closing quote"));
        assert!(err(r#"a text="x"y""#).contains("after the closing quote"));
        assert!(err(r#"a text="a\nb""#).contains("unknown escape"));
    }
}
