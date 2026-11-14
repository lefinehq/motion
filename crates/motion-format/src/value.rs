//! Typed property values: a number, a point, a colour or a path. Every value is
//! a list of numbers, its components, so tracks interpolate any type the same
//! way; the type says how many components there are and how they are written.
//! A number, point or colour has a fixed count; a path has six per vertex.

use std::fmt;

use crate::grammar::{Parsed, number, parse_all};
use crate::path::{PATH_FORM, Path};
use crate::units::{Color, hex_color};
use crate::{Diagnostic, Result, fmt_num};
use winnow::combinator::{alt, eof, separated, terminated};
use winnow::prelude::*;

/// What sort of value a property holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueType {
    /// One number.
    Number,
    /// A 2D point or size, `[x, y]`.
    Point,
    /// A colour: red, green, blue and alpha, each 0 to 1, alpha not premultiplied.
    Color,
    /// A bézier path ([`Path`]).
    Path,
}

/// What each [`ValueType`] is, one row per variant in declaration order.
struct TypeSpec {
    /// How many numbers a value holds, unless it varies by value.
    dims: Option<usize>,
    /// The accepted forms, with an example.
    form: &'static str,
    /// The type with its article, for messages.
    noun: &'static str,
}

const TYPES: [TypeSpec; 4] = [
    TypeSpec {
        dims: Some(1),
        form: "a number, e.g. `0.5`",
        noun: "a number",
    },
    TypeSpec {
        dims: Some(2),
        form: "a point, `[x, y]` in JSON or `x,y` in a string, e.g. `[540, 960]`",
        noun: "a point",
    },
    TypeSpec {
        dims: Some(4),
        form: "a colour: `#RGB`, `#RRGGBB`, `#RRGGBBAA`, or `[r, g, b, a]` with each channel 0 to 1",
        noun: "a colour",
    },
    TypeSpec {
        dims: None,
        form: PATH_FORM,
        noun: "a path",
    },
];

impl ValueType {
    const fn spec(self) -> &'static TypeSpec {
        &TYPES[self as usize]
    }

    /// How many numbers a value of this type holds: `None` for a path, whose
    /// count is six per vertex.
    pub const fn dims(self) -> Option<usize> {
        self.spec().dims
    }

    /// The accepted forms of a value of this type, with an example.
    pub const fn form(self) -> &'static str {
        self.spec().form
    }

    /// The type with its article, for messages: "a point".
    pub const fn noun(self) -> &'static str {
        self.spec().noun
    }
}

/// One value of a property.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A number.
    Number(f64),
    /// A point or size.
    Point([f64; 2]),
    /// A colour, each channel 0 to 1.
    Color([f64; 4]),
    /// A bézier path.
    Path(Path),
}

impl Value {
    /// The value's type.
    pub const fn value_type(&self) -> ValueType {
        match self {
            Value::Number(_) => ValueType::Number,
            Value::Point(_) => ValueType::Point,
            Value::Color(_) => ValueType::Color,
            Value::Path(_) => ValueType::Path,
        }
    }

    /// The value's numbers: as many as its type's [`ValueType::dims`], or six
    /// per vertex of a path.
    pub fn components(&self) -> &[f64] {
        match self {
            Value::Number(n) => std::slice::from_ref(n),
            Value::Point(p) => p,
            Value::Color(c) => c,
            Value::Path(p) => p.components(),
        }
    }

    /// The same type and shape of value made of `components`: what a track
    /// gives between two keys of one shape.
    ///
    /// # Panics
    /// If `components` is not as long as [`Value::components`].
    pub fn with_components(&self, components: &[f64]) -> Value {
        match self {
            Value::Path(path) => Value::Path(path.with_components(components)),
            _ => Value::from_components(self.value_type(), components),
        }
    }

