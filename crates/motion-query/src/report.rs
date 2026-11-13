//! The result of `check`: what was found, and how it is printed.
//!
//! Part of step 4 (Query). The text format is compact on purpose, because an
//! agent reads it instead of looking at frames.

use motion_format::fmt_num;
use std::fmt;

/// How serious a violation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Something is broken and should be fixed. Makes the CLI exit with 1.
    Error,
    /// Something is probably unintended but the scene still works.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "E",
            Severity::Warning => "W",
        })
    }
}

/// One problem found on a node, merged over a run of consecutive frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    /// How serious the problem is.
    pub severity: Severity,
    /// The short name of the rule that found it, such as `offscreen`.
    pub code: &'static str,
    /// The layer id the problem is on.
    pub id: String,
    /// The first frame of the run.
    pub first_frame: u32,
    /// The last frame of the run, inclusive.
    pub last_frame: u32,
    /// The worst overshoot in the run, rounded up to a whole pixel.
    pub amount_px: f64,
    /// A suggested replacement, written as an edit such as `card at="center 880"`.
    pub fix: Option<String>,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}-{}f +{}px",
            self.severity,
            self.code,
            self.id,
            self.first_frame,
            self.last_frame,
            fmt_num(self.amount_px)
        )?;
        if let Some(fix) = &self.fix {
            write!(f, "; fix: {fix}")?;
        }
        Ok(())
    }
}

/// Everything `check` found in a scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The problems, in scene order.
    pub violations: Vec<Violation>,
    /// How many (rule, node) pairs were evaluated and had no problem.
    pub passed: usize,
}

impl Report {
    /// Whether any violation is an error (warnings do not count).
    pub fn has_errors(&self) -> bool {
        self.violations
            .iter()
            .any(|v| v.severity == Severity::Error)
    }
}

impl fmt::Display for Report {
    /// One line per violation, then `OK {passed}`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for v in &self.violations {
            writeln!(f, "{v}")?;
        }
        write!(f, "OK {}", self.passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violation(severity: Severity, fix: Option<&str>) -> Violation {
        Violation {
            severity,
            code: "offscreen",
            id: "card".into(),
            first_frame: 3,
            last_frame: 9,
            amount_px: 12.0,
            fix: fix.map(String::from),
        }
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
}
