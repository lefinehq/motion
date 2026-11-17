//! Arrays, strings, dictionaries, types and conversions, and `bind`.

use std::collections::HashSet;

use crate::interp::{Interp, PsError, R};
use crate::object::{Arr, Key, Object, Str, Value};
use crate::ops::stack;
use crate::scan;

/// A length or index operand.
fn size(i: &Interp, depth: usize) -> R<usize> {
    usize::try_from(i.int(depth)?).map_err(|_| PsError::RangeCheck.into())
}

/// A `u32` from a length the VM handed out.
fn u32_of(n: usize) -> R<u32> {
    u32::try_from(n).map_err(|_| PsError::LimitCheck.into())
}

pub(crate) fn array(i: &mut Interp) -> R {
    let n = size(i, 0)?;
    if n > 65_535 * 16 {
        return Err(PsError::LimitCheck.into());
    }
    let arr = i.vm.new_array(vec![Object::NULL; n])?;
    i.drop_n(1);
    i.push(Object::lit(Value::Array(arr)))
}

pub(crate) fn array_close(i: &mut Interp) -> R {
    let n = stack::mark_depth(i)?;
    let start = i.ostack.len() - n;
    let objects = i.ostack[start..].to_vec();
    let arr = i.vm.new_array(objects)?;
    i.drop_n(n + 1);
    i.push(Object::lit(Value::Array(arr)))
}

pub(crate) fn packedarray(i: &mut Interp) -> R {
    let n = size(i, 0)?;
    i.need(n + 1)?;
    let start = i.ostack.len() - 1 - n;
    let objects = i.ostack[start..i.ostack.len() - 1].to_vec();
    let arr = i.vm.new_array(objects)?;
    i.drop_n(n + 1);
    i.push(Object::lit(Value::Packed(arr)))
}

pub(crate) fn setpacking(i: &mut Interp) -> R {
    i.packing = i.boolean(0)?;
    i.drop_n(1);
    Ok(())
}

pub(crate) fn currentpacking(i: &mut Interp) -> R {
    let packing = i.packing;
    i.push(Object::bool(packing))
}

pub(crate) fn aload(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let arr = object.as_arr().ok_or(PsError::TypeCheck)?;
    let items = i.vm.arr(arr).to_vec();
    i.drop_n(1);
    if i.ostack.len() + items.len() + 1 > i.limits.operands {
        return Err(PsError::StackOverflow.into());
    }
    i.ostack.extend(items);
    i.push(object)
}

pub(crate) fn astore(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let arr = object.as_arr().ok_or(PsError::TypeCheck)?;
    let n = arr.len as usize;
    i.need(n + 1)?;
    let start = i.ostack.len() - 1 - n;
    let items = i.ostack[start..i.ostack.len() - 1].to_vec();
    i.vm.arr_mut(arr).copy_from_slice(&items);
    i.drop_n(n + 1);
    i.push(object)
}

pub(crate) fn length(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let n = match object.value {
        Value::Array(arr) | Value::Packed(arr) => arr.len as usize,
        Value::String(s) => s.len as usize,
        Value::Dict(dict) => i.vm.dicts[dict as usize].map.len(),
        Value::Name(name) => i.vm.name_bytes(name).len(),
        _ => return Err(PsError::TypeCheck.into()),
    };
    i.drop_n(1);
    i.push(Object::int(
        i32::try_from(n).map_err(|_| PsError::LimitCheck)?,
    ))
}

pub(crate) fn get(i: &mut Interp) -> R {
    let (container, key) = (i.peek(1)?, i.peek(0)?);
    let value = match container.value {
        Value::Array(arr) | Value::Packed(arr) => {
            let index = size(i, 0)?;
            i.vm.arr_get(arr, index).ok_or(PsError::RangeCheck)?
        }
        Value::String(s) => {
            let index = size(i, 0)?;
            let byte = *i.vm.str_bytes(s).get(index).ok_or(PsError::RangeCheck)?;
            Object::int(i32::from(byte))
        }
        Value::Dict(dict) => {
            let key = i.vm.key(key);
            match i.vm.dict_get(dict, &key) {
                Some(value) => value,
                None => {
                    i.current = key_object_for_error(key);
                    return Err(PsError::Undefined.into());
                }
            }
        }
        _ => return Err(PsError::TypeCheck.into()),
    };
    i.drop_n(2);
    i.push(value)
}

