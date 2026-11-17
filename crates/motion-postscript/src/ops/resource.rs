//! Named resources, as the language's second level defines them.
//!
//! Every category has an implementation dictionary, itself an instance of
//! the `Category` category, holding the procedures `FindResource`,
//! `DefineResource`, `UndefineResource`, `ResourceStatus` and
//! `ResourceForAll`. The resource operators run the category's procedure
//! with its dictionary on the dictionary stack, so a program may define
//! categories of its own, usually by copying `Generic`'s dictionary. The
//! built-in procedures keep each category's instances by the category's
//! name, which `defineresource` writes into a new category's dictionary.

use crate::interp::{Frame, Interp, PsError, R};
use crate::object::{Key, Object, Str, Value};
use crate::ops::font;

/// The categories a run starts with, and the instances each holds.
const CATEGORIES: &[(&str, &[&str])] = &[
    ("Category", &[]),
    ("Generic", &[]),
    ("ProcSet", &[]),
    ("Font", &[]),
    ("CIDFont", &[]),
    ("CMap", &[]),
    ("Encoding", &["StandardEncoding", "ISOLatin1Encoding"]),
    ("Form", &[]),
    ("Pattern", &[]),
    ("ColorSpace", &[]),
    ("Halftone", &[]),
    ("ColorRendering", &[]),
    ("IdiomSet", &[]),
    ("InkParams", &[]),
    ("TrapParams", &[]),
    ("OutputDevice", &[]),
    ("ControlLanguage", &[]),
    ("Localization", &[]),
    ("PDL", &[]),
    ("HWOptions", &[]),
    (
        "Filter",
        &[
            "ASCIIHexDecode",
            "ASCII85Decode",
            "RunLengthDecode",
            "FlateDecode",
            "SubFileDecode",
            "NullEncode",
        ],
    ),
    (
        "ColorSpaceFamily",
        &[
            "DeviceGray",
            "DeviceRGB",
            "DeviceCMYK",
            "Indexed",
            "Separation",
            "DeviceN",
            "Pattern",
            "CIEBasedA",
            "CIEBasedABC",
            "CIEBasedDEF",
            "CIEBasedDEFG",
        ],
    ),
    ("Emulator", &[]),
    ("IODevice", &[]),
    ("ColorRenderingType", &["1"]),
    ("FMapType", &[]),
    ("FontType", &["0", "1", "3", "42"]),
    ("FormType", &["1"]),
    ("HalftoneType", &["1"]),
    ("ImageType", &["1"]),
    ("PatternType", &["1"]),
    ("FunctionType", &[]),
    ("ShadingType", &[]),
];

/// The procedures of an implementation dictionary and the operators that
/// implement them for the built-in categories.
const PROCEDURES: [(&str, &str); 5] = [
    ("FindResource", ".findresource"),
    ("DefineResource", ".defineresource"),
    ("UndefineResource", ".undefineresource"),
    ("ResourceStatus", ".resourcestatus"),
    ("ResourceForAll", ".resourceforall"),
];

/// Creates the categories: their instances and their implementation
/// dictionaries.
pub(crate) fn install(i: &mut Interp) -> R {
    let categories = i.vm.new_dict(CATEGORIES.len())?;
    for (category, instances) in CATEGORIES {
        let store = i.vm.new_dict(instances.len().max(8))?;
        for instance in *instances {
            let (key, value) = match instance.parse::<i32>() {
                Ok(n) => (Key::Int(i64::from(n)), Object::int(n)),
                Err(_) => {
                    let key = Key::Name(i.vm.name(instance.as_bytes()));
                    let value = match i.lookup_str(instance) {
                        Some(value) if !matches!(value.value, Value::Operator(_)) => value,
                        _ => i.name_obj(instance),
                    };
                    (key, value)
                }
            };
            i.vm.dict_put(store, key, value)?;
        }
        i.vm.dict_put_name(i.resources, category, Object::lit(Value::Dict(store)))?;
        let implementation = implementation(i, category)?;
        i.vm.dict_put_name(categories, category, implementation)?;
    }
    // The `Category` category's instances are the implementation
    // dictionaries themselves.
    i.vm.dict_put_name(
        i.resources,
        "Category",
        Object::lit(Value::Dict(categories)),
    )?;
    Ok(())
}

/// A new implementation dictionary for category `name`.
fn implementation(i: &mut Interp, name: &str) -> R<Object> {
    let dict = i.vm.new_dict(12)?;
    for (entry, operator) in PROCEDURES {
        let op = i.lookup_str(operator).ok_or(PsError::Undefined)?;
        i.vm.dict_put_name(dict, entry, op)?;
    }
    let category = i.name_obj(name);
    i.vm.dict_put_name(dict, "Category", category)?;
    i.vm.dict_put_name(dict, "InstanceType", Object::NULL)?;
    Ok(Object::lit(Value::Dict(dict)))
}

