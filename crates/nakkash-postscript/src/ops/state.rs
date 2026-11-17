//! The graphics state stack, `save` and `restore`, the line style, and the
//! matrix operators.
//!
//! `save` and `restore` keep and bring back the graphics state only:
//! changes to memory made in between stay, which a program that draws
//! does not notice.

use kurbo::{Affine, Point, Vec2};

use crate::graphics::{Cap, Join};
use crate::interp::{Interp, PsError, R};
use crate::object::{Arr, Object, Value};

pub(crate) fn gsave(i: &mut Interp) -> R {
    if i.gstack.len() >= i.limits.gsaves {
        return Err(PsError::LimitCheck.into());
    }
    i.gstack.push((i.g.clone(), u32::MAX));
    Ok(())
}

pub(crate) fn grestore(i: &mut Interp) -> R {
    // A state `save` kept is brought back but not popped by `grestore`.
    match i.gstack.last() {
        Some((state, level)) if *level == u32::MAX => {
            i.g = state.clone();
            i.gstack.pop();
        }
        Some((state, _)) => i.g = state.clone(),
        None => {}
    }
    Ok(())
}

pub(crate) fn grestoreall(i: &mut Interp) -> R {
    while let Some((state, level)) = i.gstack.last() {
        i.g = state.clone();
        if *level != u32::MAX {
            break;
        }
        i.gstack.pop();
    }
    Ok(())
}

pub(crate) fn initgraphics(i: &mut Interp) -> R {
    let ctm = i.out.initial_matrix();
    i.g = crate::graphics::GState::new(ctm);
    Ok(())
}

pub(crate) fn gstate(i: &mut Interp) -> R {
    let id = u32::try_from(i.gstates.len()).map_err(|_| PsError::LimitCheck)?;
    i.vm.charge(64)?;
    i.gstates.push(i.g.clone());
    i.push(Object::lit(Value::GState(id)))
}

pub(crate) fn setgstate(i: &mut Interp) -> R {
    let Value::GState(id) = i.peek(0)?.value else {
        return Err(PsError::TypeCheck.into());
    };
    i.g = i.gstates[id as usize].clone();
    i.drop_n(1);
    Ok(())
}

pub(crate) fn currentgstate(i: &mut Interp) -> R {
    let Value::GState(id) = i.peek(0)?.value else {
        return Err(PsError::TypeCheck.into());
    };
    i.gstates[id as usize] = i.g.clone();
    Ok(())
}

pub(crate) fn save(i: &mut Interp) -> R {
    if i.gstack.len() >= i.limits.gsaves {
        return Err(PsError::LimitCheck.into());
    }
    i.save_level += 1;
    let level = i.save_level;
    i.gstack.push((i.g.clone(), level));
    i.push(Object::lit(Value::Save(level)))
}

pub(crate) fn restore(i: &mut Interp) -> R {
    let Value::Save(level) = i.peek(0)?.value else {
        return Err(PsError::TypeCheck.into());
    };
    if level > i.save_level {
        return Err(PsError::InvalidRestore.into());
    }
    i.drop_n(1);
    while let Some((state, at)) = i.gstack.pop() {
        if at == level {
            i.g = state;
            break;
        }
    }
    i.save_level = level - 1;
    Ok(())
}

/// Sets one number of the state.
fn set_number(i: &mut Interp, set: fn(&mut crate::graphics::GState, f64)) -> R {
    let value = i.num(0)?;
    i.drop_n(1);
    set(&mut i.g, value);
    Ok(())
}

pub(crate) fn setlinewidth(i: &mut Interp) -> R {
    set_number(i, |g, v| g.line_width = v.abs())
}

pub(crate) fn currentlinewidth(i: &mut Interp) -> R {
    let value = i.g.line_width;
    i.push(Object::real(value))
}

pub(crate) fn setlinecap(i: &mut Interp) -> R {
    let cap = match i.int(0)? {
        0 => Cap::Butt,
        1 => Cap::Round,
        2 => Cap::Square,
        _ => return Err(PsError::RangeCheck.into()),
    };
    i.drop_n(1);
    i.g.cap = cap;
    Ok(())
}

pub(crate) fn currentlinecap(i: &mut Interp) -> R {
    let n = match i.g.cap {
        Cap::Butt => 0,
        Cap::Round => 1,
        Cap::Square => 2,
    };
    i.push(Object::int(n))
}

pub(crate) fn setlinejoin(i: &mut Interp) -> R {
    let join = match i.int(0)? {
        0 => Join::Miter,
        1 => Join::Round,
        2 => Join::Bevel,
        _ => return Err(PsError::RangeCheck.into()),
    };
    i.drop_n(1);
    i.g.join = join;
    Ok(())
}

pub(crate) fn currentlinejoin(i: &mut Interp) -> R {
    let n = match i.g.join {
        Join::Miter => 0,
        Join::Round => 1,
        Join::Bevel => 2,
    };
    i.push(Object::int(n))
}

