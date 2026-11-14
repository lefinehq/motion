//! Unit tests for `src/parse.rs`.

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
    assert_eq!(
        err("card at=center 666"),
        "`666` is not a change; expected `<key>=<value>`, with quotes around a value that has spaces"
    );
}

#[test]
fn rejects_unsupported_operators_and_empty_lines() {
    assert!(
        parse_edit("card delay+=10f")
            .unwrap_err()
            .to_string()
            .contains("operator `+=` is not supported yet")
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
    assert_eq!(e.changes[1].key.as_str(), "fill");
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
        "missing value for `fill`; expected `fill=<value>`, or `fill=\"\"` for an empty string"
    );
    assert_eq!(set(r#"a fill="""#), json!(""));
}

#[test]
fn missing_target_is_an_error() {
    assert_eq!(
        err("fill=red"),
        "no target before `fill=red`; expected a target first: a layer id or `scene`"
    );
}

#[test]
fn a_quote_inside_a_bare_value_is_rejected_with_the_whole_value_suggested() {
    assert_eq!(
        err(r#"a x=ab"c d""#),
        r#"unexpected quote in `x=ab"c`; expected the whole value quoted: x="abc d""#
    );
    // Quote straight after the closing one, and a second `=` before it.
    assert_eq!(
        err(r#"a x=ab"c""#),
        r#"unexpected quote in `x=ab"c"`; expected the whole value quoted: x="abc""#
    );
    // The suggestion is escaped, so it parses when pasted back.
    let message = err(r#"a x=a"b\"c""#);
    assert_eq!(
        message,
        r#"unexpected quote in `x=a"b\"c"`; expected the whole value quoted: x="ab\"c""#
    );
    let suggestion = message.rsplit("the whole value quoted: ").next().unwrap();
    let e = parse_edit(&format!("a {suggestion}")).unwrap();
    assert_eq!(
        e.changes[0].value,
        EditValue::Set(serde_json::json!("ab\"c"))
    );
    assert_eq!(
        err(r#"a x=1="2 3" y=4"#),
        r#"unexpected quote in `x=1="2`; expected the whole value quoted: x="1=2 3""#
    );
}

#[test]
fn a_quote_outside_a_value_names_the_expected_form() {
    assert_eq!(
        err(r#"a"b x=1"#),
        r#"unexpected quote in `a"`; expected quotes only around a value, as in `<key>="<value>"`"#
    );
    assert_eq!(
        err(r#"a "x"=1"#),
        r#"unexpected quote in `"`; expected quotes only around a value, as in `<key>="<value>"`"#
    );
}

#[test]
fn text_after_a_closing_quote_is_rejected() {
    assert_eq!(
        err(r#"a x="b"c"#),
        "unexpected text after the closing quote in `x=\"b\"c`; expected a space after the quote, or the whole value quoted"
    );
}

#[test]
fn unclosed_quote_shows_what_was_read() {
    assert_eq!(
        err(r#"a text="open"#),
        r#"unclosed quote in `text="open`; expected a closing `"`"#
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
    assert_eq!(
        err(r#"a text="a\nb""#),
        r#"unknown escape `\n` in a quoted value; expected `\"` or `\\`"#
    );
    assert_eq!(
        err(r#"a text="a\"#),
        r#"dangling `\` at the end of a quoted value; expected `\"` or `\\`"#
    );
}

#[test]
fn dotted_keys_and_json_values() {
    let e = parse_edit(r#"card anim.opacity=[["0f",0],["12f",1,"ease out ]"]] in=none"#).unwrap();
    assert_eq!(e.changes[0].key.as_str(), "anim.opacity");
    assert_eq!(e.changes[0].key.segments(), ["anim", "opacity"]);
    assert_eq!(
        e.changes[0].value,
        EditValue::Set(json!([["0f", 0], ["12f", 1, "ease out ]"]]))
    );
    assert_eq!(e.changes[1].value, EditValue::Remove);
    let e = parse_edit(r#"scene presets.my-pop={"opacity":0,"scale":0.5} fps=60"#).unwrap();
    assert_eq!(e.changes[0].key.segments(), ["presets", "my-pop"]);
    assert_eq!(
        e.changes[0].value,
        EditValue::Set(json!({"opacity":0,"scale":0.5}))
    );
    assert_eq!(e.changes[1].value, EditValue::Set(json!(60)));
    // Escaped quotes inside a JSON string do not end it.
    assert_eq!(set(r#"a x=["a\"]b"]"#), json!(["a\"]b"]));
}

#[test]
fn empty_path_segments_are_errors() {
    for line in ["a anim..x=1", "a .x=1", "a x.=1"] {
        assert!(
            err(line).ends_with("expected `<key>[.<key>…]=<value>`"),
            "{line}"
        );
    }
}

#[test]
fn json_errors_show_the_expected_form() {
    assert_eq!(
        err(r#"a x=[1,"]" y=2"#),
        "invalid JSON in `x`: expected `,` or `]` at line 1 column 8; expected `<key>=[…]` or `<key>={…}`, valid JSON"
    );
    assert!(err("a x={\"a\":1").starts_with("unclosed `{` in `x={\"a\":1`"));
    let m = err("a x=[1,]");
    assert!(m.starts_with("invalid JSON in `x`: ") && m.ends_with("valid JSON"));
    assert!(err("a x=[1]y").contains("after the end of the JSON value"));
    // A `[` that does not start a value stays an ordinary character.
    assert_eq!(set("a x=a[1"), json!("a[1"));
}

/// Property tests: the parser must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_edit_any_string(s in ".*") {
            let _ = parse_edit(&s);
        }

        #[test]
        fn parse_edit_bracket_heavy(s in r#"[a-z.=\[\]{}", \\0-9]{0,40}"#) {
            let _ = parse_edit(&s);
        }

        #[test]
        fn parse_edit_json_after_equals(s in r#"a x=[\[\]{}", \\a0-9:]{0,30}"#) {
            let _ = parse_edit(&s);
        }

        #[test]
        fn parse_edit_quote_heavy(s in r#"[a-z.=" \\]{0,40}"#) {
            let _ = parse_edit(&s);
        }

        #[test]
        fn parse_edit_short_printable_ascii(s in "[ -~]{0,40}") {
            let _ = parse_edit(&s);
        }
    }
}

/// Property tests: a change written the way the grammar documents reads
/// back as the key and value it was written from.
mod round_trips {
    use super::*;
    use proptest::prelude::*;

    /// A dotted key whose names do not end in `-`, which would read as `-=`.
    const KEY: &str = "[a-z]([a-z0-9_-]{0,6}[a-z0-9_])?(\\.[a-z][a-z0-9_]{0,6}){0,2}";

    fn only_change(line: &str) -> Change {
        let mut edit = parse_edit(line).unwrap();
        assert_eq!(edit.changes.len(), 1);
        edit.changes.remove(0)
    }

    proptest! {
        #[test]
        fn quoted_text_reads_back_as_the_same_string(key in KEY, text in ".*") {
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            let change = only_change(&format!("card {key}=\"{escaped}\""));
            prop_assert_eq!(change.key.as_str(), key.as_str());
            prop_assert_eq!(change.value, EditValue::Set(Value::String(text)));
        }

        #[test]
        fn integers_read_back_as_integers(key in KEY, n in prop::num::i64::ANY) {
            let change = only_change(&format!("scene {key}={n}"));
            prop_assert_eq!(change.key.as_str(), key.as_str());
            prop_assert_eq!(change.value, EditValue::Set(Value::from(n)));
        }

        #[test]
        fn json_reads_back_as_the_same_value(key in KEY, items in prop::collection::vec(".*", 0..4)) {
            // `in` itself, or a key under it, would overlap the `in=none` after it.
            prop_assume!(key.split('.').next() != Some("in"));
            let value = serde_json::json!({ "items": items });
            // The value ends at its balanced bracket, so a change can follow it.
            let edit = parse_edit(&format!("card {key}={value} in=none")).unwrap();
            prop_assert_eq!(&edit.changes[0].value, &EditValue::Set(value));
            prop_assert_eq!(&edit.changes[1].value, &EditValue::Remove);
        }
    }
}

#[test]
fn operators_other_than_equals_are_named() {
    for (line, op) in [("a x+=1", "+"), ("a x-=1", "-"), ("a anim.x*=2", "*")] {
        assert_eq!(
            err(line),
            format!("operator `{op}=` is not supported yet; expected `<key>=<value>`"),
            "{line}"
        );
    }
}

#[test]
fn key_errors_name_the_key() {
    assert_eq!(
        err("a =x"),
        "missing key before `=` in `=x`; expected `<key>=<value>`"
    );
    assert_eq!(
        err("a x$y=1"),
        "invalid key `x$y`: each dot-separated part must be non-empty letters, digits, `-` or `_`; expected `<key>[.<key>…]=<value>`"
    );
    // A quote in a key is a stray quote, not a bad character.
    assert!(err(r#"a x"y=1"#).starts_with(r#"unexpected quote in `x"`"#));
}

#[test]
fn a_misquoted_value_with_an_open_quote_still_gets_a_suggestion() {
    assert_eq!(
        err(r#"a x=ab"c d"#),
        r#"unexpected quote in `x=ab"c`; expected the whole value quoted: x="abc d""#
    );
}

#[test]
fn json_is_read_to_its_closing_bracket_only() {
    // A mismatched closing bracket is serde_json's to report.
    assert!(err("a x=[1}").starts_with("invalid JSON in `x`: "));
    assert_eq!(
        err("a x=[1]y"),
        "unexpected text after the end of the JSON value in `x=[1]y`; expected a space after the closing bracket"
    );
    let deep = format!("a x={}{}", "[".repeat(200), "]".repeat(200));
    assert_eq!(
        err(&deep),
        "invalid JSON in `x`: recursion limit exceeded at line 1 column 128; expected `<key>=[…]` or `<key>={…}`, valid JSON"
    );
}

#[test]
fn values_may_contain_equals_and_brackets_after_their_start() {
    assert_eq!(set("a x==1"), json!("=1"));
    assert_eq!(set("a x=1=2"), json!("1=2"));
}

#[test]
fn spaces_around_the_line_and_between_words_are_ignored() {
    let e = parse_edit("  card \t fill=red   in=none  ").unwrap();
    assert_eq!(e.target, "card");
    assert_eq!(e.changes.len(), 2);
}

#[test]
fn overlapping_paths_are_rejected() {
    for line in [
        r#"card anim=[] anim.x="0 -> 1 @0-5f""#,
        r#"card anim.x="0 -> 1 @0-5f" anim={}"#,
    ] {
        assert_eq!(
            err(line),
            "keys `anim` and `anim.x` overlap; set them in separate edits or merge them into one value",
            "{line}"
        );
    }
    assert!(err("card anim.x.y=1 anim.x=2").contains("keys `anim.x` and `anim.x.y` overlap"));
    // Siblings and names that merely share a prefix are fine.
    assert!(parse_edit("card anim.x=1 anim.y=2 animal=3 anim.xy=4").is_ok());
}

#[test]
fn errors_say_what_kind_of_problem_they_are() {
    let kind = |line: &str| parse_edit(line).unwrap_err().kind;
    assert_eq!(kind(""), Kind::Syntax);
    assert_eq!(kind("a fill=\"red"), Kind::Syntax);
    assert_eq!(kind("a fill=red fill=blue"), Kind::Conflict);
    assert_eq!(kind("a anim={} anim.x=1"), Kind::Conflict);
}

/// Every error message of the edit language, one sample line each, so a
/// change of wording shows up in review.
#[test]
fn every_error_message() {
    let samples = [
        "",
        "fill=red",
        "card",
        "card at=center 666",
        r#"a"b x=1"#,
        r#"a x"y=1"#,
        r#"a x=ab"c d""#,
        r#"a x=a"b\"c""#,
        "card delay+=10f",
        "a x-=1",
        "a =x",
        "a x$y=1",
        "a anim..x=1",
        "a fill=",
        r#"a text="open"#,
        r#"a x="b"c"#,
        r#"a text="a\nb""#,
        r#"a text="a\"#,
        "a x=[1, 2",
        r#"a x={"a":1"#,
        "a x=[1,]",
        "a x=[1}",
        r#"a x=[1,"]" y=2"#,
        "a x=[1]y",
        "a fill=red fill=blue",
        "card anim={} anim.x=1",
        "card anim.x.y=1 anim.x=2",
    ];
    let report: Vec<String> = samples
        .iter()
        .map(|line| format!("`{line}` → {}", err(line)))
        .collect();
    insta::assert_snapshot!(report.join("\n"));
}
