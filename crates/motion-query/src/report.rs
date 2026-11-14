//! The result of `check`, printed compactly because an agent reads it instead of frames.

use motion_format::fmt_num;
use std::fmt;

/// How serious a violation is, printed as its first letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
pub enum Severity {
    /// Something is broken and should be fixed. Makes the CLI exit with 1.
    #[strum(serialize = "E")]
    Error,
    /// Something is probably unintended but the scene still works.
    #[strum(serialize = "W")]
    Warning,
}

/// The identity of a rule, printed as its short name, such as `offscreen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum Code {
    /// A visible node leaves the canvas.
    Offscreen,
    /// A visible node leaves the title-safe area.
    Unsafe,
}

/// One problem found on a node, merged over a run of consecutive frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    /// How serious the problem is.
    pub severity: Severity,
    /// The rule that found it.
    pub code: Code,
    /// The layer id the problem is on.
    pub id: String,
    /// The first frame of the run.
    pub first_frame: u32,
    /// The last frame of the run, inclusive.
    pub last_frame: u32,
    /// The worst overshoot in the run, rounded up to a whole pixel.
    pub amount_px: f64,
    /// An edit that clears it, such as `card at="center 880"`.
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
    /// How many (rule, node) pairs ran and found nothing.
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
#[path = "../tests/unit/report.rs"]
mod tests;