pub(crate) fn setmiterlimit(i: &mut Interp) -> R {
    if i.num(0)? < 1.0 {
        return Err(PsError::RangeCheck.into());
    }
    set_number(i, |g, v| g.miter = v)
}

pub(crate) fn currentmiterlimit(i: &mut Interp) -> R {
    let value = i.g.miter;
    i.push(Object::real(value))
}

pub(crate) fn setdash(i: &mut Interp) -> R {
    let (arr, offset) = (i.array(1)?, i.num(0)?);
    let dash = i.numbers(arr)?;
    if dash.iter().any(|d| *d < 0.0) || (!dash.is_empty() && dash.iter().all(|d| *d == 0.0)) {
        return Err(PsError::RangeCheck.into());
    }
    i.drop_n(2);
    i.g.dash = dash;
    i.g.dash_offset = offset;
    Ok(())
}

pub(crate) fn currentdash(i: &mut Interp) -> R {
    let dash: Vec<Object> = i.g.dash.iter().map(|d| Object::number(*d)).collect();
    let offset = i.g.dash_offset;
    let arr = i.vm.new_array(dash)?;
    i.push(Object::lit(Value::Array(arr)))?;
    i.push(Object::number(offset))
}

pub(crate) fn setflat(i: &mut Interp) -> R {
    set_number(i, |g, v| g.flat = v.clamp(0.2, 100.0))
}

pub(crate) fn currentflat(i: &mut Interp) -> R {
    let value = i.g.flat;
    i.push(Object::real(value))
}

pub(crate) fn setsmoothness(i: &mut Interp) -> R {
    set_number(i, |g, v| g.smoothness = v.clamp(0.0, 1.0))
}

pub(crate) fn currentsmoothness(i: &mut Interp) -> R {
    let value = i.g.smoothness;
    i.push(Object::real(value))
}

pub(crate) fn setstrokeadjust(i: &mut Interp) -> R {
    i.g.stroke_adjust = i.boolean(0)?;
    i.drop_n(1);
    Ok(())
}

pub(crate) fn currentstrokeadjust(i: &mut Interp) -> R {
    let value = i.g.stroke_adjust;
    i.push(Object::bool(value))
}

pub(crate) fn setoverprint(i: &mut Interp) -> R {
    i.g.overprint = i.boolean(0)?;
    i.drop_n(1);
    Ok(())
}

pub(crate) fn currentoverprint(i: &mut Interp) -> R {
    let value = i.g.overprint;
    i.push(Object::bool(value))
}

// ----- matrices ------------------------------------------------------------

/// The six numbers of a matrix array.
pub(crate) fn matrix_of(i: &Interp, arr: Arr) -> R<Affine> {
    if arr.len != 6 {
        return Err(PsError::RangeCheck.into());
    }
    let n = i.numbers(arr)?;
    Ok(Affine::new([n[0], n[1], n[2], n[3], n[4], n[5]]))
}

/// The matrix operand `depth` places below the top.
pub(crate) fn matrix_at(i: &Interp, depth: usize) -> R<Affine> {
    let arr = i.array(depth)?;
    matrix_of(i, arr)
}

/// Writes `m` into the matrix array `arr`.
fn store_matrix(i: &mut Interp, arr: Arr, m: Affine) -> R {
    if arr.len != 6 {
        return Err(PsError::RangeCheck.into());
    }
    let values = m.as_coeffs().map(Object::real);
    i.vm.arr_mut(arr).copy_from_slice(&values);
    Ok(())
}

/// A new matrix array holding `m`.
pub(crate) fn new_matrix(i: &mut Interp, m: Affine) -> R<Object> {
    let arr = i.vm.new_array(m.as_coeffs().map(Object::real).to_vec())?;
    Ok(Object::lit(Value::Array(arr)))
}

pub(crate) fn matrix(i: &mut Interp) -> R {
    let m = new_matrix(i, Affine::IDENTITY)?;
    i.push(m)
}

pub(crate) fn initmatrix(i: &mut Interp) -> R {
    i.g.ctm = i.out.initial_matrix();
    Ok(())
}

/// Fills the matrix on top with `m` and leaves it there.
fn fill_matrix(i: &mut Interp, m: Affine) -> R {
    let arr = i.array(0)?;
    store_matrix(i, arr, m)
}

pub(crate) fn identmatrix(i: &mut Interp) -> R {
    fill_matrix(i, Affine::IDENTITY)
}

pub(crate) fn defaultmatrix(i: &mut Interp) -> R {
    let m = i.out.initial_matrix();
    fill_matrix(i, m)
}

pub(crate) fn currentmatrix(i: &mut Interp) -> R {
    let m = i.g.ctm;
    fill_matrix(i, m)
}

pub(crate) fn setmatrix(i: &mut Interp) -> R {
    let m = matrix_at(i, 0)?;
    i.drop_n(1);
    i.g.ctm = m;
    Ok(())
}

