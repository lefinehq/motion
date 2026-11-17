//! Easing curves as data: a cubic bézier from (0,0) to (1,1), as in CSS
//! timing functions, `hold`, or the speed and influence on each side of a key
//! (`speed(...)`), which holds any ease in the value's own units ([D31]).
//! Names are shortcuts listed in [`NAMED_CURVES`].
//!
//! [D31]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d31-typed-values-and-value-space-keys

use std::fmt;
use std::str::FromStr;

use crate::grammar::{Parsed, blank0, ends_with, number, parse_all};
use crate::{Diagnostic, Result};
use winnow::combinator::{
    alt, cut_err, delimited, empty, eof, opt, preceded, separated, separated_pair, terminated,
};
use winnow::prelude::*;
use winnow::token::rest;

use crate::doc::Form;
use crate::fmt_num;
use crate::spring::Spring;
use crate::table;
use crate::units::{Time, time};

/// The accepted forms of a curve, for a value that is not a string.
const EXPECTED: &str = "a curve name such as `ease-out` or `out-expo`, `cubic(x1,y1,x2,y2)`, `speed(out,out-reach,in,in-reach)` or `steps(n)`";
const CUBIC_EXPECTED: &str =
    "`cubic(x1,y1,x2,y2)` (no spaces) with x1 and x2 between 0 and 1, e.g. `cubic(.2,0,0,1)`";
const SPEED_EXPECTED: &str = "`speed(out,out-reach,in,in-reach)` (no spaces): the speed leaving the key and arriving at the next, in value units per second, each with how far into the span it reaches, 0 to 1, e.g. `speed(0,.33,0,.33)`";
const FAMILY_EXPECTED: &str = "`<shape>-<family>` such as `out-expo`, with a shape of `in`, `out`, `in-out` or `out-in` and a family of `quad`, `cubic`, `quart`, `quint`, `sine`, `circ`, `expo`, `back`, `elastic` or `bounce`; `<shape>(power)` such as `in-out(2.5)`; `<shape>-back(overshoot)`; or `<shape>-elastic(amplitude,period)`";
const SPRING_EXPECTED: &str = "`spring(bounce)` or `spring(bounce,perceived)` with a bounce above -1 and below 1 and a perceived duration such as `12f` or `.5s`, e.g. `spring(.3)`";
const STEPS_EXPECTED: &str =
    "`steps(n)` or `steps(n,start)` with a whole number of steps of at least 1, e.g. `steps(4)`";
/// The custom forms, as listed after the curve names.
const CUSTOM_FORMS: [&str; 5] = [
    "<shape>-<family> (e.g. out-expo)",
    "cubic(x1,y1,x2,y2)",
    "speed(out,out-reach,in,in-reach)",
    "steps(n)",
    "spring(bounce)",
];
pub(crate) const CUBIC_PREFIX: &str = "cubic(";
const SPEED_PREFIX: &str = "speed(";
const STEPS_PREFIX: &str = "steps(";
const SPRING_PREFIX: &str = "spring(";

/// The overshoot of `back` by default: the classic constant, about 10%.
pub const BACK_OVERSHOOT: f64 = 1.70158;
/// The amplitude and period of `elastic` by default.
pub const ELASTIC: (f64, f64) = (1.0, 0.3);

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
    /// A family's curve run in a shape: `out-expo`, `in-out-quint`,
    /// `out-back(2)`. Each starts at exactly 0 and ends at exactly 1.
    Shaped(Shape, Family),
    /// A spring released from rest that settles on the end: overshooting
    /// with a bounce above 0. In a span it is scaled to the span; a
    /// behaviour without a duration lasts its settle time.
    Spring(Spring),
    /// `count` equal steps, jumping at the end of each (`floor`), or at the
    /// start of each with `start` (`ceil`), as CSS `steps()`.
    Steps {
        /// How many steps, at least 1.
        count: u32,
        /// Whether each step jumps at its start.
        start: bool,
    },
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

/// Which way a family's curve runs across a span, for a family's "in"
/// curve `f`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `f(t)`: starts slowly.
    In,
    /// `1 - f(1 - t)`: ends slowly.
    Out,
    /// `in` over the first half and `out` over the second.
    InOut,
    /// `out` over the first half and `in` over the second.
    OutIn,
}

