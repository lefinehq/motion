//! Parsing of one-line edits into typed [`Edit`] values.
//!
//! Part of the edit step of the pipeline. It only reads text; it knows
//! nothing about scenes. Whether a key is valid for its target is checked
//! later, in `apply`.
//!
//! Grammar: `<target> <key>=<value> [<key>=<value> ...]`, where a value is a
//! quoted string (`\"` and `\\` escapes), `none` (remove the key), a number,
//! `true`/`false`, or any other bare word (a string).

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
/// Returns an [`EditError`] for an empty line, an unclosed quote, a missing
/// target or changes, a word that is not `key=value`, an unsupported
/// operator, an empty key or value, or a key given twice.
pub fn parse_edit(line: &str) -> Result<Edit, EditError> {
    let tokens = split_tokens(line)?;
    let mut tokens = tokens.into_iter();
    let target = tokens
        .next()
        .ok_or_else(|| EditError::new("empty edit; expected `<target> <key>=<value>`"))?;
    if target.contains('=') {
        return Err(EditError::new(format!(
            "expected a target (a layer id or `scene`) before `{target}`"
        )));
    }
    let mut changes: Vec<Change> = Vec::new();
    for token in tokens {
        let change = parse_change(&token)?;
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

/// Splits a line into whitespace-separated words, keeping quoted text
/// (spaces included) inside its word. Quotes and escapes are left in place;
/// `parse_value` unescapes them later.
fn split_tokens(line: &str) -> Result<Vec<String>, EditError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            // Inside quotes a backslash protects the next char, so `\"`
            // does not end the string. Both chars stay in the token.
            '\\' if in_quote => {
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '"' => {
                in_quote = !in_quote;
                current.push(c);
            }
            c if c.is_whitespace() && !in_quote => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if in_quote {
        return Err(EditError::new(format!(
            "unclosed quote in `{}`; add a closing `\"`",
            current.trim()
        )));
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

/// Parses one `key=value` word.
fn parse_change(token: &str) -> Result<Change, EditError> {
    let Some((key, raw_value)) = token.split_once('=') else {
        return Err(EditError::new(format!(
            "expected `<key>=<value>`, got `{token}`; quote values that contain spaces"
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
            "missing key before `=` in `{token}`; expected `<key>=<value>`"
        )));
    }
    if raw_value.is_empty() {
        return Err(EditError::new(format!(
            "missing value for `{key}`; use `{key}=\"\"` for an empty string"
        )));
    }
    Ok(Change {
        key: key.to_string(),
        value: parse_value(raw_value)?,
    })
}

/// Turns the text after `=` into an [`EditValue`].
fn parse_value(raw: &str) -> Result<EditValue, EditError> {
    if let Some(rest) = raw.strip_prefix('"') {
        return unquote(rest).map(|s| EditValue::Set(Value::String(s)));
    }
    Ok(match raw {
        "none" => EditValue::Remove,
        "true" => EditValue::Set(Value::Bool(true)),
        "false" => EditValue::Set(Value::Bool(false)),
        _ => EditValue::Set(parse_number(raw).unwrap_or_else(|| Value::String(raw.to_string()))),
    })
}

/// Unescapes the body of a quoted string (the opening quote is already
/// removed). The closing quote must be the last character.
fn unquote(rest: &str) -> Result<String, EditError> {
    let mut out = String::new();
    let mut chars = rest.chars();
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
            '"' => {
                return if chars.next().is_none() {
                    Ok(out)
                } else {
                    Err(EditError::new(format!(
                        "unexpected text after the closing quote in `\"{rest}`"
                    )))
                };
            }
            c => out.push(c),
        }
    }
    Err(EditError::new("unclosed quote in a value"))
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
    fn bad_quoting_and_escapes_are_errors() {
        assert!(err(r#"a text="x"y"#).contains("after the closing quote"));
        assert!(err(r#"a text="x"y""#).contains("unclosed quote"));
        assert!(err(r#"a text="a\nb""#).contains("unknown escape"));
    }
}