    /// Whether a track can move between this value and `other`: the same
    /// type and, for paths, the same shape.
    pub fn same_shape(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Path(a), Value::Path(b)) => a.same_shape(b),
            _ => self.value_type() == other.value_type(),
        }
    }

    /// The value of fixed-size type `ty` made of `components`.
    ///
    /// # Panics
    /// If `ty` is [`ValueType::Path`], whose shape the numbers alone do not
    /// give (use [`Value::with_components`]), or `components` does not hold
    /// exactly `ty.dims()` numbers.
    pub fn from_components(ty: ValueType, components: &[f64]) -> Value {
        let dims = ty.dims().expect("a fixed-size type");
        assert_eq!(components.len(), dims, "a {ty:?} has {dims} components");
        match ty {
            ValueType::Number => Value::Number(components[0]),
            ValueType::Point => Value::Point([components[0], components[1]]),
            ValueType::Color => {
                Value::Color([components[0], components[1], components[2], components[3]])
            }
            ValueType::Path => unreachable!("dims checked above"),
        }
    }

    /// Every component of fixed-size type `ty` set to `n`; a number written
    /// for a point that broadcasts.
    ///
    /// # Panics
    /// If `ty` is [`ValueType::Path`].
    pub fn splat(ty: ValueType, n: f64) -> Value {
        let dims = ty.dims().expect("a fixed-size type");
        Value::from_components(ty, &vec![n; dims])
    }

    /// The number, for a [`Value::Number`].
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The point, for a [`Value::Point`].
    pub fn as_point(&self) -> Option<[f64; 2]> {
        match self {
            Value::Point(p) => Some(*p),
            _ => None,
        }
    }

    /// The path, for a [`Value::Path`].
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Value::Path(p) => Some(p),
            _ => None,
        }
    }

    /// Reads a value of type `ty` from its string form: a number, `x,y`, a
    /// colour or path data. With `broadcast`, a lone number sets every component.
    ///
    /// # Errors
    /// A syntax [`Diagnostic`] with the type's accepted form.
    pub fn parse(text: &str, ty: ValueType, broadcast: bool) -> Result<Value> {
        if ty == ValueType::Path {
            return text.parse().map(Value::Path);
        }
        let grammar = |input: &mut &str| value(input, ty, broadcast);
        parse_all(text, grammar, |_: Option<()>| {
            Diagnostic::syntax(format!("`{text}` is not {}", ty.noun())).expected(ty.form())
        })
    }

    /// Reads a value of type `ty` from the numbers a JSON array or number held.
    /// With `broadcast`, one number sets every component.
    ///
    /// # Errors
    /// A wrong-type [`Diagnostic`] when the count does not fit the type, and
    /// always for a path, which is written as path data.
    pub fn from_numbers(numbers: &[f64], ty: ValueType, broadcast: bool) -> Result<Value> {
        match numbers {
            _ if ty == ValueType::Path => Err(Diagnostic::new(
                crate::Kind::WrongType,
                "a path is written as a string of path data, not numbers",
            )
            .expected(ty.form())),
            [n] if broadcast || ty == ValueType::Number => Ok(Value::splat(ty, *n)),
            // A colour may leave out alpha, which is then opaque.
            [r, g, b] if ty == ValueType::Color => Ok(Value::Color([*r, *g, *b, 1.0])),
            _ if Some(numbers.len()) == ty.dims() => Ok(Value::from_components(ty, numbers)),
            _ => Err(Diagnostic::new(
                crate::Kind::WrongType,
                format!("{} numbers do not make {}", numbers.len(), ty.noun()),
            )
            .expected(ty.form())),
        }
    }
}

impl From<Color> for Value {
    fn from(c: Color) -> Value {
        let channel = |v: u8| f64::from(v) / 255.0;
        Value::Color([channel(c.r), channel(c.g), channel(c.b), channel(c.a)])
    }
}

/// The string form of a value of type `ty`, which must end the input.
fn value(input: &mut &str, ty: ValueType, broadcast: bool) -> Parsed<Value, ()> {
    let numbers = |input: &mut &str| -> Parsed<Vec<f64>, ()> {
        terminated(separated(1..=4, number, ','), eof).parse_next(input)
    };
    let mut from_numbers =
        numbers.verify_map(|n: Vec<f64>| Value::from_numbers(&n, ty, broadcast).ok());
    match ty {
        ValueType::Color => {
            alt((terminated(hex_color, eof).map(Value::from), from_numbers)).parse_next(input)
        }
        ValueType::Number | ValueType::Point | ValueType::Path => from_numbers.parse_next(input),
    }
}

impl fmt::Display for Value {
    /// The string form: `12`, `540,960`, `#FFE000` when every channel is a whole
    /// 8-bit step, else the channels as `r,g,b,a`, and path data for a path.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Value::Path(path) = self {
            return path.fmt(f);
        }
        if let Value::Color(c) = self
            && let Some(hex) = as_hex(c)
        {
            return f.write_str(&hex);
        }
        let words: Vec<String> = self.components().iter().map(|&n| fmt_num(n)).collect();
        f.write_str(&words.join(","))
    }
}

/// `#RRGGBB` or `#RRGGBBAA` when every channel is exactly `k / 255`.
fn as_hex(channels: &[f64; 4]) -> Option<String> {
    let bytes: Option<Vec<u8>> = channels
        .iter()
        .map(|&c| {
            let scaled = c * 255.0;
            let byte = scaled.round();
            ((0.0..=255.0).contains(&byte) && (scaled - byte).abs() < 1e-9).then_some(byte as u8)
        })
        .collect();
    let bytes = bytes?;
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    Some(match bytes[3] {
        255 => format!("#{}", &hex[..6]),
        _ => format!("#{hex}"),
    })
}

#[cfg(test)]
#[path = "../tests/unit/value.rs"]
mod tests;
