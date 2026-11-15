//! Times written against a base: a label of the composition, or the layer
//! before in the list (`<` its end, `<<` its start), moved by a time. A
//! layer's `offset` and the ends of its `span` take them, and labels take
//! them against other labels. They resolve when the scene is read.

use std::fmt;
use std::str::FromStr;

use winnow::combinator::{alt, eof, opt, peek, preceded, terminated};
use winnow::prelude::*;
use winnow::token::{one_of, take_while};

use crate::doc::Form;
use crate::grammar::{Parsed, number, parse_all};
use crate::units::{Time, TimeSpan, amount, time, unit};
use crate::{Diagnostic, Kind, Result};

/// The accepted forms of a moment, such as an `offset` or a label.
pub(crate) const MOMENT_EXPECTED: &str = "a time such as `2s` or `-6f`, or a base moved by a time: a label (`cta`, `cta+12f`, `cta-0.5s`), `<` where the layer before ends (`<+6f`) or `<<` where it starts (`<<-3f`)";
/// The accepted forms of a span with relative ends.
pub(crate) const MOMENTS_EXPECTED: &str = "`<start>-<end>`, each a time or a base moved by a time, e.g. `0-30f`, `intro-cta`, `<-<+2s` or `cta-6f-outro`; a bare start takes the end's unit";
/// The rule for label names.
pub(crate) const LABEL_RULE: &str =
    "label names use lowercase letters, digits and `_`, starting with a letter";

/// What a moment is measured from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// The start of the composition's timeline.
    Zero,
    /// A label of the composition.
    Label(String),
    /// `<`: where the layer before in the list ends.
    Before,
    /// `<<`: where the layer before in the list starts.
    BeforeStart,
}

/// A time as written: a base moved by a signed time.
#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    /// What it is measured from.
    pub base: Base,
    /// How far from the base, either way.
    pub shift: Time,
}

impl Moment {
    /// An absolute time.
    pub fn at(time: Time) -> Moment {
        Moment {
            base: Base::Zero,
            shift: time,
        }
    }

    /// The time it stands for: its base, found by `base`, plus its shift,
    /// in the unit they share or else in frames at `fps`.
    ///
    /// # Errors
    /// The error `base` gives for a base it cannot find.
    pub fn resolve(&self, base: impl FnOnce(&Base) -> Result<Time>, fps: f64) -> Result<Time> {
        let from = match &self.base {
            Base::Zero => return Ok(self.shift),
            other => base(other)?,
        };
        Ok(add(from, self.shift, fps))
    }
}

/// `a + b`, in the unit they share or else in frames at `fps`; adding
/// nothing keeps `a` as it is.
pub fn add(a: Time, b: Time, fps: f64) -> Time {
    match (a, b) {
        (a, Time::Frames(0.0) | Time::Seconds(0.0)) => a,
        (Time::Frames(x), Time::Frames(y)) => Time::Frames(x + y),
        (Time::Seconds(x), Time::Seconds(y)) => Time::Seconds(x + y),
        (a, b) => Time::Frames(a.to_frames(fps) + b.to_frames(fps)),
    }
}

impl fmt::Display for Base {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base::Zero => Ok(()),
            Base::Label(name) => f.write_str(name),
            Base::Before => f.write_str("<"),
            Base::BeforeStart => f.write_str("<<"),
        }
    }
}

impl fmt::Display for Moment {
    /// Writes the moment as a scene would: `2s`, `cta`, `<+6f`, `cta-0.5s`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (amount, frames) = match self.shift {
            Time::Frames(n) => (n, true),
            Time::Seconds(n) => (n, false),
        };
        if self.base == Base::Zero {
            return write!(f, "{}", self.shift);
        }
        write!(f, "{}", self.base)?;
        if amount == 0.0 {
            return Ok(());
        }
        let size = match frames {
            true => Time::Frames(amount.abs()),
            false => Time::Seconds(amount.abs()),
        };
        write!(f, "{}{size}", if amount < 0.0 { '-' } else { '+' })
    }
}

impl FromStr for Moment {
    type Err = Diagnostic;

    /// Parses a signed time, or a base with an optional move.
    fn from_str(text: &str) -> Result<Moment> {
        parse_all(text, terminated(moment, eof), |_: Option<()>| {
            Diagnostic::syntax(format!("`{text}` is not a time or a moment"))
                .expected(MOMENT_EXPECTED)
        })
    }
}

impl Form for Moment {
    const FORM: &str = MOMENT_EXPECTED;
}

/// A span whose ends may be relative, written `<start>-<end>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Moments {
    /// Where it starts.
    pub start: Moment,
    /// Where it ends.
    pub end: Moment,
}