/// A matrix operator that either changes the CTM or, given a matrix
/// operand, fills that matrix: `x y translate` or `x y matrix translate`.
fn transform_op(i: &mut Interp, numbers: usize, make: fn(&[f64]) -> Affine) -> R {
    let with_matrix = matches!(i.peek(0)?.value, Value::Array(_) | Value::Packed(_));
    let skip = usize::from(with_matrix);
    let mut args = Vec::with_capacity(numbers);
    for depth in (skip..skip + numbers).rev() {
        args.push(i.num(depth)?);
    }
    let m = make(&args);
    if with_matrix {
        let arr = i.array(0)?;
        store_matrix(i, arr, m)?;
        let result = i.peek(0)?;
        i.drop_n(numbers + 1);
        i.push(result)
    } else {
        i.drop_n(numbers);
        i.g.ctm *= m;
        Ok(())
    }
}

pub(crate) fn translate(i: &mut Interp) -> R {
    transform_op(i, 2, |a| Affine::translate((a[0], a[1])))
}

pub(crate) fn scale(i: &mut Interp) -> R {
    transform_op(i, 2, |a| Affine::scale_non_uniform(a[0], a[1]))
}

pub(crate) fn rotate(i: &mut Interp) -> R {
    transform_op(i, 1, |a| rotation(a[0]))
}

/// A rotation by `degrees`, exact at the quarter turns.
pub(crate) fn rotation(degrees: f64) -> Affine {
    let turn = degrees.rem_euclid(360.0);
    let (sin, cos) = if turn == 0.0 {
        (0.0, 1.0)
    } else if turn == 90.0 {
        (1.0, 0.0)
    } else if turn == 180.0 {
        (0.0, -1.0)
    } else if turn == 270.0 {
        (-1.0, 0.0)
    } else {
        degrees.to_radians().sin_cos()
    };
    Affine::new([cos, sin, -sin, cos, 0.0, 0.0])
}

pub(crate) fn concat(i: &mut Interp) -> R {
    let m = matrix_at(i, 0)?;
    i.drop_n(1);
    i.g.ctm *= m;
    Ok(())
}

pub(crate) fn concatmatrix(i: &mut Interp) -> R {
    let (a, b) = (matrix_at(i, 2)?, matrix_at(i, 1)?);
    let target = i.array(0)?;
    // The language multiplies row vectors: m1 m2 concatmatrix applies m1
    // first, which is m2 × m1 in kurbo's column convention.
    store_matrix(i, target, b * a)?;
    let result = i.peek(0)?;
    i.drop_n(3);
    i.push(result)
}

/// A point operator that uses the CTM, or a given matrix.
fn point_op(i: &mut Interp, apply: fn(Affine, f64, f64) -> R<(f64, f64)>) -> R {
    let with_matrix = matches!(i.peek(0)?.value, Value::Array(_) | Value::Packed(_));
    let (m, skip) = if with_matrix {
        (matrix_at(i, 0)?, 1)
    } else {
        (i.g.ctm, 0)
    };
    let (x, y) = (i.num(skip + 1)?, i.num(skip)?);
    let (u, v) = apply(m, x, y)?;
    i.drop_n(2 + skip);
    i.push(Object::real(u))?;
    i.push(Object::real(v))
}

/// The inverse of `m`, or an error when it has none.
pub(crate) fn inverse(m: Affine) -> R<Affine> {
    let det = m.determinant();
    if det == 0.0 || !det.is_finite() {
        return Err(PsError::UndefinedResult.into());
    }
    Ok(m.inverse())
}

pub(crate) fn transform(i: &mut Interp) -> R {
    point_op(i, |m, x, y| {
        let p = m * Point::new(x, y);
        Ok((p.x, p.y))
    })
}

pub(crate) fn itransform(i: &mut Interp) -> R {
    point_op(i, |m, x, y| {
        let p = inverse(m)? * Point::new(x, y);
        Ok((p.x, p.y))
    })
}

/// A distance under `m`: the matrix without its translation.
fn distance(m: Affine, x: f64, y: f64) -> Vec2 {
    let c = m.as_coeffs();
    Vec2::new(c[0] * x + c[2] * y, c[1] * x + c[3] * y)
}

pub(crate) fn dtransform(i: &mut Interp) -> R {
    point_op(i, |m, x, y| {
        let d = distance(m, x, y);
        Ok((d.x, d.y))
    })
}

pub(crate) fn idtransform(i: &mut Interp) -> R {
    point_op(i, |m, x, y| {
        let d = distance(inverse(m)?, x, y);
        Ok((d.x, d.y))
    })
}

pub(crate) fn invertmatrix(i: &mut Interp) -> R {
    let m = matrix_at(i, 1)?;
    let target = i.array(0)?;
    store_matrix(i, target, inverse(m)?)?;
    let result = i.peek(0)?;
    i.drop_n(2);
    i.push(result)
}