/// The object an `undefined` error from a dictionary names.
fn key_object_for_error(key: Key) -> Object {
    crate::vm::Vm::key_object(key)
}

pub(crate) fn put(i: &mut Interp) -> R {
    let (container, key, value) = (i.peek(2)?, i.peek(1)?, i.peek(0)?);
    match container.value {
        Value::Array(arr) | Value::Packed(arr) => {
            let index = size(i, 1)?;
            let slot =
                i.vm.arr_mut(arr)
                    .get_mut(index)
                    .ok_or(PsError::RangeCheck)?;
            *slot = value;
        }
        Value::String(s) => {
            let index = size(i, 1)?;
            let byte = u8::try_from(i.int(0)? & 0xFF).unwrap_or(0);
            let slot =
                i.vm.str_bytes_mut(s)
                    .get_mut(index)
                    .ok_or(PsError::RangeCheck)?;
            *slot = byte;
        }
        Value::Dict(dict) => {
            let key = i.vm.key(key);
            i.vm.dict_put(dict, key, value)?;
        }
        _ => return Err(PsError::TypeCheck.into()),
    }
    i.drop_n(3);
    Ok(())
}

pub(crate) fn getinterval(i: &mut Interp) -> R {
    let container = i.peek(2)?;
    let (index, count) = (size(i, 1)?, size(i, 0)?);
    let value = match container.value {
        Value::Array(arr) | Value::Packed(arr) => {
            if index + count > arr.len as usize {
                return Err(PsError::RangeCheck.into());
            }
            let sub = Arr {
                id: arr.id,
                start: arr.start + u32_of(index)?,
                len: u32_of(count)?,
            };
            match container.value {
                Value::Packed(_) => Value::Packed(sub),
                _ => Value::Array(sub),
            }
        }
        Value::String(s) => {
            if index + count > s.len as usize {
                return Err(PsError::RangeCheck.into());
            }
            Value::String(Str {
                id: s.id,
                start: s.start + u32_of(index)?,
                len: u32_of(count)?,
            })
        }
        _ => return Err(PsError::TypeCheck.into()),
    };
    i.drop_n(3);
    i.push(Object {
        value,
        exec: container.exec,
    })
}

pub(crate) fn putinterval(i: &mut Interp) -> R {
    let (target, source) = (i.peek(2)?, i.peek(0)?);
    let index = size(i, 1)?;
    match (target.value, source.value) {
        (Value::Array(t), Value::Array(s) | Value::Packed(s)) => {
            let items = i.vm.arr(s).to_vec();
            let slots = i.vm.arr_mut(t);
            if index + items.len() > slots.len() {
                return Err(PsError::RangeCheck.into());
            }
            slots[index..index + items.len()].copy_from_slice(&items);
        }
        (Value::String(t), Value::String(s)) => {
            let bytes = i.vm.str_bytes(s).to_vec();
            let slots = i.vm.str_bytes_mut(t);
            if index + bytes.len() > slots.len() {
                return Err(PsError::RangeCheck.into());
            }
            slots[index..index + bytes.len()].copy_from_slice(&bytes);
        }
        _ => return Err(PsError::TypeCheck.into()),
    }
    i.drop_n(3);
    Ok(())
}

