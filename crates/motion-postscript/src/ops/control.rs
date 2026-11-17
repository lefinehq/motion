//! Control: `exec`, conditionals, loops, `exit`, `stop` and `stopped`.
//!
//! Each loop is a frame on the execution stack that pushes its body once
//! per turn, so loops nest without Rust recursion and `exit` only has to
//! cut the stack back to the innermost loop frame.

use crate::interp::{Fault, Frame, Halt, Interp, Over, PsError, R};
use crate::object::{Object, Value};

pub(crate) fn nothing(_: &mut Interp) -> R {
    Ok(())
}

pub(crate) fn exec(i: &mut Interp) -> R {
    let object = i.pop()?;
    i.push_frame(Frame::Exec(object))
}

pub(crate) fn if_(i: &mut Interp) -> R {
    let (condition, body) = (i.boolean(1)?, i.proc(0)?);
    i.drop_n(2);
    if condition {
        i.push_frame(Frame::Exec(body))?;
    }
    Ok(())
}

pub(crate) fn ifelse(i: &mut Interp) -> R {
    let (condition, yes, no) = (i.boolean(2)?, i.proc(1)?, i.proc(0)?);
    i.drop_n(3);
    i.push_frame(Frame::Exec(if condition { yes } else { no }))
}

pub(crate) fn for_(i: &mut Interp) -> R {
    let (start, step, limit, body) = (i.peek(3)?, i.peek(2)?, i.peek(1)?, i.proc(0)?);
    let frame = match (start.value, step.value, limit.value) {
        (Value::Int(a), Value::Int(b), Value::Int(c)) => Frame::ForInt {
            next: i64::from(a),
            step: i64::from(b),
            limit: i64::from(c),
            body,
        },
        _ => Frame::ForReal {
            next: start.as_f64().ok_or(PsError::TypeCheck)?,
            step: step.as_f64().ok_or(PsError::TypeCheck)?,
            limit: limit.as_f64().ok_or(PsError::TypeCheck)?,
            body,
            count: 0,
        },
    };
    i.drop_n(4);
    i.push_frame(frame)
}

pub(crate) fn repeat(i: &mut Interp) -> R {
    let (count, body) = (i.int(1)?, i.proc(0)?);
    if count < 0 {
        return Err(PsError::RangeCheck.into());
    }
    i.drop_n(2);
    i.push_frame(Frame::Repeat {
        left: i64::from(count),
        body,
    })
}

pub(crate) fn loop_(i: &mut Interp) -> R {
    let body = i.proc(0)?;
    i.drop_n(1);
    i.push_frame(Frame::Loop { body })
}

pub(crate) fn forall(i: &mut Interp) -> R {
    let (container, body) = (i.peek(1)?, i.proc(0)?);
    let over = match container.value {
        Value::Array(arr) | Value::Packed(arr) => Over::Array(arr),
        Value::String(s) => Over::String(s),
        Value::Dict(dict) => Over::Dict(dict),
        _ => return Err(PsError::TypeCheck.into()),
    };
    i.drop_n(2);
    i.push_frame(Frame::Forall {
        over,
        index: 0,
        body,
    })
}

/// `exit`: leaves the innermost loop. A `stopped` context, or the edge of
/// the current nested run, between here and the loop is an error.
pub(crate) fn exit(i: &mut Interp) -> R {
    for index in (i.floor..i.estack.len()).rev() {
        match i.estack[index] {
            Frame::ForInt { .. }
            | Frame::ForReal { .. }
            | Frame::Repeat { .. }
            | Frame::Loop { .. }
            | Frame::Forall { .. }
            | Frame::PathForall { .. } => {
                i.estack.truncate(index);
                return Ok(());
            }
            Frame::Stopped => break,
            _ => {}
        }
    }
    Err(PsError::InvalidExit.into())
}

pub(crate) fn stop(_: &mut Interp) -> R {
    Err(Fault::Stop)
}

pub(crate) fn stopped(i: &mut Interp) -> R {
    let body = i.pop()?;
    i.push_frame(Frame::Stopped)?;
    i.push_frame(Frame::Exec(body))
}

pub(crate) fn countexecstack(i: &mut Interp) -> R {
    let n = i32::try_from(i.estack.len()).unwrap_or(i32::MAX);
    i.push(Object::int(n))
}

/// `execstack`: fills the array with nulls, one per frame; nothing here
/// lets a program look inside the interpreter's frames.
pub(crate) fn execstack(i: &mut Interp) -> R {
    let arr = i.array(0)?;
    let n = i.estack.len();
    if n > arr.len as usize {
        return Err(PsError::RangeCheck.into());
    }
    i.drop_n(1);
    let len = u32::try_from(n).map_err(|_| PsError::RangeCheck)?;
    i.push(Object::lit(Value::Array(crate::object::Arr { len, ..arr })))
}

pub(crate) fn quit(_: &mut Interp) -> R {
    Err(Fault::Halt(Halt::Quit))
}

/// The body of the standard error handlers: `command /name .error`
/// records the error in `$error` and stops.
pub(crate) fn error(i: &mut Interp) -> R {
    let name = i.pop()?;
    let command = i.pop().unwrap_or(Object::NULL);
    if let Value::Name(name) = name.value {
        i.record_error(name, command)?;
    }
    Err(Fault::Stop)
}
