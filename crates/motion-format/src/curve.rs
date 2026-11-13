//! Easing curves as data: how progress through an animation maps to movement.
//!
//! Part of step 1 (Read): parses the curve tokens of a behaviour (and later
//! of keyframes) into a [`Curve`]. Step 2 stores the curve on the graph and
//! step 3 samples it. Every curve is a cubic bézier from (0,0) to (1,1), the
//! same model as CSS timing functions, plus `hold`, which steps. Names are
//! shortcuts listed once, in [`NAMED_CURVES`].

use std::fmt;

use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use winnow::combinator::{alt, cut_err, delimited, eof, preceded, separated_pair, terminated};
use winnow::prelude::*;
use winnow::token::take_while;

use crate::fmt_num;
use crate::shorthand::lex::{self, Parsed};

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
        NAMED_CURVES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, c)| *c)
    }
}

impl FromStr for Curve {
    type Err = Diagnostic;

    /// Parses a name from [`NAMED_CURVES`] or `cubic(x1,y1,x2,y2)`.
    fn from_str(token: &str) -> Result<Curve> {
        lex::parse_all(token, curve, |problem| match problem {
            None => {
                let names = NAMED_CURVES.iter().map(|(name, _)| name);
                Diagnostic::unknown("curve", token, names).or_expected(CUBIC_FORM)
            }
            Some(problem) => Diagnostic::syntax(problem.describe(token)).expected(CUBIC_EXPECTED),
        })
    }
}

/// What is wrong with a token that starts `cubic(`.
#[derive(Debug, Clone, Copy)]
enum Cubic {
    /// The token does not end with the `)` that closes the arguments.
    Unclosed,
    /// The arguments are not four numbers.
    Count,
    /// `x1` or `x2` is outside 0..=1.
    XRange,
}

impl Cubic {
    /// The message for `token`, which has this problem.
    fn describe(self, token: &str) -> String {
        match self {
            Cubic::Unclosed => format!("`{token}` is missing its closing `)`"),
            Cubic::Count => format!("`{token}` needs four numbers"),
            Cubic::XRange => format!("`{token}`: x1 and x2 must be between 0 and 1"),
        }
    }
}

/// A table name or `cubic(x1,y1,x2,y2)`. Once `cubic(` is read the token is
/// a custom curve, so its errors say what is wrong with the arguments
/// instead of listing the names.
fn curve(input: &mut &str) -> Parsed<Curve, Cubic> {
    alt((preceded(CUBIC_PREFIX, cut_err(cubic_arguments)), named)).parse_next(input)
}

/// A curve name, looked up in [`NAMED_CURVES`]: names are data, so none is
/// written in the grammar.
fn named(input: &mut &str) -> Parsed<Curve, Cubic> {
    take_while(1.., ('a'..='z', '-'))
        .verify_map(Curve::named)
        .parse_next(input)
}

/// `x1,y1,x2,y2)`: what follows `cubic(`. The token must end with `)`
/// before its arguments are read.
fn cubic_arguments(input: &mut &str) -> Parsed<Curve, Cubic> {
    lex::ends_with(')')
        .take()
        .context(Cubic::Unclosed)
        .and_then(terminated(separated_pair(point, ',', point), (')', eof)).context(Cubic::Count))
        // With x outside 0..=1 the curve could run backwards in time, and a
        // frame would have two values.
        .verify(|((x1, _), (x2, _)): &(Point, Point)| {
            (0.0..=1.0).contains(x1) && (0.0..=1.0).contains(x2)
        })
        .context(Cubic::XRange)
        .map(|((x1, y1), (x2, y2))| Curve::Bezier(x1, y1, x2, y2))
        .parse_next(input)
}

/// A control point of a cubic curve, `(x, y)`.
type Point = (f64, f64);

/// `x,y`: one control point.
fn point(input: &mut &str) -> Parsed<Point, Cubic> {
    separated_pair(coordinate, ',', coordinate).parse_next(input)
}

/// One number of a control point, optionally padded with spaces.
fn coordinate(input: &mut &str) -> Parsed<f64, Cubic> {
    delimited(lex::blank0, lex::number, lex::blank0).parse_next(input)
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