/// `copy` in all its forms: the stack, arrays, strings and dictionaries.
pub(crate) fn copy(i: &mut Interp) -> R {
    let (source, target) = (i.peek(1).ok(), i.peek(0)?);
    if let Value::Int(n) = target.value {
        return stack::copy_n(i, n);
    }
    let source = source.ok_or(PsError::StackUnderflow)?;
    let result = match (source.value, target.value) {
        (Value::Array(s) | Value::Packed(s), Value::Array(t)) => {
            let items = i.vm.arr(s).to_vec();
            if items.len() > t.len as usize {
                return Err(PsError::RangeCheck.into());
            }
            i.vm.arr_mut(t)[..items.len()].copy_from_slice(&items);
            Object {
                value: Value::Array(Arr {
                    len: u32_of(items.len())?,
                    ..t
                }),
                exec: target.exec,
            }
        }
        (Value::String(s), Value::String(t)) => {
            let bytes = i.vm.str_bytes(s).to_vec();
            if bytes.len() > t.len as usize {
                return Err(PsError::RangeCheck.into());
            }
            i.vm.str_bytes_mut(t)[..bytes.len()].copy_from_slice(&bytes);
            Object {
                value: Value::String(Str {
                    len: u32_of(bytes.len())?,
                    ..t
                }),
                exec: target.exec,
            }
        }
        (Value::Dict(s), Value::Dict(t)) => {
            let entries: Vec<(Key, Object)> = i.vm.dicts[s as usize]
                .map
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect();
            for (key, value) in entries {
                i.vm.dict_put(t, key, value)?;
            }
            target
        }
        (Value::GState(s), Value::GState(t)) => {
            let state = i.gstates[s as usize].clone();
            i.gstates[t as usize] = state;
            target
        }
        _ => return Err(PsError::TypeCheck.into()),
    };
    i.drop_n(2);
    i.push(result)
}

pub(crate) fn string(i: &mut Interp) -> R {
    let n = size(i, 0)?;
    if n > 65_535 * 64 {
        return Err(PsError::LimitCheck.into());
    }
    let s = i.vm.new_string(vec![0; n])?;
    i.drop_n(1);
    i.push(Object::lit(Value::String(s)))
}

/// The part of `s` from `start`, `len` bytes long.
fn sub(s: Str, start: usize, len: usize) -> Str {
    Str {
        id: s.id,
        start: s.start + u32::try_from(start).unwrap_or(0),
        len: u32::try_from(len).unwrap_or(0),
    }
}

pub(crate) fn search(i: &mut Interp) -> R {
    let (text, seek) = (i.string(1)?, i.string(0)?);
    let hay = i.vm.str_bytes(text);
    let needle = i.vm.str_bytes(seek);
    let found = if needle.is_empty() {
        Some(0)
    } else {
        hay.windows(needle.len()).position(|w| w == needle)
    };
    let (n, h) = (needle.len(), hay.len());
    i.tick(u64::try_from(h / 16).unwrap_or(0))?;
    i.drop_n(2);
    match found {
        Some(at) => {
            i.push(Object::lit(Value::String(sub(text, at + n, h - at - n))))?;
            i.push(Object::lit(Value::String(sub(text, at, n))))?;
            i.push(Object::lit(Value::String(sub(text, 0, at))))?;
            i.push(Object::bool(true))
        }
        None => {
            i.push(Object::lit(Value::String(text)))?;
            i.push(Object::bool(false))
        }
    }
}

pub(crate) fn anchorsearch(i: &mut Interp) -> R {
    let (text, seek) = (i.string(1)?, i.string(0)?);
    let hit = i.vm.str_bytes(text).starts_with(i.vm.str_bytes(seek));
    let n = seek.len as usize;
    i.drop_n(2);
    if hit {
        i.push(Object::lit(Value::String(sub(
            text,
            n,
            text.len as usize - n,
        ))))?;
        i.push(Object::lit(Value::String(sub(text, 0, n))))?;
        i.push(Object::bool(true))
    } else {
        i.push(Object::lit(Value::String(text)))?;
        i.push(Object::bool(false))
    }
}

pub(crate) fn dict(i: &mut Interp) -> R {
    let n = size(i, 0)?;
    let dict = i.vm.new_dict(n)?;
    i.drop_n(1);
    i.push(Object::lit(Value::Dict(dict)))
}

pub(crate) fn dict_close(i: &mut Interp) -> R {
    let n = stack::mark_depth(i)?;
    if n % 2 != 0 {
        return Err(PsError::RangeCheck.into());
    }
    let start = i.ostack.len() - n;
    let pairs = i.ostack[start..].to_vec();
    let dict = i.vm.new_dict(n / 2)?;
    for pair in pairs.chunks(2) {
        let key = i.vm.key(pair[0]);
        i.vm.dict_put(dict, key, pair[1])?;
    }
    i.drop_n(n + 1);
    i.push(Object::lit(Value::Dict(dict)))
}

