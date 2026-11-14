//! Easing curves as data: a cubic bézier from (0,0) to (1,1), as in CSS
//! timing functions, or `hold`. Names are shortcuts listed in [`NAMED_CURVES`].

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

/// The accepted forms of a curve, for a value that is not a string.
const EXPECTED: &str = "a curve name such as `ease-out`, or `cubic(x1,y1,x2,y2)`";
const CUBIC_EXPECTED: &str =
    "`cubic(x1,y1,x2,y2)` (no spaces) with x1 and x2 between 0 and 1, e.g. `cubic(.2,0,0,1)`";
/// The custom form, as listed after the curve names.
const CUBIC_FORM: &str = "cubic(x1,y1,x2,y2)";
pub(crate) const CUBIC_PREFIX: &str = "cubic(";

/// How progress is eased over a duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// Constant speed.
    Linear,
    /// Stays at the start value, then jumps to the end value at the end.
    Hold,
    /// A cubic bézier with control points `(x1, y1)` and `(x2, y2)`; `x1` and
    /// `x2` lie in 0..=1 so time only moves forward, `y` may overshoot.
    Bezier(f64, f64, f64, f64),
}

/// The curves that have a name, the one list parsing, printing and errors read.
pub const NAMED_CURVES: &[(&str, Curve)] = &[
    ("linear", Curve::Linear),
    ("hold", Curve::Hold),
    ("ease", Curve::Bezier(0.25, 0.1, 0.25, 1.0)),
    ("ease-in", Curve::Bezier(0.42, 0.0, 1.0, 1.0)),
    ("ease-out", Curve::Bezier(0.0, 0.0, 0.58, 1.0)),
    ("ease-in-out", Curve::Bezier(0.42, 0.0, 0.58, 1.0)),
];

impl Curve {
    /// Looks a curve up in [`NAMED_CURVES`].
    pub fn named(name: &str) -> Option<Curve> {
        table::lookup(NAMED_CURVES, name).map(|(_, c)| *c)
    }
}

impl FromStr for Curve {
    type Err = Diagnostic;

    /// Parses a name from [`NAMED_CURVES`] or `cubic(x1,y1,x2,y2)`.
    fn from_str(token: &str) -> Result<Curve> {
        parse_all(token, curve, |problem| {
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
    Unknown,
    Unclosed,
    /// The arguments are not four numbers.
    Count,
    /// `x1` or `x2` is outside 0..=1.
    XRange,
}

impl Problem {
    fn diagnostic(self, token: &str) -> Diagnostic {
        let message = match self {
            Problem::Unknown => {
                return Diagnostic::unknown("curve", token, table::names(NAMED_CURVES))
                    .or_expected(CUBIC_FORM);
            }
            Problem::Unclosed => format!("`{token}` is missing its closing `)`"),
            Problem::Count => format!("`{token}` needs four numbers"),
            Problem::XRange => format!("`{token}`: x1 and x2 must be between 0 and 1"),
        };
        Diagnostic::syntax(message).expected(CUBIC_EXPECTED)
    }
}

/// A name or `cubic(…)`; once `cubic(` is read, errors are about the arguments.
fn curve(input: &mut &str) -> Parsed<Curve, Problem> {
    alt((preceded(CUBIC_PREFIX, cut_err(cubic_arguments)), named)).parse_next(input)
}

/// The whole token looked up in [`NAMED_CURVES`]; the grammar knows no names.
fn named(input: &mut &str) -> Parsed<Curve, Problem> {
    rest.verify_map(Curve::named).parse_next(input)
}

/// `x1,y1,x2,y2)`; the token must end with `)` before the arguments are read.
fn cubic_arguments(input: &mut &str) -> Parsed<Curve, Problem> {
    closed
        .context(Problem::Unclosed)
        .and_then(arguments)
        .parse_next(input)
}

fn closed<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem> {
    ends_with(')').take().parse_next(input)
}

fn arguments(input: &mut &str) -> Parsed<Curve, Problem> {
    terminated(separated_pair(point, ',', point), (')', eof))
        .context(Problem::Count)
        .verify(x_in_range)
        .context(Problem::XRange)
        .map(|((x1, y1), (x2, y2))| Curve::Bezier(x1, y1, x2, y2))
        .parse_next(input)
}

/// Whether both `x` lie in 0..=1; outside it a frame could have two values.
fn x_in_range(((x1, _), (x2, _)): &(Point, Point)) -> bool {
    (0.0..=1.0).contains(x1) && (0.0..=1.0).contains(x2)
}

/// A control point of a cubic curve, `(x, y)`.
type Point = (f64, f64);

fn point(input: &mut &str) -> Parsed<Point, Problem> {
    separated_pair(coordinate, ',', coordinate).parse_next(input)
}

fn coordinate(input: &mut &str) -> Parsed<f64, Problem> {
    delimited(blank0, number, blank0).parse_next(input)
}

impl fmt::Display for Curve {
    /// The table name when there is one, else `cubic(x1,y1,x2,y2)` to two decimals.
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
            Curve::Linear | Curve::Hold => {
                unreachable!("every non-bézier curve has a NAMED_CURVES entry")
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/curve.rs"]
mod tests;
