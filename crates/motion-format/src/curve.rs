//! Easing curves as data: a cubic bézier from (0,0) to (1,1), as in CSS
//! timing functions, `hold`, or the speed and influence on each side of a key
//! (`speed(...)`), which holds any ease in the value's own units ([D31]).
//! Names are shortcuts listed in [`NAMED_CURVES`].
//!
//! [D31]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d31-typed-values-and-value-space-keys

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
const EXPECTED: &str =
    "a curve name such as `ease-out`, `cubic(x1,y1,x2,y2)` or `speed(out,out-reach,in,in-reach)`";
const CUBIC_EXPECTED: &str =
    "`cubic(x1,y1,x2,y2)` (no spaces) with x1 and x2 between 0 and 1, e.g. `cubic(.2,0,0,1)`";
const SPEED_EXPECTED: &str = "`speed(out,out-reach,in,in-reach)` (no spaces): the speed leaving the key and arriving at the next, in value units per second, each with how far into the span it reaches, 0 to 1, e.g. `speed(0,.33,0,.33)`";
/// The custom forms, as listed after the curve names.
const CUSTOM_FORMS: [&str; 2] = ["cubic(x1,y1,x2,y2)", "speed(out,out-reach,in,in-reach)"];
pub(crate) const CUBIC_PREFIX: &str = "cubic(";
const SPEED_PREFIX: &str = "speed(";

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
    /// The speed on each side of a span, in value units per second, and how
    /// far into the span each side's handle reaches (0 to 1). Unlike a bézier,
    /// it holds a span that leaves a value and returns to it.
    Speed {
        /// The speed leaving the first key.
        out_speed: f64,
        /// How far the first key's handle reaches into the span.
        out_reach: f64,
        /// The speed arriving at the second key.
        in_speed: f64,
        /// How far the second key's handle reaches back into the span.
        in_reach: f64,
    },
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
    /// `speed(…)` is not closed or does not hold four numbers.
    SpeedForm,
    /// A reach of `speed(…)` is outside 0..=1.
    ReachRange,
}

impl Problem {
    fn diagnostic(self, token: &str) -> Diagnostic {
        let message = match self {
            Problem::Unknown => {
                return Diagnostic::unknown("curve", token, table::names(NAMED_CURVES))
                    .or_expected(CUSTOM_FORMS.join(" or "));
            }
            Problem::SpeedForm => {
                return Diagnostic::syntax(format!(
                    "`{token}` needs four numbers and a closing `)`"
                ))
                .expected(SPEED_EXPECTED);
            }
            Problem::ReachRange => {
                return Diagnostic::syntax(format!(
                    "`{token}`: each reach must be between 0 and 1"
                ))
                .expected(SPEED_EXPECTED);
            }
            Problem::Unclosed => format!("`{token}` is missing its closing `)`"),
            Problem::Count => format!("`{token}` needs four numbers"),
            Problem::XRange => format!("`{token}`: x1 and x2 must be between 0 and 1"),
        };
        Diagnostic::syntax(message).expected(CUBIC_EXPECTED)
    }
}

/// A name, `cubic(…)` or `speed(…)`; once a prefix is read, errors are about its arguments.
fn curve(input: &mut &str) -> Parsed<Curve, Problem> {
    alt((
        preceded(CUBIC_PREFIX, cut_err(cubic_arguments)),
        preceded(SPEED_PREFIX, cut_err(speed_arguments)),
        named,
    ))
    .parse_next(input)
}

/// `out,out-reach,in,in-reach)`, each reach in 0..=1.
fn speed_arguments(input: &mut &str) -> Parsed<Curve, Problem> {
    terminated(separated_pair(point, ',', point), (')', eof))
        .context(Problem::SpeedForm)
        .verify(|((_, out_reach), (_, in_reach)): &(Point, Point)| {
            (0.0..=1.0).contains(out_reach) && (0.0..=1.0).contains(in_reach)
        })
        .context(Problem::ReachRange)
        .map(
            |((out_speed, out_reach), (in_speed, in_reach))| Curve::Speed {
                out_speed,
                out_reach,
                in_speed,
                in_reach,
            },
        )
        .parse_next(input)
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
            Curve::Speed {
                out_speed,
                out_reach,
                in_speed,
                in_reach,
            } => write!(
                f,
                "speed({},{},{},{})",
                fmt_num(*out_speed),
                fmt_num(*out_reach),
                fmt_num(*in_speed),
                fmt_num(*in_reach)
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
