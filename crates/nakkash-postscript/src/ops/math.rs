//! Arithmetic, comparison and bitwise operators.
//!
//! Integer arithmetic stays integer while it fits 32 bits and turns real
//! where it would overflow, as the language defines it.

use crate::interp::{Interp, PsError, R};
use crate::object::{Object, Value};

/// Two operands as numbers, deeper first, and as integers when both are.
type Pair = (f64, f64, Option<(i64, i64)>);

/// The two numbers on top, deeper first, and whether both are integers.
fn two(i: &Interp) -> R<Pair> {
    let (a, b) = (i.peek(1)?, i.peek(0)?);
    let ints = match (a.value, b.value) {
        (Value::Int(x), Value::Int(y)) => Some((i64::from(x), i64::from(y))),
        _ => None,
    };
    let x = a.as_f64().ok_or(PsError::TypeCheck)?;
    let y = b.as_f64().ok_or(PsError::TypeCheck)?;
    Ok((x, y, ints))
}

/// Replaces the top `n` operands with `result`.
fn give(i: &mut Interp, n: usize, result: Object) -> R {
    i.drop_n(n);
    i.push(result)
}

/// A real result, which must be a number.
fn real(i: &mut Interp, n: usize, value: f64) -> R {
    if !value.is_finite() {
        return Err(PsError::UndefinedResult.into());
    }
    give(i, n, Object::real(value))
}

pub(crate) fn add(i: &mut Interp) -> R {
    match two(i)? {
        (_, _, Some((x, y))) => give(i, 2, Object::from_i64(x + y)),
        (x, y, None) => real(i, 2, x + y),
    }
}

pub(crate) fn sub(i: &mut Interp) -> R {
    match two(i)? {
        (_, _, Some((x, y))) => give(i, 2, Object::from_i64(x - y)),
        (x, y, None) => real(i, 2, x - y),
    }
}

pub(crate) fn mul(i: &mut Interp) -> R {
    match two(i)? {
        (_, _, Some((x, y))) => give(i, 2, Object::from_i64(x * y)),
        (x, y, None) => real(i, 2, x * y),
    }
}

pub(crate) fn div(i: &mut Interp) -> R {
    let (x, y, _) = two(i)?;
    if y == 0.0 {
        return Err(PsError::UndefinedResult.into());
    }
    real(i, 2, x / y)
}

pub(crate) fn idiv(i: &mut Interp) -> R {
    let (_, _, ints) = two(i)?;
    let (x, y) = ints.ok_or(PsError::TypeCheck)?;
    if y == 0 {
        return Err(PsError::UndefinedResult.into());
    }
    give(i, 2, Object::from_i64(x / y))
}

pub(crate) fn modulo(i: &mut Interp) -> R {
    let (_, _, ints) = two(i)?;
    let (x, y) = ints.ok_or(PsError::TypeCheck)?;
    if y == 0 {
        return Err(PsError::UndefinedResult.into());
    }
    give(i, 2, Object::from_i64(x % y))
}

/// A one-number operator that keeps integers integer where `int` says how.
fn unary(i: &mut Interp, int: fn(i64) -> i64, re: fn(f64) -> f64) -> R {
    match i.peek(0)?.value {
        Value::Int(x) => give(i, 1, Object::from_i64(int(i64::from(x)))),
        Value::Real(x) => real(i, 1, re(x)),
        _ => Err(PsError::TypeCheck.into()),
    }
}

pub(crate) fn abs(i: &mut Interp) -> R {
    unary(i, i64::abs, f64::abs)
}

pub(crate) fn neg(i: &mut Interp) -> R {
    unary(i, |x| -x, |x| -x)
}

pub(crate) fn ceiling(i: &mut Interp) -> R {
    unary(i, |x| x, f64::ceil)
}

pub(crate) fn floor(i: &mut Interp) -> R {
    unary(i, |x| x, f64::floor)
}

pub(crate) fn round(i: &mut Interp) -> R {
    // Halves round up, towards positive infinity.
    unary(i, |x| x, |x| (x + 0.5).floor())
}

pub(crate) fn truncate(i: &mut Interp) -> R {
    unary(i, |x| x, f64::trunc)
}

/// A one-number operator with a real result.
fn real_fn(i: &mut Interp, f: fn(f64) -> Option<f64>) -> R {
    let x = i.num(0)?;
    let value = f(x).ok_or(PsError::RangeCheck)?;
    real(i, 1, value)
}

pub(crate) fn sqrt(i: &mut Interp) -> R {
    real_fn(i, |x| (x >= 0.0).then(|| x.sqrt()))
}

pub(crate) fn exp(i: &mut Interp) -> R {
    let (x, y, _) = two(i)?;
    if x == 0.0 && y < 0.0 {
        return Err(PsError::UndefinedResult.into());
    }
    real(i, 2, x.powf(y))
}

pub(crate) fn ln(i: &mut Interp) -> R {
    real_fn(i, |x| (x > 0.0).then(|| x.ln()))
}

pub(crate) fn log(i: &mut Interp) -> R {
    real_fn(i, |x| (x > 0.0).then(|| x.log10()))
}

pub(crate) fn atan(i: &mut Interp) -> R {
    let (num, den, _) = two(i)?;
    if num == 0.0 && den == 0.0 {
        return Err(PsError::UndefinedResult.into());
    }
    let degrees = num.atan2(den).to_degrees().rem_euclid(360.0);
    real(i, 2, degrees)
}

