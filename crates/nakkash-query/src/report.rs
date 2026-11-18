//! The result of `check`, printed compactly because an agent reads it instead of frames.

use nakkash_format::{Address, fmt_num};
use rustc_hash::FxHashMap;
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
    /// A move runs at one speed from start to end instead of easing.
    Easing,
    /// More layers move on their own at once than the eye can follow.
    Crowd,
    /// A text layer's glyphs stand out too little from what is drawn
    /// behind them.
    Contrast,
    /// A quarter or more of the frame flashes more often than is safe.
    Flash,
    /// Two readable text layers' glyphs run into each other.
    Overlap,
    /// A corner pin's corners make no quad its layer's box can be laid
    /// onto, so the layer draws nothing ([D89]).
    ///
    /// [D89]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography
    Pin,
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
    /// Why a corner pin's corners make no quad, at the run's first frame:
    /// `crossed: top-right–bottom-right meets bottom-left–top-left`.
    Pin(nakkash_scene::pin::Fault),
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
    /// A plain number: a ratio or a count.
    #[strum(serialize = "")]
    Number,
    /// A contrast ratio, against 1: `2.1:1`.
    #[strum(serialize = ":1")]
    Ratio,
    /// Times a second.
    #[strum(serialize = "/s")]
    PerSecond,
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
            Measure::Pin(fault) => write!(f, "{fault}"),
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
    /// The other layer, for a rule that measures two: the one `address` is
    /// drawn over.
    pub with: Option<Address>,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}-{}f {}",
            self.severity, self.code, self.address, self.first_frame, self.last_frame, self.measure
        )?;
        if let Some(with) = &self.with {
            write!(f, " over {with}")?;
        }
        if let Some(fix) = &self.fix {
            write!(f, "; fix: {fix}")?;
        }
        Ok(())
    }
}

/// Everything `check` found in a scene.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// The problems, in scene order.
    pub violations: Vec<Violation>,
    /// How many (rule, node) pairs ran and found nothing.
    pub passed: usize,
}

impl Measure {
    /// Whether this is no worse than `other`, a measure of the same rule:
    /// no farther out, no higher above its limit or no lower below it.
    fn no_worse_than(self, other: Measure) -> bool {
        match (self, other) {
            (Measure::Overshoot(now), Measure::Overshoot(then)) => now <= then,
            (Measure::Above { value: now, .. }, Measure::Above { value: then, .. }) => now <= then,
            (Measure::Below { value: now, .. }, Measure::Below { value: then, .. }) => now >= then,
            // A pin that made no quad then is as bad as one that makes none now.
            (Measure::Pin(_), Measure::Pin(_)) => true,
            _ => false,
        }
    }
}

/// The worst measures of one kind of violation in a base report, one per
/// form of [`Measure`]: what [`Report::since`] compares a violation with.
#[derive(Default)]
struct Worst {
    /// The farthest overshoot.
    overshoot: Option<f64>,
    /// The highest value above a limit.
    above: Option<f64>,
    /// The lowest value below a limit.
    below: Option<f64>,
    /// A corner pin that made no quad, if one did.
    pin: Option<nakkash_scene::pin::Fault>,
}

impl Worst {
    /// Counts `measure` in. A value that is not a number is kept only while
    /// nothing else is: no measure is ever no worse than it, as no
    /// comparison with it holds, while any other value it sits beside
    /// decides alone.
    fn add(&mut self, measure: Measure) {
        let keep = |slot: &mut Option<f64>, value: f64, worse: fn(f64, f64) -> bool| {
            *slot = Some(match *slot {
                None => value,
                Some(then) if then.is_nan() || worse(value, then) => value,
                Some(then) => then,
            });
        };
        match measure {
            Measure::Overshoot(v) => keep(&mut self.overshoot, v, |a, b| a > b),
            Measure::Above { value, .. } => keep(&mut self.above, value, |a, b| a > b),
            Measure::Below { value, .. } => keep(&mut self.below, value, |a, b| a < b),
            Measure::Pin(fault) => self.pin = self.pin.or(Some(fault)),
        }
    }

    /// Whether some measure counted in is as bad as `measure` or worse,
    /// as [`Measure::no_worse_than`] compares one with one.
    fn covers(&self, measure: Measure) -> bool {
        let base = match measure {
            Measure::Overshoot(_) => self.overshoot.map(Measure::Overshoot),
            Measure::Above { limit, unit, .. } => {
                self.above
                    .map(|value| Measure::Above { value, limit, unit })
            }
            Measure::Below { limit, unit, .. } => {
                self.below
                    .map(|value| Measure::Below { value, limit, unit })
            }
            Measure::Pin(_) => self.pin.map(Measure::Pin),
        };
        base.is_some_and(|base| measure.no_worse_than(base))
    }
}

impl Report {
    /// Drops the violations `base`, the report of an earlier version of the
    /// scene, already has, and returns how many it dropped. A violation is
    /// known when `base` has one by the same rule on the same layer (and the
    /// same other layer, for a rule that measures two) that is
    /// measured as bad or worse, whatever its frames: an edit may move a
    /// problem in time without making it worse.
    ///
    /// It takes time in proportion to the two reports' lengths: `base` is
    /// read once into the worst measure of each rule, layer and other layer,
    /// which is all a violation is compared with.
    pub fn since(&mut self, base: &Report) -> usize {
        // Sized for every violation of `base`, so it is never hashed twice.
        let mut worst: FxHashMap<(Code, &Address, Option<&Address>), Worst> =
            FxHashMap::with_capacity_and_hasher(base.violations.len(), Default::default());
        for b in &base.violations {
            worst
                .entry((b.code, &b.address, b.with.as_ref()))
                .or_default()
                .add(b.measure);
        }
        let before = self.violations.len();
        self.violations.retain(|v| {
            !worst
                .get(&(v.code, &v.address, v.with.as_ref()))
                .is_some_and(|w| w.covers(v.measure))
        });
        before - self.violations.len()
    }

    /// Whether this report, an earlier version's, already has `v`: one by
    /// the same rule on the same layer (and the same other layer, for a
    /// rule that measures two) measured as bad or worse, whatever its
    /// frames. What [`Report::since`] leaves out.
    pub fn knows(&self, v: &Violation) -> bool {
        self.violations.iter().any(|b| {
            b.code == v.code
                && b.address == v.address
                && b.with == v.with
                && v.measure.no_worse_than(b.measure)
        })
    }

    /// Whether any violation is an error (warnings do not count).
    pub fn has_errors(&self) -> bool {
        self.violations
            .iter()
            .any(|v| v.severity == Severity::Error)
    }

    /// Adds `other`'s violations after this report's, and its passes.
    pub fn join(&mut self, other: Report) {
        self.violations.extend(other.violations);
        self.passed += other.passed;
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
