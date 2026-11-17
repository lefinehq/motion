//! The device, memory and job queries a program makes before it draws,
//! answered the same way on every machine: no answer depends on the clock
//! or the host, so a file draws the same every time.

use crate::interp::{Frame, Interp, PsError, R};
use crate::object::{Key, Object, Value};

/// Pops `n` operands, for settings that change nothing drawn.
fn pop_n(i: &mut Interp, n: usize) -> R {
    i.need(n)?;
    i.drop_n(n);
    Ok(())
}

pub(crate) fn pop2(i: &mut Interp) -> R {
    pop_n(i, 2)
}

pub(crate) fn pop3(i: &mut Interp) -> R {
    pop_n(i, 3)
}

pub(crate) fn pop4(i: &mut Interp) -> R {
    pop_n(i, 4)
}

pub(crate) fn pop12(i: &mut Interp) -> R {
    pop_n(i, 12)
}

pub(crate) fn zero(i: &mut Interp) -> R {
    i.push(Object::int(0))
}

/// An empty procedure.
fn empty_proc(i: &mut Interp) -> R<Object> {
    let arr = i.vm.new_array(Vec::new())?;
    Ok(Object::exe(Value::Array(arr)))
}

pub(crate) fn identity_proc(i: &mut Interp) -> R {
    let proc = empty_proc(i)?;
    i.push(proc)
}

pub(crate) fn currentcolortransfer(i: &mut Interp) -> R {
    for _ in 0..4 {
        identity_proc(i)?;
    }
    Ok(())
}

pub(crate) fn empty_dict(i: &mut Interp) -> R {
    let dict = i.vm.new_dict(8)?;
    i.push(Object::lit(Value::Dict(dict)))
}

/// A new dictionary holding `entries`.
fn dict_of(i: &mut Interp, entries: &[(&str, Object)]) -> R<Object> {
    let dict = i.vm.new_dict(entries.len())?;
    for (name, value) in entries {
        i.vm.dict_put_name(dict, name, *value)?;
    }
    Ok(Object::lit(Value::Dict(dict)))
}

pub(crate) fn currenthalftone(i: &mut Interp) -> R {
    let spot = empty_proc(i)?;
    let dict = dict_of(
        i,
        &[
            ("HalftoneType", Object::int(1)),
            ("Frequency", Object::real(60.0)),
            ("Angle", Object::real(45.0)),
            ("SpotFunction", spot),
        ],
    )?;
    i.push(dict)
}

pub(crate) fn currentscreen(i: &mut Interp) -> R {
    i.push(Object::real(60.0))?;
    i.push(Object::real(45.0))?;
    identity_proc(i)
}

pub(crate) fn currentcolorscreen(i: &mut Interp) -> R {
    for _ in 0..4 {
        currentscreen(i)?;
    }
    Ok(())
}

pub(crate) fn cachestatus(i: &mut Interp) -> R {
    for v in [0, 100_000, 0, 1000, 0, 10_000, 1000] {
        i.push(Object::int(v))?;
    }
    Ok(())
}

pub(crate) fn ucachestatus(i: &mut Interp) -> R {
    i.push(Object::MARK)?;
    for v in [0, 100_000, 0, 1000, 10_000] {
        i.push(Object::int(v))?;
    }
    Ok(())
}

/// A dictionary describing the page: its size is the bounding box's, at
/// one device unit per point.
pub(crate) fn currentpagedevice(i: &mut Interp) -> R {
    let b = i.out.bounding_box;
    let size =
        i.vm.new_array(vec![Object::real(b.width()), Object::real(b.height())])?;
    let resolution =
        i.vm.new_array(vec![Object::real(72.0), Object::real(72.0)])?;
    let margins = i.vm.new_array(vec![Object::int(0), Object::int(0)])?;
    let begin = empty_proc(i)?;
    let policies = dict_of(i, &[])?;
    let dict = dict_of(
        i,
        &[
            ("PageSize", Object::lit(Value::Array(size))),
            ("HWResolution", Object::lit(Value::Array(resolution))),
            ("Margins", Object::lit(Value::Array(margins))),
            ("ImagingBBox", Object::NULL),
            ("Orientation", Object::int(0)),
            ("BeginPage", begin),
            ("Policies", policies),
            ("NumCopies", Object::NULL),
        ],
    )?;
    i.push(dict)
}

pub(crate) fn languagelevel(i: &mut Interp) -> R {
    i.push(Object::int(2))
}

/// A new string object.
fn text(i: &mut Interp, text: &str) -> R {
    let s = i.vm.new_string(text.as_bytes().to_vec())?;
    i.push(Object::lit(Value::String(s)))
}

