//! Unit tests for `src/problem.rs`.

use super::*;
use motion_diagnostic::Kind;

#[test]
fn every_problem_is_a_syntax_error_with_an_expected_form() {
    let problems = [
        Problem::Empty,
        Problem::NoTarget("x=1"),
        Problem::NoChanges("card"),
        Problem::NotAChange("word"),
        Problem::StrayQuote("a"),
        Problem::Misquoted {
            written: "x=a\"b",
            key: "x",
            meant: "ab".into(),
        },
        Problem::Operator('+'),
        Problem::MissingKey("=x"),
        Problem::InvalidKey("a..b"),
        Problem::MissingValue("x"),
        Problem::UnclosedQuote {
            key: "x",
            rest: "\"a",
        },
        Problem::TextAfterQuote {
            key: "x",
            text: "\"a\"b",
        },
        Problem::Escape('n'),
        Problem::DanglingEscape,
        Problem::UnclosedJson {
            key: "x",
            open: '[',
            rest: "[1",
        },
        Problem::InvalidJson {
            key: "x",
            reason: "bad".into(),
        },
        Problem::TextAfterJson {
            key: "x",
            text: "[1]y",
        },
    ];
    for problem in problems {
        let shown = format!("{problem:?}");
        let d = problem.diagnostic();
        assert_eq!(d.kind, Kind::Syntax, "{shown}");
        assert!(d.expected.is_some(), "{shown}");
        assert!(d.path.is_none(), "{shown}");
    }
}

#[test]
fn a_misquoted_value_is_suggested_escaped() {
    let d = Problem::Misquoted {
        written: r#"x=a"b"#,
        key: "x",
        meant: r#"a"b\"#.into(),
    }
    .diagnostic();
    assert_eq!(
        d.expected.as_deref(),
        Some(r#"the whole value quoted: x="a\"b\\""#)
    );
}