pub(crate) fn maxlength(i: &mut Interp) -> R {
    let dict = i.dict(0)?;
    let body = &i.vm.dicts[dict as usize];
    let n = body.max.max(body.map.len());
    i.drop_n(1);
    i.push(Object::int(i32::try_from(n).unwrap_or(i32::MAX)))
}

pub(crate) fn begin(i: &mut Interp) -> R {
    let dict = i.dict(0)?;
    if i.dstack.len() >= i.limits.dictionaries {
        return Err(PsError::DictStackOverflow.into());
    }
    i.drop_n(1);
    i.dstack.push(dict);
    Ok(())
}

pub(crate) fn end(i: &mut Interp) -> R {
    // The three permanent dictionaries stay.
    if i.dstack.len() <= 3 {
        return Err(PsError::DictStackUnderflow.into());
    }
    i.dstack.pop();
    Ok(())
}

pub(crate) fn def(i: &mut Interp) -> R {
    let (key, value) = (i.peek(1)?, i.peek(0)?);
    let key = i.vm.key(key);
    let dict = i.current_dict();
    i.vm.dict_put(dict, key, value)?;
    i.drop_n(2);
    Ok(())
}

pub(crate) fn load(i: &mut Interp) -> R {
    let key = i.peek(0)?;
    let key = i.vm.key(key);
    let value = i.lookup_key(&key).ok_or(PsError::Undefined)?;
    i.drop_n(1);
    i.push(value)
}

pub(crate) fn store(i: &mut Interp) -> R {
    let (key, value) = (i.peek(1)?, i.peek(0)?);
    let key = i.vm.key(key);
    let dict = i
        .dstack
        .iter()
        .rev()
        .copied()
        .find(|&d| i.vm.dict_get(d, &key).is_some())
        .unwrap_or_else(|| i.current_dict());
    i.vm.dict_put(dict, key, value)?;
    i.drop_n(2);
    Ok(())
}

pub(crate) fn known(i: &mut Interp) -> R {
    let (dict, key) = (i.dict(1)?, i.peek(0)?);
    let key = i.vm.key(key);
    let known = i.vm.dict_get(dict, &key).is_some();
    i.drop_n(2);
    i.push(Object::bool(known))
}

pub(crate) fn undef(i: &mut Interp) -> R {
    let (dict, key) = (i.dict(1)?, i.peek(0)?);
    let key = i.vm.key(key);
    i.vm.dicts[dict as usize].map.shift_remove(&key);
    i.drop_n(2);
    Ok(())
}

pub(crate) fn where_(i: &mut Interp) -> R {
    let key = i.peek(0)?;
    let key = i.vm.key(key);
    let found = i
        .dstack
        .iter()
        .rev()
        .copied()
        .find(|&d| i.vm.dict_get(d, &key).is_some());
    i.drop_n(1);
    match found {
        Some(dict) => {
            i.push(Object::lit(Value::Dict(dict)))?;
            i.push(Object::bool(true))
        }
        None => i.push(Object::bool(false)),
    }
}

pub(crate) fn currentdict(i: &mut Interp) -> R {
    let dict = i.current_dict();
    i.push(Object::lit(Value::Dict(dict)))
}

pub(crate) fn countdictstack(i: &mut Interp) -> R {
    let n = i32::try_from(i.dstack.len()).unwrap_or(i32::MAX);
    i.push(Object::int(n))
}

pub(crate) fn dictstack(i: &mut Interp) -> R {
    let arr = i.array(0)?;
    let dicts: Vec<Object> = i
        .dstack
        .iter()
        .map(|&d| Object::lit(Value::Dict(d)))
        .collect();
    if dicts.len() > arr.len as usize {
        return Err(PsError::RangeCheck.into());
    }
    i.vm.arr_mut(arr)[..dicts.len()].copy_from_slice(&dicts);
    i.drop_n(1);
    let len = u32_of(dicts.len())?;
    i.push(Object::lit(Value::Array(Arr { len, ..arr })))
}