pub(crate) fn version(i: &mut Interp) -> R {
    text(i, "3010")
}

pub(crate) fn product(i: &mut Interp) -> R {
    text(i, "motion")
}

pub(crate) fn revision(i: &mut Interp) -> R {
    i.push(Object::int(1))
}

pub(crate) fn serialnumber(i: &mut Interp) -> R {
    i.push(Object::int(0))
}

/// `realtime` and `usertime`: a count of operations, not the clock, so
/// that timing a loop gives the same answer on every run.
pub(crate) fn realtime(i: &mut Interp) -> R {
    let ms = i32::try_from(i.steps / 1000).unwrap_or(i32::MAX);
    i.push(Object::int(ms))
}

pub(crate) fn vmstatus(i: &mut Interp) -> R {
    let level = i32::try_from(i.save_level).unwrap_or(i32::MAX);
    i.push(Object::int(level))?;
    i.push(Object::int(1_000_000))?;
    i.push(Object::int(64_000_000))
}

pub(crate) fn currentdevparams(i: &mut Interp) -> R {
    i.string(0)?;
    i.drop_n(1);
    empty_dict(i)
}

/// `internaldict`: one private dictionary, the same each time it is asked
/// for.
pub(crate) fn internaldict(i: &mut Interp) -> R {
    i.int(0)?;
    i.drop_n(1);
    let key = Key::Name(i.vm.name(b".internaldict"));
    let dict = match i.vm.dict_get(i.systemdict, &key) {
        Some(dict) => dict,
        None => {
            let dict = Object::lit(Value::Dict(i.vm.new_dict(16)?));
            i.vm.dict_put(i.systemdict, key, dict)?;
            dict
        }
    };
    i.push(dict)
}

pub(crate) fn execuserobject(i: &mut Interp) -> R {
    let index = usize::try_from(i.int(0)?).map_err(|_| PsError::RangeCheck)?;
    let objects = i.lookup_str("UserObjects").and_then(Object::as_arr);
    let object = objects
        .and_then(|arr| i.vm.arr_get(arr, index))
        .ok_or(PsError::Undefined)?;
    i.drop_n(1);
    i.push_frame(Frame::Exec(object))
}

/// The system parameters a program may read: cache sizes, as a device
/// with ordinary memory reports them.
pub(crate) fn currentsystemparams(i: &mut Interp) -> R {
    let mut entries = Vec::new();
    for (name, value) in [
        ("MaxFontCache", 4_000_000),
        ("CurFontCache", 0),
        ("MaxFormCache", 1_000_000),
        ("CurFormCache", 0),
        ("MaxPatternCache", 1_000_000),
        ("CurPatternCache", 0),
        ("MaxScreenStorage", 1_000_000),
        ("CurScreenStorage", 0),
        ("MaxUPathCache", 300_000),
        ("CurUPathCache", 0),
        ("MaxDisplayList", 4_000_000),
        ("CurDisplayList", 0),
        ("MaxOutlineCache", 1_000_000),
        ("CurOutlineCache", 0),
        ("BuildTime", 0),
        ("RealFormat", 0),
    ] {
        entries.push((name, Object::int(value)));
    }
    entries.push(("ByteOrder", Object::bool(false)));
    let dict = dict_of(i, &entries)?;
    i.push(dict)
}

/// The user parameters a program may read, matching its stack limits.
pub(crate) fn currentuserparams(i: &mut Interp) -> R {
    let operands = i32::try_from(i.limits.operands).unwrap_or(i32::MAX);
    let dictionaries = i32::try_from(i.limits.dictionaries).unwrap_or(i32::MAX);
    let depth = i32::try_from(i.limits.depth).unwrap_or(i32::MAX);
    let mut entries = Vec::new();
    for (name, value) in [
        ("MaxFontItem", 12_000),
        ("MinFontCompress", 100),
        ("MaxUPathItem", 5000),
        ("MaxFormItem", 100_000),
        ("MaxPatternItem", 20_000),
        ("MaxScreenItem", 48_000),
        ("MaxOpStack", operands),
        ("MaxDictStack", dictionaries),
        ("MaxExecStack", depth),
        ("MaxLocalVM", 64_000_000),
        ("VMReclaim", 0),
        ("VMThreshold", 1_000_000),
    ] {
        entries.push((name, Object::int(value)));
    }
    entries.push(("AccurateScreens", Object::bool(false)));
    let dict = dict_of(i, &entries)?;
    i.push(dict)
}
