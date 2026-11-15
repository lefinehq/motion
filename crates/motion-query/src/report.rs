//! The result of `check`, printed compactly because an agent reads it instead of frames.

use motion_format::{Address, fmt_num};
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
    /// A layer moves farther in one frame than the eye follows smoothly.
    Judder,
    /// A text layer is readable for less time than it takes to read.
    TextHold,
}

/// What a finding measured, as the report prints it after the frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measure {
    /// How far a node leaves an area, in whole px: `+12px`.
    Overshoot(f64),
    /// A value above the most a rule allows: `24px/f >11`.
    Above {
        /// The worst value measured.
        value: f64,
        /// The most the rule allows.
        limit: f64,
        /// What both are counted in.
        unit: Unit,
    },
    /// A value below the least a rule allows: `0.4s <1.3`.
    Below {
        /// The worst value measured.
        value: f64,
        /// The least the rule allows.
        limit: f64,
        /// What both are counted in.
        unit: Unit,
    },
}

/// The unit of a [`Measure`], printed right after its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
pub enum Unit {
    /// Pixels of the canvas per frame.
    #[strum(serialize = "px/f")]
    PxPerFrame,
    /// Seconds.
    #[strum(serialize = "s")]
    Seconds,
}

impl fmt::Display for Measure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Measure::Overshoot(px) => write!(f, "+{}px", fmt_num(px)),
            Measure::Above { value, limit, unit } => {
                write!(f, "{}{unit} >{}", fmt_num(value), fmt_num(limit))
            }
            Measure::Below { value, limit, unit } => {
                write!(f, "{}{unit} <{}", fmt_num(value), fmt_num(limit))
            }
        }
    }
}

/// One problem found on a node, merged over a run of consecutive frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    /// How serious the problem is.
    pub severity: Severity,
    /// The rule that found it.
    pub code: Code,
    /// The layer the problem is on, at any depth: `card/title`.
    pub address: Address,
    /// The first frame of the run.
    pub first_frame: u32,
    /// The last frame of the run, inclusive.
    pub last_frame: u32,
    /// The worst the run gets, against the rule's limit.
    pub measure: Measure,
    /// An edit that clears it, such as `card at="center 880"`.
    pub fix: Option<String>,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}-{}f {}",
            self.severity, self.code, self.address, self.first_frame, self.last_frame, self.measure
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