pub(crate) fn cleardictstack(i: &mut Interp) -> R {
    i.dstack.truncate(3);
    Ok(())
}

pub(crate) fn readonly(i: &mut Interp) -> R {
    if let Value::Dict(dict) = i.peek(0)?.value {
        i.vm.dicts[dict as usize].readonly = true;
    }
    Ok(())
}

/// `executeonly` and `noaccess`: recorded nowhere, since nothing here
/// reads what a program protects.
pub(crate) fn executeonly(i: &mut Interp) -> R {
    i.peek(0).map(drop)
}

pub(crate) fn noaccess(i: &mut Interp) -> R {
    i.peek(0).map(drop)
}

pub(crate) fn rcheck(i: &mut Interp) -> R {
    i.peek(0)?;
    i.drop_n(1);
    i.push(Object::bool(true))
}

pub(crate) fn wcheck(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let writable = match object.value {
        Value::Dict(dict) => !i.vm.dicts[dict as usize].readonly,
        Value::Packed(_) => false,
        _ => true,
    };
    i.drop_n(1);
    i.push(Object::bool(writable))
}

pub(crate) fn xcheck(i: &mut Interp) -> R {
    let exec = i.peek(0)?.exec;
    i.drop_n(1);
    i.push(Object::bool(exec))
}

pub(crate) fn type_(i: &mut Interp) -> R {
    let name = i.peek(0)?.type_name();
    let name = i.name_obj(name).cvx();
    i.drop_n(1);
    i.push(name)
}

pub(crate) fn cvlit(i: &mut Interp) -> R {
    let object = i.pop()?;
    i.push(object.cvlit())
}

pub(crate) fn cvx(i: &mut Interp) -> R {
    let object = i.pop()?;
    i.push(object.cvx())
}

/// The number a string spells, for `cvi` and `cvr`.
fn string_number(i: &Interp, s: Str) -> R<f64> {
    let text = i.vm.str_bytes(s);
    let word: Vec<u8> = text
        .iter()
        .copied()
        .skip_while(|b| scan::is_space(*b))
        .take_while(|b| !scan::is_space(*b))
        .collect();
    scan::number(&word)
        .and_then(Object::as_f64)
        .ok_or_else(|| PsError::SyntaxError.into())
}

pub(crate) fn cvi(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let value = match object.value {
        Value::Int(_) => return Ok(()),
        Value::Real(real) => real,
        Value::String(s) => string_number(i, s)?,
        _ => return Err(PsError::TypeCheck.into()),
    };
    let value = value.trunc();
    if value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(PsError::RangeCheck.into());
    }
    i.drop_n(1);
    i.push(Object::number(value))
}

pub(crate) fn cvr(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let value = match object.value {
        Value::Int(int) => f64::from(int),
        Value::Real(_) => return Ok(()),
        Value::String(s) => string_number(i, s)?,
        _ => return Err(PsError::TypeCheck.into()),
    };
    i.drop_n(1);
    i.push(Object::real(value))
}

pub(crate) fn cvn(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let Value::String(s) = object.value else {
        return Err(PsError::TypeCheck.into());
    };
    let bytes = i.vm.str_bytes(s).to_vec();
    let name = i.vm.name(&bytes);
    i.drop_n(1);
    i.push(Object {
        value: Value::Name(name),
        exec: object.exec,
    })
}

/// A real as `cvs` writes it: the shortest form that reads back, with a
/// point so it stays a real.
pub(crate) fn format_real(value: f64) -> String {
    // The language's reals are single precision; print what they hold.
    #[allow(clippy::cast_possible_truncation)]
    let single = value as f32;
    let mut text = format!("{single}");
    if !text.contains(['.', 'e', 'E', 'i', 'N']) {
        text.push_str(".0");
    }
    text
}

/// The text `cvs` gives an object.
pub(crate) fn text_of(i: &Interp, object: Object) -> Vec<u8> {
    match object.value {
        Value::Int(int) => int.to_string().into_bytes(),
        Value::Real(real) => format_real(real).into_bytes(),
        Value::Bool(b) => b.to_string().into_bytes(),
        Value::String(s) => i.vm.str_bytes(s).to_vec(),
        Value::Name(name) => i.vm.name_bytes(name).to_vec(),
        Value::Operator(op) => crate::ops::OPERATORS
            .get(op as usize)
            .map_or_else(Vec::new, |def| def.name.as_bytes().to_vec()),
        _ => b"--nostringval--".to_vec(),
    }
}

