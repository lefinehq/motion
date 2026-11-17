//! The operand stack.

use crate::interp::{Interp, PsError, R};
use crate::object::{Object, Value};

pub(crate) fn pop(i: &mut Interp) -> R {
    i.pop().map(drop)
}

pub(crate) fn exch(i: &mut Interp) -> R {
    i.need(2)?;
    let len = i.ostack.len();
    i.ostack.swap(len - 1, len - 2);
    Ok(())
}

pub(crate) fn dup(i: &mut Interp) -> R {
    let top = i.peek(0)?;
    i.push(top)
}

/// `n copy`, the stack form of `copy`.
pub(crate) fn copy_n(i: &mut Interp, n: i32) -> R {
    let n = usize::try_from(n).map_err(|_| PsError::RangeCheck)?;
    i.need(n + 1)?;
    i.drop_n(1);
    if i.ostack.len() + n > i.limits.operands {
        return Err(PsError::StackOverflow.into());
    }
    let start = i.ostack.len() - n;
    i.ostack.extend_from_within(start..);
    Ok(())
}

pub(crate) fn index(i: &mut Interp) -> R {
    let n = i.int(0)?;
    let n = usize::try_from(n).map_err(|_| PsError::RangeCheck)?;
    let object = i.peek(n + 1).map_err(|_| PsError::RangeCheck)?;
    i.drop_n(1);
    i.push(object)
}

pub(crate) fn roll(i: &mut Interp) -> R {
    let (n, j) = (i.int(1)?, i.int(0)?);
    let n = usize::try_from(n).map_err(|_| PsError::RangeCheck)?;
    i.need(n + 2)?;
    i.drop_n(2);
    if n == 0 {
        return Ok(());
    }
    let len = i.ostack.len();
    let n_i64 = i64::try_from(n).map_err(|_| PsError::RangeCheck)?;
    let shift = usize::try_from(i64::from(j).rem_euclid(n_i64)).unwrap_or(0);
    i.ostack[len - n..].rotate_right(shift);
    Ok(())
}

pub(crate) fn clear(i: &mut Interp) -> R {
    i.ostack.clear();
    Ok(())
}

pub(crate) fn count(i: &mut Interp) -> R {
    let n = i32::try_from(i.ostack.len()).map_err(|_| PsError::RangeCheck)?;
    i.push(Object::int(n))
}

pub(crate) fn mark(i: &mut Interp) -> R {
    i.push(Object::MARK)
}

/// The depth of the topmost mark.
pub(crate) fn mark_depth(i: &Interp) -> R<usize> {
    i.ostack
        .iter()
        .rev()
        .position(|o| o.value == Value::Mark)
        .ok_or_else(|| PsError::UnmatchedMark.into())
}

pub(crate) fn cleartomark(i: &mut Interp) -> R {
    let depth = mark_depth(i)?;
    i.drop_n(depth + 1);
    Ok(())
}

pub(crate) fn counttomark(i: &mut Interp) -> R {
    let depth = mark_depth(i)?;
    let n = i32::try_from(depth).map_err(|_| PsError::RangeCheck)?;
    i.push(Object::int(n))
}