/// The instances of `Category`: every implementation dictionary.
fn categories(i: &mut Interp) -> R<u32> {
    let key = Key::Name(i.vm.name(b"Category"));
    match i.vm.dict_get(i.resources, &key).map(|o| o.value) {
        Some(Value::Dict(dict)) => Ok(dict),
        _ => Err(PsError::UndefinedResource.into()),
    }
}

/// Whether `object` is the name `Category`.
fn is_category(i: &Interp, object: Object) -> bool {
    matches!(object.value, Value::Name(n) if i.vm.name_bytes(n) == b"Category")
}

/// Runs the procedure `entry` of the implementation dictionary of the
/// category on top, with that dictionary on the dictionary stack. The
/// operands it needs stay below the category. A built-in procedure runs
/// directly on its category's instances, so an error it raises leaves the
/// dictionary stack as it was.
fn dispatch(i: &mut Interp, entry: &str, native: fn(&mut Interp, u32) -> R) -> R {
    let category = i.peek(0)?;
    let categories = categories(i)?;
    if is_category(i, category) {
        // `Category` itself is handled here, on its own instances.
        i.drop_n(1);
        return native(i, categories);
    }
    let key = i.vm.key(category);
    let Some(Object {
        value: Value::Dict(implementation),
        ..
    }) = i.vm.dict_get(categories, &key)
    else {
        return Err(PsError::UndefinedResource.into());
    };
    let entry_key = Key::Name(i.vm.name(entry.as_bytes()));
    let procedure =
        i.vm.dict_get(implementation, &entry_key)
            .ok_or(PsError::Undefined)?;
    i.drop_n(1);
    if let Value::Operator(op) = procedure.value
        && crate::ops::OPERATORS
            .get(op as usize)
            .is_some_and(|def| def.name.starts_with('.'))
    {
        return match store_of(i, implementation)? {
            Some(store) => native(i, store),
            None => font_native(i, entry),
        };
    }
    if i.dstack.len() >= i.limits.dictionaries {
        return Err(PsError::DictStackOverflow.into());
    }
    let end = i.lookup_str("end").ok_or(PsError::Undefined)?;
    i.dstack.push(implementation);
    i.push_frame(Frame::Exec(end))?;
    i.push_frame(Frame::Exec(procedure))
}

/// The font category's built-in procedures, on `FontDirectory`.
fn font_native(i: &mut Interp, entry: &str) -> R {
    let directory = i.fontdirectory;
    match entry {
        "FindResource" => font::findfont(i),
        "DefineResource" => font::definefont(i),
        "UndefineResource" => undefine_in(i, directory),
        "ResourceStatus" => status_in(i, directory),
        _ => forall_in(i, directory),
    }
}

pub(crate) fn findresource(i: &mut Interp) -> R {
    i.need(2)?;
    dispatch(i, "FindResource", find_in)
}

pub(crate) fn defineresource(i: &mut Interp) -> R {
    i.need(3)?;
    dispatch(i, "DefineResource", define_in)
}

pub(crate) fn undefineresource(i: &mut Interp) -> R {
    i.need(2)?;
    dispatch(i, "UndefineResource", undefine_in)
}

pub(crate) fn resourcestatus(i: &mut Interp) -> R {
    i.need(2)?;
    dispatch(i, "ResourceStatus", status_in)
}

pub(crate) fn resourceforall(i: &mut Interp) -> R {
    i.need(4)?;
    dispatch(i, "ResourceForAll", forall_in)
}

pub(crate) fn findencoding(i: &mut Interp) -> R {
    let category = i.name_obj("Encoding");
    i.push(category)?;
    findresource(i)
}

/// The instances of the category whose implementation dictionary is
/// `implementation`, by the name it gives itself; `None` for fonts, which
/// live in `FontDirectory`.
fn store_of(i: &mut Interp, implementation: u32) -> R<Option<u32>> {
    let key = Key::Name(i.vm.name(b"Category"));
    let name =
        i.vm.dict_get(implementation, &key)
            .ok_or(PsError::Undefined)?;
    if matches!(name.value, Value::Name(n) if i.vm.name_bytes(n) == b"Font") {
        return Ok(None);
    }
    let name_key = i.vm.key(name);
    if let Some(Object {
        value: Value::Dict(store),
        ..
    }) = i.vm.dict_get(i.resources, &name_key)
    {
        return Ok(Some(store));
    }
    let store = i.vm.new_dict(8)?;
    i.vm.dict_put(i.resources, name_key, Object::lit(Value::Dict(store)))?;
    Ok(Some(store))
}

