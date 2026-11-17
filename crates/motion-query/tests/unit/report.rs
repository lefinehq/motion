//! Unit tests for `src/report.rs`.

use super::*;
use rstest::rstest;

fn violation(severity: Severity, fix: Option<&str>) -> Violation {
    Violation {
        severity,
        code: Code::Offscreen,
        address: "card".parse().unwrap(),
        first_frame: 3,
        last_frame: 9,
        measure: Measure::Overshoot(12.0),
        fix: fix.map(String::from),
    }
}

#[rstest]
#[case(Code::Offscreen, "offscreen")]
#[case(Code::Unsafe, "unsafe")]
#[case(Code::Judder, "judder")]
#[case(Code::TextHold, "text_hold")]
#[case(Code::Easing, "easing")]
#[case(Code::Crowd, "crowd")]
#[case(Code::Contrast, "contrast")]
#[case(Code::Flash, "flash")]
fn codes_display_as_their_names(#[case] code: Code, #[case] name: &str) {
    assert_eq!(code.to_string(), name);
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

#[rstest]
#[case::an_overshoot(Measure::Overshoot(12.0), "+12px")]
#[case::above_a_limit(
    Measure::Above { value: 24.0, limit: 11.0, unit: Unit::PxPerFrame },
    "24px/f >11"
)]
#[case::below_a_limit(
    Measure::Below { value: 0.4, limit: 1.3, unit: Unit::Seconds },
    "0.4s <1.3"
)]
#[case::a_contrast_ratio(
    Measure::Below { value: 2.1, limit: 4.5, unit: Unit::Ratio },
    "2.1:1 <4.5"
)]
#[case::flashes_a_second(
    Measure::Above { value: 5.0, limit: 3.0, unit: Unit::PerSecond },
    "5/s >3"
)]
fn measures_print_their_value_unit_and_limit(#[case] measure: Measure, #[case] printed: &str) {
    assert_eq!(measure.to_string(), printed);
}

/// One error by `code` on `address`, measured `measure`, over frames 3–9.
fn found(code: Code, address: &str, measure: Measure) -> Violation {
    Violation {
        code,
        address: address.parse().unwrap(),
        measure,
        ..violation(Severity::Error, None)
    }
}

fn above(value: f64) -> Measure {
    Measure::Above {
        value,
        limit: 11.0,
        unit: Unit::PxPerFrame,
    }
}

fn below(value: f64) -> Measure {
    Measure::Below {
        value,
        limit: 4.5,
        unit: Unit::Ratio,
    }
}

#[rstest]
#[case::the_same(found(Code::Offscreen, "card", Measure::Overshoot(12.0)), true)]
#[case::less_far_out(found(Code::Offscreen, "card", Measure::Overshoot(5.0)), true)]
#[case::farther_out(found(Code::Offscreen, "card", Measure::Overshoot(13.0)), false)]
#[case::another_layer(found(Code::Offscreen, "title", Measure::Overshoot(12.0)), false)]
#[case::another_rule(found(Code::Unsafe, "card", Measure::Overshoot(12.0)), false)]
#[case::slower(found(Code::Judder, "card", above(20.0)), true)]
#[case::faster(found(Code::Judder, "card", above(30.0)), false)]
#[case::more_contrast(found(Code::Contrast, "card", below(3.0)), true)]
#[case::less_contrast(found(Code::Contrast, "card", below(1.5)), false)]
fn a_baseline_knows_what_it_has_as_bad_or_worse(#[case] now: Violation, #[case] known: bool) {
    let base = Report {
        violations: vec![
            found(Code::Offscreen, "card", Measure::Overshoot(12.0)),
            found(Code::Judder, "card", above(25.0)),
            found(Code::Contrast, "card", below(2.1)),
        ],
        passed: 3,
    };
    let mut report = Report {
        violations: vec![now.clone()],
        passed: 1,
    };
    assert_eq!(report.since(&base), usize::from(known));
    assert_eq!(report.violations.is_empty(), known);
    assert_eq!(report.passed, 1);
}

#[test]
fn a_baseline_knows_a_problem_that_moved_in_time() {
    let base = Report {
        violations: vec![violation(Severity::Error, None)],
        passed: 0,
    };
    let mut later = Report {
        violations: vec![Violation {
            first_frame: 40,
            last_frame: 80,
            ..violation(Severity::Error, None)
        }],
        passed: 0,
    };
    assert_eq!(later.since(&base), 1);
}
