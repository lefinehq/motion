//! Unit tests for `src/parse.rs`. Errors are compared with the [`Problem`] the
//! grammar should name; their wording is pinned once, in `every_error_message`.

use super::*;
use serde_json::json;

fn set(line: &str) -> Value {
    match &parse_edit(line).unwrap().changes[0].value {
        EditValue::Set(v) => v.clone(),
        EditValue::Remove => panic!("expected Set"),
    }
}
fn fail(line: &str) -> Diagnostic {
    parse_edit(line).unwrap_err()
}
/// The diagnostic for JSON in `x` that serde_json rejects, for its own reason.
fn invalid_json(json: &str) -> Diagnostic {
    let reason = serde_json::from_str::<Value>(json).unwrap_err().to_string();
    Problem::InvalidJson { key: "x", reason }.diagnostic()
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
fn unquoted_spaces_are_a_word_that_is_not_a_change() {
    assert_eq!(
        fail("card at=center 666"),
        Problem::NotAChange("666").diagnostic()
    );
}

#[test]
fn rejects_unsupported_operators_and_empty_lines() {
    assert_eq!(fail("card delay+=10f"), Problem::Operator('+').diagnostic());
    assert_eq!(fail("   "), Problem::Empty.diagnostic());
    assert_eq!(fail("card"), Problem::NoChanges("card").diagnostic());
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
fn repeated_key_is_a_conflict() {
    let e = fail("a fill=red fill=blue");
    assert_eq!(e.kind, Kind::Conflict);
    assert_eq!((e.path, e.expected), (None, None));
}

#[test]
fn empty_value_is_an_error_with_a_hint() {
    assert_eq!(fail("a fill="), Problem::MissingValue("fill").diagnostic());
    assert_eq!(set(r#"a fill="""#), json!(""));
}

#[test]
fn missing_target_is_an_error() {
    assert_eq!(fail("fill=red"), Problem::NoTarget("fill=red").diagnostic());
}

#[test]
fn a_quote_inside_a_bare_value_is_rejected_with_the_whole_value_suggested() {
    let misquoted = |written, meant: &str| {
        Problem::Misquoted {
            written,
            key: "x",
            meant: meant.to_owned(),
        }
        .diagnostic()
    };
    assert_eq!(fail(r#"a x=ab"c d""#), misquoted(r#"x=ab"c"#, "abc d"));
    // An open quote, a quote straight after the closing one, and a second `=`.
    assert_eq!(fail(r#"a x=ab"c d"#), misquoted(r#"x=ab"c"#, "abc d"));
    assert_eq!(fail(r#"a x=ab"c""#), misquoted(r#"x=ab"c""#, "abc"));
    assert_eq!(fail(r#"a x=1="2 3" y=4"#), misquoted(r#"x=1="2"#, "1=2 3"));
    assert_eq!(fail(r#"a x=a"b\"c""#), misquoted(r#"x=a"b\"c""#, r#"ab"c"#));
}

#[test]
fn a_misquoted_suggestion_parses_when_pasted_back() {
    let expected = fail(r#"a x=a"b\"c""#).expected.unwrap();
    let suggestion = &expected[expected.find("x=").unwrap()..];
    let e = parse_edit(&format!("a {suggestion}")).unwrap();
    assert_eq!(e.changes[0].value, EditValue::Set(json!("ab\"c")));
}

#[test]
fn a_quote_outside_a_value_is_a_stray_quote() {
    assert_eq!(fail(r#"a"b x=1"#), Problem::StrayQuote("a").diagnostic());
    assert_eq!(fail(r#"a "x"=1"#), Problem::StrayQuote("").diagnostic());
}

#[test]
fn text_after_a_closing_quote_is_rejected() {
    for (line, key, text) in [
        (r#"a x="b"c"#, "x", r#""b"c"#),
        (r#"a text="x"y"#, "text", r#""x"y"#),
        (r#"a text="x"y""#, "text", r#""x"y""#),
    ] {
        assert_eq!(
            fail(line),
            Problem::TextAfterQuote { key, text }.diagnostic(),
            "{line}"
        );
    }
}

#[test]
fn unclosed_quote_shows_what_was_read() {
    assert_eq!(
        fail(r#"a text="open"#),
        Problem::UnclosedQuote {
            key: "text",
            rest: r#""open"#
        }
        .diagnostic()
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
fn bad_escapes_are_errors() {
    assert_eq!(fail(r#"a text="a\nb""#), Problem::Escape('n').diagnostic());
    assert_eq!(fail(r#"a text="a\"#), Problem::DanglingEscape.diagnostic());
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
fn empty_path_segments_are_invalid_keys() {
    for key in ["anim..x", ".x", "x."] {
        assert_eq!(
            fail(&format!("a {key}=1")),
            Problem::InvalidKey(key).diagnostic(),
            "{key}"
        );
    }
}

#[test]
fn json_errors_carry_serde_json_s_reason() {
    assert_eq!(fail(r#"a x=[1,"]" y=2"#), invalid_json(r#"[1,"]" y=2"#));
    assert_eq!(fail("a x=[1,]"), invalid_json("[1,]"));
    assert_eq!(
        fail(r#"a x={"a":1"#),
        Problem::UnclosedJson {
            key: "x",
            open: '{',
            rest: r#"{"a":1"#
        }
        .diagnostic()
    );
    // A `[` that does not start a value stays an ordinary character.
    assert_eq!(set("a x=a[1"), json!("a[1"));
}

#[test]
fn operators_other_than_equals_are_named() {
    for (line, op) in [("a x+=1", '+'), ("a x-=1", '-'), ("a anim.x*=2", '*')] {
        assert_eq!(fail(line), Problem::Operator(op).diagnostic(), "{line}");
    }
}

#[test]
fn key_errors_name_the_key() {
    assert_eq!(fail("a =x"), Problem::MissingKey("=x").diagnostic());
    assert_eq!(fail("a x$y=1"), Problem::InvalidKey("x$y").diagnostic());
    // A quote in a key is a stray quote, not a bad character.
    assert_eq!(fail(r#"a x"y=1"#), Problem::StrayQuote("x").diagnostic());
}

#[test]
fn json_is_read_to_its_closing_bracket_only() {
    // A mismatched closing bracket is serde_json's to report.
    assert_eq!(fail("a x=[1}"), invalid_json("[1}"));
    assert_eq!(
        fail("a x=[1]y"),
        Problem::TextAfterJson {
            key: "x",
            text: "[1]y"
        }
        .diagnostic()
    );
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    assert_eq!(fail(&format!("a x={deep}")), invalid_json(&deep));
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
fn overlapping_paths_are_one_conflict_in_either_order() {
    for (first, second) in [
        (
            r#"card anim=[] anim.x="0 -> 1 @0-5f""#,
            r#"card anim.x="0 -> 1 @0-5f" anim={}"#,
        ),
        ("card anim.x.y=1 anim.x=2", "card anim.x=2 anim.x.y=1"),
    ] {
        let e = fail(first);
        assert_eq!(e.kind, Kind::Conflict, "{first}");
        assert_eq!(e, fail(second), "{first}");
    }
    // Siblings and names that merely share a prefix are fine.
    assert!(parse_edit("card anim.x=1 anim.y=2 animal=3 anim.xy=4").is_ok());
}

#[test]
fn errors_say_what_kind_of_problem_they_are() {
    let kind = |line: &str| fail(line).kind;
    assert_eq!(kind(""), Kind::Syntax);
    assert_eq!(kind("a fill=\"red"), Kind::Syntax);
    assert_eq!(kind("a fill=red fill=blue"), Kind::Conflict);
    assert_eq!(kind("a anim={} anim.x=1"), Kind::Conflict);
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

/// Every error message of an edit, from the line to the edited scene, one
/// sample each, so a change of wording shows up in review.
#[test]
fn every_error_message() {
    let doc = motion_format::parse_document(include_str!("../../../../examples/launch.json"))
        .expect("the launch example");
    let deep = format!("a x={}{}", "[".repeat(200), "]".repeat(200));
    let samples = [
        "",
        "fill=red",
        "card",
        "card at=center 666",
        r#"a"b x=1"#,
        r#"a "x"=1"#,
        r#"a x"y=1"#,
        r#"a x=ab"c d""#,
        r#"a x=ab"c d"#,
        r#"a x=ab"c""#,
        r#"a x=1="2 3" y=4"#,
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
        &deep,
        "a x=[1]y",
        "a fill=red fill=blue",
        "card anim={} anim.x=1",
        "card anim.x.y=1 anim.x=2",
        "ghost fill=#FFF",
        "card colour=#FFF",
        "scene colour=red",
        "card in.opacity=1",
        "title text=2024",
        "title text=true",
    ];
    let report: Vec<String> = samples
        .iter()
        .map(|line| {
            let error = parse_edit(line)
                .and_then(|edit| crate::apply_edit(&doc, &edit))
                .unwrap_err();
            let shown = if *line == deep { "a x=[[[…]]]" } else { line };
            format!("`{shown}` → {error}")
        })
        .collect();
    insta::assert_snapshot!(report.join("\n"));
}