pub(crate) fn cos(i: &mut Interp) -> R {
    real_fn(i, |x| Some(x.to_radians().cos()))
}

pub(crate) fn sin(i: &mut Interp) -> R {
    real_fn(i, |x| Some(x.to_radians().sin()))
}

/// The state of the pseudo-random generator, in `$error`'s neighbour:
/// kept on the interpreter as a plain integer.
fn next_random(i: &mut Interp) -> i32 {
    // A Park–Miller generator: the same sequence on every machine.
    let state = i64::from(i.random.max(1));
    let next = (state * 16_807) % 2_147_483_647;
    i.random = i32::try_from(next).unwrap_or(1);
    i.random
}

pub(crate) fn rand(i: &mut Interp) -> R {
    let value = next_random(i);
    i.push(Object::int(value))
}

pub(crate) fn srand(i: &mut Interp) -> R {
    let seed = i.int(0)?;
    i.drop_n(1);
    i.random = seed;
    Ok(())
}

pub(crate) fn rrand(i: &mut Interp) -> R {
    let state = i.random;
    i.push(Object::int(state))
}

/// Whether two objects are equal as `eq` compares them: numbers by value,
/// strings by content (and against names of the same text), the rest by
/// identity.
pub(crate) fn equal(i: &Interp, a: Object, b: Object) -> bool {
    if let (Some(x), Some(y)) = (a.as_f64(), b.as_f64()) {
        return x == y;
    }
    match (a.value, b.value) {
        (Value::String(x), Value::String(y)) => i.vm.str_bytes(x) == i.vm.str_bytes(y),
        (Value::String(s), Value::Name(n)) | (Value::Name(n), Value::String(s)) => {
            i.vm.str_bytes(s) == i.vm.name_bytes(n)
        }
        (Value::Array(x) | Value::Packed(x), Value::Array(y) | Value::Packed(y)) => x == y,
        (x, y) => x == y,
    }
}

pub(crate) fn eq(i: &mut Interp) -> R {
    let (a, b) = (i.peek(1)?, i.peek(0)?);
    let result = equal(i, a, b);
    give(i, 2, Object::bool(result))
}

pub(crate) fn ne(i: &mut Interp) -> R {
    let (a, b) = (i.peek(1)?, i.peek(0)?);
    let result = !equal(i, a, b);
    give(i, 2, Object::bool(result))
}

/// Orders two numbers or two strings.
fn order(i: &mut Interp, keep: fn(std::cmp::Ordering) -> bool) -> R {
    let (a, b) = (i.peek(1)?, i.peek(0)?);
    let ordering = match (a.value, b.value) {
        (Value::String(x), Value::String(y)) => i.vm.str_bytes(x).cmp(i.vm.str_bytes(y)),
        _ => {
            let x = a.as_f64().ok_or(PsError::TypeCheck)?;
            let y = b.as_f64().ok_or(PsError::TypeCheck)?;
            x.partial_cmp(&y).ok_or(PsError::UndefinedResult)?
        }
    };
    give(i, 2, Object::bool(keep(ordering)))
}

pub(crate) fn ge(i: &mut Interp) -> R {
    order(i, std::cmp::Ordering::is_ge)
}

pub(crate) fn gt(i: &mut Interp) -> R {
    order(i, std::cmp::Ordering::is_gt)
}

pub(crate) fn le(i: &mut Interp) -> R {
    order(i, std::cmp::Ordering::is_le)
}

pub(crate) fn lt(i: &mut Interp) -> R {
    order(i, std::cmp::Ordering::is_lt)
}

/// A bitwise or logical operator on two booleans or two integers.
fn logic(i: &mut Interp, b: fn(bool, bool) -> bool, n: fn(i32, i32) -> i32) -> R {
    let result = match (i.peek(1)?.value, i.peek(0)?.value) {
        (Value::Bool(x), Value::Bool(y)) => Object::bool(b(x, y)),
        (Value::Int(x), Value::Int(y)) => Object::int(n(x, y)),
        _ => return Err(PsError::TypeCheck.into()),
    };
    give(i, 2, result)
}

pub(crate) fn and(i: &mut Interp) -> R {
    logic(i, |x, y| x & y, |x, y| x & y)
}

pub(crate) fn or(i: &mut Interp) -> R {
    logic(i, |x, y| x | y, |x, y| x | y)
}

pub(crate) fn xor(i: &mut Interp) -> R {
    logic(i, |x, y| x ^ y, |x, y| x ^ y)
}

pub(crate) fn not(i: &mut Interp) -> R {
    let result = match i.peek(0)?.value {
        Value::Bool(x) => Object::bool(!x),
        Value::Int(x) => Object::int(!x),
        _ => return Err(PsError::TypeCheck.into()),
    };
    give(i, 1, result)
}

pub(crate) fn bitshift(i: &mut Interp) -> R {
    let (value, shift) = (i.int(1)?, i.int(0)?);
    // The 32 bits shift as unsigned, so a right shift brings in zeros.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let bits = value as u32;
    let result = match shift {
        s if s >= 32 || s <= -32 => 0,
        s if s >= 0 => bits << s,
        s => bits >> -s,
    };
    #[allow(clippy::cast_possible_wrap)]
    give(i, 2, Object::int(result as i32))
}
