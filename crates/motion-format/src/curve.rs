//! Easing curves as data: how progress through an animation maps to movement.
//!
//! Part of step 1 (Read): parses the curve tokens of a behaviour (and later
//! of keyframes) into a [`Curve`]. Step 2 stores the curve on the graph and
//! step 3 samples it. Every curve is a cubic bézier from (0,0) to (1,1), the
//! same model as CSS timing functions, plus `hold`, which steps. Names are
//! shortcuts listed once, in [`NAMED_CURVES`].

use std::fmt;

use motion_diagnostic::{Diagnostic, Kind, Result};

use crate::fmt_num;

/// The expected form of a custom curve.
const CUBIC_EXPECTED: &str =
    "`cubic(x1,y1,x2,y2)` (no spaces) with x1 and x2 between 0 and 1, e.g. `cubic(.2,0,0,1)`";

/// The prefix that marks a custom curve token.
const CUBIC_PREFIX: &str = "cubic(";

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

    /// True when `token` is meant as a curve: a table name or anything that
    /// starts `cubic(`. Malformed `cubic(` tokens count, so they reach
    /// [`Curve::parse`] and get its specific error.
    pub fn is_curve_token(token: &str) -> bool {
        token.starts_with(CUBIC_PREFIX) || Curve::named(token).is_some()
    }

    /// Parses a name from [`NAMED_CURVES`] or `cubic(x1,y1,x2,y2)`.
    pub fn parse(token: &str) -> Result<Curve> {
        if let Some(curve) = Curve::named(token) {
            return Ok(curve);
        }
        let Some(inner) = token.strip_prefix(CUBIC_PREFIX) else {
            let names: Vec<&str> = NAMED_CURVES.iter().map(|(n, _)| *n).collect();
            return Err(
                Diagnostic::new(Kind::UnknownName, format!("unknown curve `{token}`")).expected(
                    format!("one of {}, or cubic(x1,y1,x2,y2)", names.join(", ")),
                ),
            );
        };
        let err = |m: String| Diagnostic::syntax(m).expected(CUBIC_EXPECTED);
        let Some(inner) = inner.strip_suffix(')') else {
            return Err(err(format!("`{token}` is missing its closing `)`")));
        };
        let values: Vec<f64> = match inner
            .split(',')
            .map(|v| v.trim().parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect::<Option<Vec<f64>>>()
        {
            Some(v) if v.len() == 4 => v,
            _ => return Err(err(format!("`{token}` needs four numbers"))),
        };
        let (x1, y1, x2, y2) = (values[0], values[1], values[2], values[3]);
        // With x outside 0..=1 the curve could run backwards in time, and a
        // frame would have two values.
        if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
            return Err(err(format!("`{token}`: x1 and x2 must be between 0 and 1")));
        }
        Ok(Curve::Bezier(x1, y1, x2, y2))
    }
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
