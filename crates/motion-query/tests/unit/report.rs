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
    assert_eq!(Code::Offscreen.to_string(), "offscreen");
    assert_eq!(Code::Unsafe.to_string(), "unsafe");
}

#[test]
fn severity_displays_as_one_letter() {
    assert_eq!(Severity::Error.to_string(), "E");
    assert_eq!(Severity::Warning.to_string(), "W");
}

#[test]
fn violation_displays_without_fix() {
    assert_eq!(
        violation(Severity::Error, None).to_string(),
        "E offscreen card 3-9f +12px"
    );
}

#[test]
fn violation_displays_with_fix() {
    assert_eq!(
        violation(Severity::Warning, Some("card at=\"1 2\"")).to_string(),
        "W offscreen card 3-9f +12px; fix: card at=\"1 2\""
    );
}

#[test]
fn report_joins_lines_and_ends_with_ok_count() {
    let r = Report {
        violations: vec![
            violation(Severity::Error, None),
            violation(Severity::Warning, None),
        ],
        passed: 2,
    };
    assert_eq!(
        r.to_string(),
        "E offscreen card 3-9f +12px\nW offscreen card 3-9f +12px\nOK 2"
    );
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