/// Writes `bytes` into the string on top and replaces both operands with
/// the part written.
fn write_into(i: &mut Interp, bytes: &[u8]) -> R {
    let target = i.string(0)?;
    if bytes.len() > target.len as usize {
        return Err(PsError::RangeCheck.into());
    }
    i.vm.str_bytes_mut(target)[..bytes.len()].copy_from_slice(bytes);
    i.drop_n(2);
    i.push(Object::lit(Value::String(sub(target, 0, bytes.len()))))
}

pub(crate) fn cvs(i: &mut Interp) -> R {
    let object = i.peek(1)?;
    let bytes = text_of(i, object);
    write_into(i, &bytes)
}

pub(crate) fn cvrs(i: &mut Interp) -> R {
    let (number, radix) = (i.peek(2)?, i.int(1)?);
    if !(2..=36).contains(&radix) {
        return Err(PsError::RangeCheck.into());
    }
    let bytes = if radix == 10 {
        text_of(i, number)
    } else {
        let value = match number.value {
            Value::Int(int) => int,
            #[allow(clippy::cast_possible_truncation)]
            Value::Real(real) => real as i32,
            _ => return Err(PsError::TypeCheck.into()),
        };
        // Other radices write the 32 bits as unsigned.
        #[allow(clippy::cast_sign_loss)]
        let mut value = value as u32;
        let radix = radix.unsigned_abs();
        let mut digits = Vec::new();
        loop {
            let digit = char::from_digit(value % radix, radix).unwrap_or('0');
            digits.push(digit.to_ascii_uppercase() as u8);
            value /= radix;
            if value == 0 {
                break;
            }
        }
        digits.reverse();
        digits
    };
    let target = i.string(0)?;
    if bytes.len() > target.len as usize {
        return Err(PsError::RangeCheck.into());
    }
    i.vm.str_bytes_mut(target)[..bytes.len()].copy_from_slice(&bytes);
    i.drop_n(3);
    i.push(Object::lit(Value::String(sub(target, 0, bytes.len()))))
}

/// `bind`: replaces each executable name in a procedure, and in the
/// procedures inside it, whose value is an operator with the operator.
/// It walks with a work list rather than recursion, so a procedure nested
/// any depth cannot exhaust the stack.
pub(crate) fn bind(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let Some(first) = object.as_arr() else {
        return Ok(());
    };
    let mut seen = HashSet::new();
    let mut work = vec![first];
    while let Some(arr) = work.pop() {
        if !seen.insert((arr.id, arr.start)) {
            continue;
        }
        i.tick(u64::from(arr.len))?;
        for index in 0..arr.len as usize {
            let Some(element) = i.vm.arr_get(arr, index) else {
                break;
            };
            match element.value {
                Value::Name(name) if element.exec => {
                    if let Some(value) = i.lookup(name)
                        && matches!(value.value, Value::Operator(_))
                    {
                        i.vm.arr_mut(arr)[index] = value;
                    }
                }
                Value::Array(inner) | Value::Packed(inner) if element.exec => work.push(inner),
                _ => {}
            }
        }
    }
    Ok(())
}

pub(crate) fn null(i: &mut Interp) -> R {
    i.push(Object::NULL)
}

pub(crate) fn true_(i: &mut Interp) -> R {
    i.push(Object::bool(true))
}

pub(crate) fn false_(i: &mut Interp) -> R {
    i.push(Object::bool(false))
}

pub(crate) fn setglobal(i: &mut Interp) -> R {
    i.global = i.boolean(0)?;
    i.drop_n(1);
    Ok(())
}

pub(crate) fn currentglobal(i: &mut Interp) -> R {
    let global = i.global;
    i.push(Object::bool(global))
}

pub(crate) fn gcheck(i: &mut Interp) -> R {
    i.peek(0)?;
    i.drop_n(1);
    i.push(Object::bool(false))
}
