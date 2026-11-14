//! Typed property values: a number, a point or a colour. Every value is a fixed
//! list of numbers, its components, so tracks interpolate any type the same way;
//! the type says how many components there are and how they are written.

use std::fmt;

use crate::grammar::{Parsed, number, parse_all};
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
}

impl ValueType {
    /// How many numbers a value of this type holds.
    pub const fn dims(self) -> usize {
        match self {
            ValueType::Number => 1,
            ValueType::Point => 2,
            ValueType::Color => 4,
        }
    }

    /// The accepted forms of a value of this type, with an example.
    pub const fn form(self) -> &'static str {
        match self {
            ValueType::Number => "a number, e.g. `0.5`",
            ValueType::Point => "a point, `[x, y]` in JSON or `x,y` in a string, e.g. `[540, 960]`",
            ValueType::Color => {
                "a colour: `#RGB`, `#RRGGBB`, `#RRGGBBAA`, or `[r, g, b, a]` with each channel 0 to 1"
            }
        }
    }
}

/// One value of a property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// A number.
    Number(f64),
    /// A point or size.
    Point([f64; 2]),
    /// A colour, each channel 0 to 1.
    Color([f64; 4]),
}

impl Value {
    /// The value's type.
    pub const fn value_type(&self) -> ValueType {
        match self {
            Value::Number(_) => ValueType::Number,
            Value::Point(_) => ValueType::Point,
            Value::Color(_) => ValueType::Color,
        }
    }

    /// The value's numbers, as many as its type's [`ValueType::dims`].
    pub fn components(&self) -> &[f64] {
        match self {
            Value::Number(n) => std::slice::from_ref(n),
            Value::Point(p) => p,
            Value::Color(c) => c,
        }
    }

    /// The value of type `ty` made of `components`.
    ///
    /// # Panics
    /// If `components` does not hold exactly `ty.dims()` numbers.
    pub fn from_components(ty: ValueType, components: &[f64]) -> Value {
        assert_eq!(
            components.len(),
            ty.dims(),
            "a {ty:?} has {} components",
            ty.dims()
        );
        match ty {
            ValueType::Number => Value::Number(components[0]),
            ValueType::Point => Value::Point([components[0], components[1]]),
            ValueType::Color => {
                Value::Color([components[0], components[1], components[2], components[3]])
            }
        }
    }

    /// Every component set to `n`; a number written for a point that broadcasts.
    pub fn splat(ty: ValueType, n: f64) -> Value {
        Value::from_components(ty, &vec![n; ty.dims()])
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

    /// Reads a value of type `ty` from its string form: a number, `x,y`, or a
    /// colour. With `broadcast`, a lone number sets every component.
    ///
    /// # Errors
    /// A syntax [`Diagnostic`] with the type's accepted form.
    pub fn parse(text: &str, ty: ValueType, broadcast: bool) -> Result<Value> {
        let grammar = |input: &mut &str| value(input, ty, broadcast);
        parse_all(text, grammar, |_: Option<()>| {
            Diagnostic::syntax(format!("`{text}` is not {}", article(ty))).expected(ty.form())
        })
    }

    /// Reads a value of type `ty` from the numbers a JSON array or number held.
    /// With `broadcast`, one number sets every component.
    ///
    /// # Errors
    /// A wrong-type [`Diagnostic`] when the count does not fit the type.
    pub fn from_numbers(numbers: &[f64], ty: ValueType, broadcast: bool) -> Result<Value> {
        match numbers {
            [n] if broadcast || ty == ValueType::Number => Ok(Value::splat(ty, *n)),
            // A colour may leave out alpha, which is then opaque.
            [r, g, b] if ty == ValueType::Color => Ok(Value::Color([*r, *g, *b, 1.0])),
            _ if numbers.len() == ty.dims() => Ok(Value::from_components(ty, numbers)),
            _ => Err(Diagnostic::new(
                crate::Kind::WrongType,
                format!("{} numbers do not make {}", numbers.len(), article(ty)),
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

/// "a number", "a point", "a colour", for messages.
fn article(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Number => "a number",
        ValueType::Point => "a point",
        ValueType::Color => "a colour",
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
        ValueType::Number | ValueType::Point => from_numbers.parse_next(input),
    }
}

impl fmt::Display for Value {
    /// The string form: `12`, `540,960`, `#FFE000` when every channel is a whole
    /// 8-bit step, else the channels as `r,g,b,a`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