/// Runs a built-in procedure called by name, with its category's
/// implementation dictionary on top of the dictionary stack.
fn native_here(i: &mut Interp, entry: &str, run: fn(&mut Interp, u32) -> R) -> R {
    let implementation = i.current_dict();
    match store_of(i, implementation)? {
        Some(store) => run(i, store),
        None => font_native(i, entry),
    }
}

/// The built-in `FindResource`: `key FindResource instance`.
pub(crate) fn native_find(i: &mut Interp) -> R {
    native_here(i, "FindResource", find_in)
}

/// The built-in `DefineResource`: `key instance DefineResource instance`.
pub(crate) fn native_define(i: &mut Interp) -> R {
    native_here(i, "DefineResource", define_in)
}

/// The built-in `UndefineResource`: `key UndefineResource`.
pub(crate) fn native_undefine(i: &mut Interp) -> R {
    native_here(i, "UndefineResource", undefine_in)
}

/// The built-in `ResourceStatus`: `key ResourceStatus status size true`
/// or `false`.
pub(crate) fn native_status(i: &mut Interp) -> R {
    native_here(i, "ResourceStatus", status_in)
}

/// The built-in `ResourceForAll`: `template proc scratch ResourceForAll`.
pub(crate) fn native_forall(i: &mut Interp) -> R {
    native_here(i, "ResourceForAll", forall_in)
}

fn find_in(i: &mut Interp, store: u32) -> R {
    let key = i.peek(0)?;
    let key = i.vm.key(key);
    let instance =
        i.vm.dict_get(store, &key)
            .ok_or(PsError::UndefinedResource)?;
    i.drop_n(1);
    i.push(instance)
}

fn define_in(i: &mut Interp, store: u32) -> R {
    let (key, instance) = (i.peek(1)?, i.peek(0)?);
    let categories = categories(i)?;
    if store == categories {
        // A new category's dictionary learns its own name, so the built-in
        // procedures it copied keep its instances apart.
        let Value::Dict(implementation) = instance.value else {
            return Err(PsError::TypeCheck.into());
        };
        i.vm.dict_put_name(implementation, "Category", key)?;
    }
    let key = i.vm.key(key);
    i.vm.dict_put(store, key, instance)?;
    i.drop_n(2);
    i.push(instance)
}

fn undefine_in(i: &mut Interp, store: u32) -> R {
    let key = i.peek(0)?;
    let key = i.vm.key(key);
    i.vm.dicts[store as usize].map.shift_remove(&key);
    i.drop_n(1);
    Ok(())
}

fn status_in(i: &mut Interp, store: u32) -> R {
    let key = i.peek(0)?;
    let key = i.vm.key(key);
    let found = i.vm.dict_get(store, &key).is_some();
    i.drop_n(1);
    if found {
        i.push(Object::int(1))?;
        i.push(Object::int(-1))?;
    }
    i.push(Object::bool(found))
}

fn forall_in(i: &mut Interp, store: u32) -> R {
    let (template, proc, scratch) = (i.string(2)?, i.proc(1)?, i.string(0)?);
    let template = i.vm.str_bytes(template).to_vec();
    i.drop_n(3);
    let keys: Vec<Key> = i.vm.dicts[store as usize].map.keys().copied().collect();
    for key in keys {
        let Key::Name(name) = key else {
            continue;
        };
        let bytes = i.vm.name_bytes(name).to_vec();
        if !matches(&template, &bytes) {
            continue;
        }
        if bytes.len() > scratch.len as usize {
            return Err(PsError::RangeCheck.into());
        }
        i.vm.str_bytes_mut(scratch)[..bytes.len()].copy_from_slice(&bytes);
        let part = Str {
            len: u32::try_from(bytes.len()).unwrap_or(0),
            ..scratch
        };
        i.push(Object::lit(Value::String(part)))?;
        i.call(proc)?;
    }
    Ok(())
}

/// Whether `name` matches `template`, where `*` stands for any run of
/// characters and `?` for one. Greedy with one backtrack point, so the
/// time is linear in the two lengths times the stars, never exponential.
fn matches(template: &[u8], name: &[u8]) -> bool {
    let (mut t, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match template.get(t) {
            Some(b'*') => {
                star = Some((t, n));
                t += 1;
            }
            Some(&c) if c == b'?' || c == name[n] => {
                t += 1;
                n += 1;
            }
            _ => match star {
                Some((st, sn)) => {
                    t = st + 1;
                    n = sn + 1;
                    star = Some((st, sn + 1));
                }
                None => return false,
            },
        }
    }
    template[t.min(template.len())..].iter().all(|&c| c == b'*')
}
