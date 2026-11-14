//! Easing curves as data: how progress through an animation maps to movement.
//!
//! Part of step 1 (Read): parses the curve tokens of a behaviour (and later
//! of keyframes) into a [`Curve`]. Step 2 stores the curve on the graph and
//! step 3 samples it. Every curve is a cubic bézier from (0,0) to (1,1), the
//! same model as CSS timing functions, plus `hold`, which steps. Names are
//! shortcuts listed once, in [`NAMED_CURVES`].

use std::fmt;
use std::str::FromStr;

use crate::grammar::{Parsed, blank0, ends_with, number, parse_all};
use crate::{Diagnostic, Result};
use winnow::combinator::{alt, cut_err, delimited, eof, preceded, separated_pair, terminated};
use winnow::prelude::*;
use winnow::token::rest;

use crate::doc::Form;
use crate::fmt_num;
use crate::table;

/// The accepted forms of a curve, for a value that is not a string. The
/// names are listed by the unknown-curve error, which reads [`NAMED_CURVES`].
const EXPECTED: &str = "a curve name such as `ease-out`, or `cubic(x1,y1,x2,y2)`";

/// The expected form of a custom curve.
const CUBIC_EXPECTED: &str =
    "`cubic(x1,y1,x2,y2)` (no spaces) with x1 and x2 between 0 and 1, e.g. `cubic(.2,0,0,1)`";

/// The form of a custom curve, as listed after the curve names.
const CUBIC_FORM: &str = "cubic(x1,y1,x2,y2)";

/// The prefix that marks a custom curve token.
pub(crate) const CUBIC_PREFIX: &str = "cubic(";

/// How progress is eased over a duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// Constant speed.
    Linear,
    /// Stays at the start value, then jumps to the end value at the end.
    Hold,
    /// A cubic bézier with control points `(x1, y1)` and `(x2, y2)`. `x1`
    /// and `x2` lie in 0..=1 so that time only moves forward; `y1` and `y2`
    /// may leave 0..=1 to overshoot or anticipate.
    Bezier(f64, f64, f64, f64),
}

/// The curves that have a name, the only place names are listed. Parsing,
/// printing and error messages all read this table.
pub const NAMED_CURVES: &[(&str, Curve)] = &[
    ("linear", Curve::Linear),
    ("hold", Curve::Hold),
    ("ease", Curve::Bezier(0.25, 0.1, 0.25, 1.0)),
    ("ease-in", Curve::Bezier(0.42, 0.0, 1.0, 1.0)),
    ("ease-out", Curve::Bezier(0.0, 0.0, 0.58, 1.0)),
    ("ease-in-out", Curve::Bezier(0.42, 0.0, 0.58, 1.0)),
];

impl Curve {
    /// Looks a curve up in [`NAMED_CURVES`] by name.
    pub fn named(name: &str) -> Option<Curve> {
        table::lookup(NAMED_CURVES, name).map(|(_, c)| *c)
    }
}

impl FromStr for Curve {
    type Err = Diagnostic;

    /// Parses a name from [`NAMED_CURVES`] or `cubic(x1,y1,x2,y2)`.
    fn from_str(token: &str) -> Result<Curve> {
        parse_all(token, curve, |problem| {
            // A custom curve names its problem once `cubic(` is read, so a
            // failure without one is a name that is not in the table.
            problem.unwrap_or(Problem::Unknown).diagnostic(token)
        })
    }
}

impl Form for Curve {
    const FORM: &str = EXPECTED;
}

/// What is wrong with a curve token.
#[derive(Debug, Clone, Copy)]
enum Problem {
    /// The token is not a name in [`NAMED_CURVES`], nor a custom curve.
    Unknown,
    /// A custom curve without the `)` that closes its arguments.
    Unclosed,
    /// A custom curve whose arguments are not four numbers.
    Count,
    /// A custom curve with `x1` or `x2` outside 0..=1.
    XRange,
}

