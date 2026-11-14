//! Unit tests for `src/report.rs`.

use super::*;

fn violation(severity: Severity, fix: Option<&str>) -> Violation {
    Violation {
        severity,
        code: Code::Offscreen,
        id: "card".into(),
        first_frame: 3,
        last_frame: 9,
        amount_px: 12.0,
        fix: fix.map(String::from),
    }
}

#[test]
fn codes_display_as_their_names() {
    for code in [Code::Offscreen, Code::Unsafe] {
        assert_eq!(code.to_string(), code.name());
    }
}

#[test]
fn severities_display_differently() {
    assert_ne!(Severity::Error.to_string(), Severity::Warning.to_string());
}

#[test]
fn a_fix_is_appended_to_the_line() {
    const FIX: &str = r#"card at="1 2""#;
    let without = violation(Severity::Error, None).to_string();
    let with = violation(Severity::Error, Some(FIX)).to_string();
    assert!(!without.contains(FIX), "{without}");
    assert!(with.starts_with(&without) && with.ends_with(FIX), "{with}");
}

#[test]
fn report_prints_one_line_per_violation_then_the_ok_count() {
    let r = Report {
        violations: vec![
            violation(Severity::Error, None),
            violation(Severity::Warning, None),
        ],
        passed: 2,
    };
    let text = r.to_string();
    let lines: Vec<&str> = text.lines().collect();
    let (last, found) = lines.split_last().unwrap();
    let expected: Vec<String> = r.violations.iter().map(ToString::to_string).collect();
    assert_eq!(found, expected);
    assert!(last.ends_with(" 2"), "{last}");
}

#[test]
fn only_errors_count_for_has_errors() {
    let warn = Report {
        violations: vec![violation(Severity::Warning, None)],
        passed: 0,
    };
    let err = Report {
        violations: vec![violation(Severity::Error, None)],
        passed: 0,
    };
    assert!(!warn.has_errors());
    assert!(err.has_errors());
    assert!(
        !Report {
            violations: vec![],
            passed: 0
        }
        .has_errors()
    );
}