/// A family of curves, as its "in" curve `f` from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Family {
    /// `t^p`, `p > 0`: `quad` is 2, `cubic` 3, `quart` 4, `quint` 5.
    Power(f64),
    /// `1 - cos(tπ/2)`.
    Sine,
    /// `1 - √(1 - t²)`.
    Circ,
    /// `(2^(10t) - 1) / 1023`: exponential, meeting both ends exactly.
    Expo,
    /// `(o + 1)t³ - o·t²`: winds up by an overshoot `o ≥ 0` first.
    Back(f64),
    /// A sine of `period` that grows from nothing with the expo envelope, at
    /// `amplitude ≥ 1`, ending at exactly 1.
    Elastic {
        /// How far it swings, at least 1.
        amplitude: f64,
        /// The swing's period, in (0, 2].
        period: f64,
    },
    /// Four parabolas that bounce up to 1.
    Bounce,
}

/// The shapes by name, longest first so `in-out` is read before `in`.
pub const SHAPES: &[(&str, Shape)] = &[
    ("in-out", Shape::InOut),
    ("out-in", Shape::OutIn),
    ("in", Shape::In),
    ("out", Shape::Out),
];

/// The families by name, each with its default parameters.
pub const FAMILIES: &[(&str, Family)] = &[
    ("quad", Family::Power(2.0)),
    ("cubic", Family::Power(3.0)),
    ("quart", Family::Power(4.0)),
    ("quint", Family::Power(5.0)),
    ("sine", Family::Sine),
    ("circ", Family::Circ),
    ("expo", Family::Expo),
    ("back", Family::Back(BACK_OVERSHOOT)),
    (
        "elastic",
        Family::Elastic {
            amplitude: ELASTIC.0,
            period: ELASTIC.1,
        },
    ),
    ("bounce", Family::Bounce),
];

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
    /// A family's arguments are not closed, not numbers, or too many.
    FamilyForm,
    /// A family's parameter is outside its range, or a bare shape has no power.
    FamilyValue,
    /// `steps(…)` is not a whole number of at least 1, optionally with `start`.
    StepsForm,
    /// `spring(…)` is not a bounce in (-1, 1), optionally with a duration.
    SpringForm,
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
            Problem::FamilyForm | Problem::FamilyValue => {
                let why = match self {
                    Problem::FamilyForm => "its arguments are not one or two numbers in `( )`",
                    _ => {
                        "a power must be above 0, an overshoot at least 0, an amplitude at least 1 and a period in (0, 2]; a shape alone needs its power, as in `in-out(2)`"
                    }
                };
                return Diagnostic::syntax(format!("`{token}`: {why}")).expected(FAMILY_EXPECTED);
            }
            Problem::SpringForm => {
                return Diagnostic::syntax(format!(
                    "`{token}` needs a bounce above -1 and below 1, and may add a perceived duration"
                ))
                .expected(SPRING_EXPECTED);
            }
            Problem::StepsForm => {
                return Diagnostic::syntax(format!(
                    "`{token}` needs a whole number of steps of at least 1"
                ))
                .expected(STEPS_EXPECTED);
            }
            Problem::Unclosed => format!("`{token}` is missing its closing `)`"),
            Problem::Count => format!("`{token}` needs four numbers"),
            Problem::XRange => format!("`{token}`: x1 and x2 must be between 0 and 1"),
        };
        Diagnostic::syntax(message).expected(CUBIC_EXPECTED)
    }
}