impl Problem {
    /// The diagnostic for `token`, which has this problem: the one place
    /// the curve grammar's failures become messages.
    fn diagnostic(self, token: &str) -> Diagnostic {
        match self {
            Problem::Unknown => Diagnostic::unknown("curve", token, table::names(NAMED_CURVES))
                .or_expected(CUBIC_FORM),
            Problem::Unclosed => {
                Diagnostic::syntax(format!("`{token}` is missing its closing `)`"))
                    .expected(CUBIC_EXPECTED)
            }
            Problem::Count => {
                Diagnostic::syntax(format!("`{token}` needs four numbers")).expected(CUBIC_EXPECTED)
            }
            Problem::XRange => {
                Diagnostic::syntax(format!("`{token}`: x1 and x2 must be between 0 and 1"))
                    .expected(CUBIC_EXPECTED)
            }
        }
    }
}

/// A table name or `cubic(x1,y1,x2,y2)`. Once `cubic(` is read the token is
/// a custom curve, so its errors say what is wrong with the arguments
/// instead of listing the names.
fn curve(input: &mut &str) -> Parsed<Curve, Problem> {
    alt((preceded(CUBIC_PREFIX, cut_err(cubic_arguments)), named)).parse_next(input)
}

/// A curve name: the whole token, looked up in [`NAMED_CURVES`]. Names are
/// data, so the grammar knows none of them, nor their alphabet.
fn named(input: &mut &str) -> Parsed<Curve, Problem> {
    rest.verify_map(Curve::named).parse_next(input)
}

/// `x1,y1,x2,y2)`: what follows `cubic(`. The token must end with `)`
/// before its arguments are read. Each step gives its own reason where it
/// can fail, and the first reason given sticks.
fn cubic_arguments(input: &mut &str) -> Parsed<Curve, Problem> {
    closed
        .context(Problem::Unclosed)
        .and_then(arguments)
        .parse_next(input)
}

/// Everything up to and including the closing `)`, which ends the token.
fn closed<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem> {
    ends_with(')').take().parse_next(input)
}

/// The four numbers and the `)` after them, with `x1` and `x2` in range.
fn arguments(input: &mut &str) -> Parsed<Curve, Problem> {
    terminated(separated_pair(point, ',', point), (')', eof))
        .context(Problem::Count)
        .verify(x_in_range)
        .context(Problem::XRange)
        .map(|((x1, y1), (x2, y2))| Curve::Bezier(x1, y1, x2, y2))
        .parse_next(input)
}

/// Whether both control points have `x` in 0..=1. Outside it the curve could
/// run backwards in time, and a frame would have two values.
fn x_in_range(((x1, _), (x2, _)): &(Point, Point)) -> bool {
    (0.0..=1.0).contains(x1) && (0.0..=1.0).contains(x2)
}

/// A control point of a cubic curve, `(x, y)`.
type Point = (f64, f64);

/// `x,y`: one control point.
fn point(input: &mut &str) -> Parsed<Point, Problem> {
    separated_pair(coordinate, ',', coordinate).parse_next(input)
}

/// One number of a control point, optionally padded with spaces.
fn coordinate(input: &mut &str) -> Parsed<f64, Problem> {
    delimited(blank0, number, blank0).parse_next(input)
}

impl fmt::Display for Curve {
    /// Prints the table name when the curve equals a table entry, otherwise
    /// `cubic(x1,y1,x2,y2)`. The output parses back to the same curve up to two
    /// decimals, because numbers go through `fmt_num`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((name, _)) = NAMED_CURVES.iter().find(|(_, c)| c == self) {
            return f.write_str(name);
        }
        match self {
            Curve::Bezier(x1, y1, x2, y2) => write!(
                f,
                "cubic({},{},{},{})",
                fmt_num(*x1),
                fmt_num(*y1),
                fmt_num(*x2),
                fmt_num(*y2)
            ),
            // Every non-bézier curve has a table entry (a unit test checks
            // this), so it was printed by name above; names live only there.
            Curve::Linear | Curve::Hold => {
                unreachable!("every non-bézier curve has a NAMED_CURVES entry")
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/curve.rs"]
mod tests;