impl FromStr for Moments {
    type Err = Diagnostic;

    /// Parses an absolute span as [`TimeSpan`] does, with its errors, or a
    /// span with a relative end.
    fn from_str(text: &str) -> Result<Moments> {
        let absolute = match text.parse::<TimeSpan>() {
            Ok(span) => {
                return Ok(Moments {
                    start: Moment::at(span.start),
                    end: Moment::at(span.end),
                });
            }
            Err(e) => e,
        };
        // Only a base, `<` or a label, makes a span relative; anything else
        // keeps the plain span's error.
        let based = text.contains('<')
            || text
                .chars()
                .any(|c| c.is_ascii_lowercase() && c != 'f' && c != 's');
        parse_all(text, moments, |_: Option<()>| match based {
            true => {
                Diagnostic::syntax(format!("`{text}` is not a span")).expected(MOMENTS_EXPECTED)
            }
            false => absolute,
        })
    }
}

impl Form for Moments {
    const FORM: &str = MOMENTS_EXPECTED;
}

/// Whether `name` may name a label.
pub fn is_label(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A label name, by [`is_label`].
fn label<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    (
        one_of(|c: char| c.is_ascii_lowercase()),
        take_while(0.., |c: char| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
        }),
    )
        .take()
        .parse_next(input)
}

/// A base other than zero.
fn base<R>(input: &mut &str) -> Parsed<Base, R> {
    alt((
        "<<".value(Base::BeforeStart),
        "<".value(Base::Before),
        label.map(|name: &str| Base::Label(name.to_owned())),
    ))
    .parse_next(input)
}

/// A move after a base: `+<time>` or `-<time>`.
fn shift<R>(input: &mut &str) -> Parsed<Time, R> {
    (one_of(['+', '-']), time)
        .map(|(sign, time)| match sign {
            '-' => negate(time),
            _ => time,
        })
        .parse_next(input)
}

fn negate(time: Time) -> Time {
    match time {
        Time::Frames(n) => Time::Frames(-n),
        Time::Seconds(n) => Time::Seconds(-n),
    }
}

/// A whole moment: a signed time, or a base with an optional move.
fn moment<R>(input: &mut &str) -> Parsed<Moment, R> {
    alt((
        (base, opt(shift)).map(|(base, shift)| Moment {
            base,
            shift: shift.unwrap_or(Time::Frames(0.0)),
        }),
        (number, unit).map(|(n, unit)| Moment::at(unit(n))),
    ))
    .parse_next(input)
}

/// One end of a span: a time that is not negative, or a base with a move,
/// or with `bare`, the base alone.
fn end<R>(bare: bool) -> impl FnMut(&mut &str) -> Parsed<Moment, R> {
    move |input: &mut &str| {
        let moved = |input: &mut &str| match bare {
            true => Ok(Time::Frames(0.0)),
            false => opt(shift)
                .map(|shift| shift.unwrap_or(Time::Frames(0.0)))
                .parse_next(input),
        };
        alt((
            (base, moved).map(|(base, shift)| Moment { base, shift }),
            time.map(Moment::at),
        ))
        .parse_next(input)
    }
}

/// `<start>-<end>`: a move binds to its base unless the span then has no
/// end, when the `-` separates the ends.
fn moments<R>(input: &mut &str) -> Parsed<Moments, R> {
    let pair = |bare: bool| {
        move |input: &mut &str| {
            (terminated(end(bare), '-'), terminated(end(false), eof))
                .map(|(start, end)| Moments { start, end })
                .parse_next(input)
        }
    };
    alt((pair(false), pair(true), bare_start)).parse_next(input)
}

/// A bare number before the dash takes the end's unit, as in `0-cta`.
fn bare_start<R>(input: &mut &str) -> Parsed<Moments, R> {
    (
        terminated(amount, peek('-')),
        preceded('-', terminated(end(false), eof)),
    )
        .map(|(start, end)| {
            let unit = match end.shift {
                Time::Seconds(_) if end.base == Base::Zero => Time::Seconds,
                _ => Time::Frames,
            };
            Moments {
                start: Moment::at(unit(start)),
                end,
            }
        })
        .parse_next(input)
}

/// Checks a label's name.
///
/// # Errors
/// An invalid-name [`Diagnostic`] that gives the rule.
pub(crate) fn label_name(name: &str) -> Result<()> {
    match is_label(name) {
        true => Ok(()),
        false => Err(
            Diagnostic::new(Kind::Invalid, format!("`{name}` is not a valid label name"))
                .expected(LABEL_RULE),
        ),
    }
}

#[cfg(test)]
#[path = "../tests/unit/when.rs"]
mod tests;