/// A name, `cubic(…)`, `speed(…)`, `steps(…)` or a family's curve; once a
/// prefix is read, errors are about its arguments.
fn curve(input: &mut &str) -> Parsed<Curve, Problem> {
    alt((
        preceded(CUBIC_PREFIX, cut_err(cubic_arguments)),
        preceded(SPEED_PREFIX, cut_err(speed_arguments)),
        preceded(STEPS_PREFIX, cut_err(steps_arguments)),
        preceded(SPRING_PREFIX, cut_err(spring_arguments)),
        named,
        shaped,
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

/// `bounce)` or `bounce,perceived)`, the bounce in (-1, 1).
fn spring_arguments(input: &mut &str) -> Parsed<Curve, Problem> {
    terminated(
        (
            coordinate,
            opt(preceded(',', delimited(blank0, time, blank0))),
        ),
        (')', eof),
    )
    .verify(|(bounce, perceived): &(f64, Option<Time>)| {
        *bounce > -1.0
            && *bounce < 1.0
            && perceived.is_none_or(|t| matches!(t, Time::Frames(x) | Time::Seconds(x) if x > 0.0))
    })
    .map(|(bounce, perceived)| Curve::Spring(Spring { bounce, perceived }))
    .context(Problem::SpringForm)
    .parse_next(input)
}

/// `n)` or `n,start)`, `n` a whole number of at least 1.
fn steps_arguments(input: &mut &str) -> Parsed<Curve, Problem> {
    terminated((coordinate, opt(preceded(',', "start"))), (')', eof))
        .verify(|(n, _): &(f64, Option<&str>)| {
            *n >= 1.0 && n.fract() == 0.0 && *n <= f64::from(u32::MAX)
        })
        .map(|(n, start)| Curve::Steps {
            // Checked whole and in range above.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "checked whole, 1..=u32::MAX"
            )]
            count: n as u32,
            start: start.is_some(),
        })
        .context(Problem::StepsForm)
        .parse_next(input)
}

/// `<shape>[-<family>][(<numbers>)]`: a family's curve. Without a family the
/// shape takes a power, `in-out(2)`. A token that only starts like one, such
/// as `outback`, backtracks to the unknown-name error.
fn shaped(input: &mut &str) -> Parsed<Curve, Problem> {
    let (shape, family) =
        (table_word(SHAPES), opt(preceded('-', table_word(FAMILIES)))).parse_next(input)?;
    let numbers: Option<Vec<f64>> = alt((
        preceded(
            '(',
            cut_err(terminated(separated(1..=2, coordinate, ','), (')', eof)))
                .context(Problem::FamilyForm),
        )
        .map(Some),
        eof.value(None),
    ))
    .parse_next(input)?;
    cut_err(
        empty.try_map(|()| {
            family_curve(shape, family, numbers.as_deref()).ok_or(Problem::FamilyValue)
        }),
    )
    .parse_next(input)
}

/// One name of `table`, the longest that matches first.
fn table_word<T: Copy>(
    table: &'static [(&'static str, T)],
) -> impl FnMut(&mut &str) -> Parsed<T, Problem> {
    move |input: &mut &str| {
        let Some(&(name, value)) = table.iter().find(|(name, _)| input.starts_with(name)) else {
            return Err(winnow::error::ErrMode::from_input(input));
        };
        *input = &input[name.len()..];
        Ok(value)
    }
}

/// A family's curve from its parts, or `None` when a parameter is out of
/// its range or the family takes none.
fn family_curve(shape: Shape, family: Option<Family>, numbers: Option<&[f64]>) -> Option<Curve> {
    let family = match (family, numbers) {
        (None, Some(&[p])) if p > 0.0 => Family::Power(p),
        (Some(family), None) => family,
        (Some(Family::Back(_)), Some(&[o])) if o >= 0.0 => Family::Back(o),
        (Some(Family::Elastic { period, .. }), Some(&[a])) if a >= 1.0 => Family::Elastic {
            amplitude: a,
            period,
        },
        (Some(Family::Elastic { .. }), Some(&[a, p])) if a >= 1.0 && p > 0.0 && p <= 2.0 => {
            Family::Elastic {
                amplitude: a,
                period: p,
            }
        }
        _ => return None,
    };
    Some(Curve::Shaped(shape, family))
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
            Curve::Shaped(shape, family) => {
                let shape = name_of(SHAPES, shape);
                if let Some((name, _)) = FAMILIES.iter().find(|(_, f)| f == family) {
                    return write!(f, "{shape}-{name}");
                }
                match *family {
                    Family::Power(p) => write!(f, "{shape}({})", fmt_num(p)),
                    Family::Back(o) => write!(f, "{shape}-back({})", fmt_num(o)),
                    Family::Elastic { amplitude, period } => write!(
                        f,
                        "{shape}-elastic({},{})",
                        fmt_num(amplitude),
                        fmt_num(period)
                    ),
                    Family::Sine | Family::Circ | Family::Expo | Family::Bounce => {
                        unreachable!("a family without parameters is in FAMILIES")
                    }
                }
            }
            Curve::Spring(Spring { bounce, perceived }) => match perceived {
                Some(time) => write!(f, "spring({},{time})", fmt_num(*bounce)),
                None => write!(f, "spring({})", fmt_num(*bounce)),
            },
            Curve::Steps { count, start } => match start {
                true => write!(f, "steps({count},start)"),
                false => write!(f, "steps({count})"),
            },
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

/// The name `value` has in `table`.
fn name_of<T: PartialEq>(table: &'static [(&'static str, T)], value: &T) -> &'static str {
    table
        .iter()
        .find(|(_, v)| v == value)
        .map_or("", |(name, _)| name)
}

#[cfg(test)]
#[path = "../tests/unit/curve.rs"]
mod tests;
