//! The virtual memory: the bodies of every composite object, by index.
//!
//! Bodies live in tables owned by one interpreter run and are dropped with
//! it, so dictionaries that hold themselves (`userdict` does) leak nothing.
//! Every allocation counts against one memory budget, so a program cannot
//! grow without bound.

use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::filter::Filter;
use crate::interp::{Fault, Halt};
use crate::object::{Arr, Key, Name, Object, Str, Value};

/// A dictionary body.
#[derive(Debug, Default, Clone)]
pub(crate) struct Dict {
    /// The entries, in the order they were first defined, so `forall`
    /// visits them the same way on every run.
    pub map: IndexMap<Key, Object>,
    /// The capacity the program asked for, as `maxlength` reports it.
    pub max: usize,
    pub readonly: bool,
}

/// A file body.
#[derive(Debug)]
pub(crate) enum File {
    /// Bytes in memory, read from `pos`: the program itself, a string read
    /// as a file, or a filter's output drained to be executed.
    Bytes {
        data: Rc<[u8]>,
        pos: usize,
    },
    /// A decoding filter and what it has decoded but not yet handed out.
    Filter(Box<Filter>),
    /// An output file whose bytes go nowhere (`%stdout`, `%stderr`).
    Sink,
    Closed,
}

/// The tables of one run.
#[derive(Debug, Default)]
pub(crate) struct Vm {
    names: Vec<Box<[u8]>>,
    by_name: HashMap<Box<[u8]>, Name>,
    pub strings: Vec<Vec<u8>>,
    pub arrays: Vec<Vec<Object>>,
    pub dicts: Vec<Dict>,
    pub files: Vec<File>,
    /// Units allocated so far: a byte of string, an array slot or a
    /// dictionary entry each count one.
    used: usize,
    /// The most units a run may allocate.
    budget: usize,
}

impl Vm {
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    /// Counts `units` against the memory budget.
    pub fn charge(&mut self, units: usize) -> Result<(), Fault> {
        self.used = self.used.saturating_add(units);
        if self.used > self.budget {
            return Err(Fault::Halt(Halt::Memory));
        }
        Ok(())
    }

    /// The name spelled `bytes`, interned on first use.
    pub fn name(&mut self, bytes: &[u8]) -> Name {
        if let Some(&name) = self.by_name.get(bytes) {
            return name;
        }
        let name = u32::try_from(self.names.len()).unwrap_or(u32::MAX);
        self.names.push(bytes.into());
        self.by_name.insert(bytes.into(), name);
        name
    }

    /// The spelling of `name`.
    pub fn name_bytes(&self, name: Name) -> &[u8] {
        self.names.get(name as usize).map_or(&[], |bytes| bytes)
    }

    /// The spelling of `name`, as text for messages.
    pub fn name_text(&self, name: Name) -> String {
        String::from_utf8_lossy(self.name_bytes(name)).into_owned()
    }

    /// A new string body holding `bytes`.
    pub fn new_string(&mut self, bytes: Vec<u8>) -> Result<Str, Fault> {
        self.charge(bytes.len())?;
        let len = u32::try_from(bytes.len()).map_err(|_| Fault::Halt(Halt::Memory))?;
        let id = u32::try_from(self.strings.len()).map_err(|_| Fault::Halt(Halt::Memory))?;
        self.strings.push(bytes);
        Ok(Str { id, start: 0, len })
    }

    /// The bytes a string object sees.
    pub fn str_bytes(&self, s: Str) -> &[u8] {
        let body = &self.strings[s.id as usize];
        let start = (s.start as usize).min(body.len());
        let end = (start + s.len as usize).min(body.len());
        &body[start..end]
    }

    /// The bytes a string object sees, to change in place.
    pub fn str_bytes_mut(&mut self, s: Str) -> &mut [u8] {
        let body = &mut self.strings[s.id as usize];
        let start = (s.start as usize).min(body.len());
        let end = (start + s.len as usize).min(body.len());
        &mut body[start..end]
    }

    /// A new array body holding `objects`.
    pub fn new_array(&mut self, objects: Vec<Object>) -> Result<Arr, Fault> {
        self.charge(objects.len())?;
        let len = u32::try_from(objects.len()).map_err(|_| Fault::Halt(Halt::Memory))?;
        let id = u32::try_from(self.arrays.len()).map_err(|_| Fault::Halt(Halt::Memory))?;
        self.arrays.push(objects);
        Ok(Arr { id, start: 0, len })
    }

    /// The objects an array object sees.
    pub fn arr(&self, a: Arr) -> &[Object] {
        let body = &self.arrays[a.id as usize];
        let start = (a.start as usize).min(body.len());
        let end = (start + a.len as usize).min(body.len());
        &body[start..end]
    }

    /// The objects an array object sees, to change in place.
    pub fn arr_mut(&mut self, a: Arr) -> &mut [Object] {
        let body = &mut self.arrays[a.id as usize];
        let start = (a.start as usize).min(body.len());
        let end = (start + a.len as usize).min(body.len());
        &mut body[start..end]
    }

    /// One element of an array object.
    pub fn arr_get(&self, a: Arr, index: usize) -> Option<Object> {
        self.arr(a).get(index).copied()
    }

    /// A new dictionary body with room for `max` entries.
    pub fn new_dict(&mut self, max: usize) -> Result<u32, Fault> {
        self.charge(1)?;
        let id = u32::try_from(self.dicts.len()).map_err(|_| Fault::Halt(Halt::Memory))?;
        self.dicts.push(Dict {
            map: IndexMap::new(),
            max,
            readonly: false,
        });
        Ok(id)
    }

    /// The dictionary key for `object`.
    pub fn key(&mut self, object: Object) -> Key {
        match object.value {
            Value::Null => Key::Null,
            Value::Int(int) => Key::Int(i64::from(int)),
            Value::Real(real) => {
                if real.fract() == 0.0 && real.abs() < 9.0e15 {
                    // Whole and well inside the range of `i64`.
                    #[allow(clippy::cast_possible_truncation)]
                    Key::Int(real as i64)
                } else {
                    Key::Real(real.to_bits())
                }
            }
            Value::Bool(b) => Key::Bool(b),
            Value::Name(name) => Key::Name(name),
            Value::Mark => Key::Mark,
            Value::Operator(op) => Key::Operator(op),
            Value::String(s) => {
                let bytes = self.str_bytes(s).to_vec();
                Key::Name(self.name(&bytes))
            }
            Value::Array(arr) | Value::Packed(arr) => Key::Array(arr),
            Value::Dict(id) => Key::Dict(id),
            Value::File(id) => Key::File(id),
            Value::Save(id) => Key::Save(id),
            Value::GState(id) => Key::GState(id),
            Value::FontId(id) => Key::FontId(id),
        }
    }

    /// The object a key stands for, as `forall` hands keys out.
    pub fn key_object(key: Key) -> Object {
        match key {
            Key::Null => Object::NULL,
            Key::Int(int) => Object::from_i64(int),
            Key::Real(bits) => Object::real(f64::from_bits(bits)),
            Key::Bool(b) => Object::bool(b),
            Key::Name(name) => Object::lit(Value::Name(name)),
            Key::Mark => Object::MARK,
            Key::Operator(op) => Object::exe(Value::Operator(op)),
            Key::Array(arr) => Object::lit(Value::Array(arr)),
            Key::Dict(id) => Object::lit(Value::Dict(id)),
            Key::File(id) => Object::lit(Value::File(id)),
            Key::Save(id) => Object::lit(Value::Save(id)),
            Key::GState(id) => Object::lit(Value::GState(id)),
            Key::FontId(id) => Object::lit(Value::FontId(id)),
        }
    }

    /// The value of `key` in dictionary `dict`.
    pub fn dict_get(&self, dict: u32, key: &Key) -> Option<Object> {
        self.dicts[dict as usize].map.get(key).copied()
    }

    /// Defines `key` as `value` in dictionary `dict`.
    pub fn dict_put(&mut self, dict: u32, key: Key, value: Object) -> Result<(), Fault> {
        let map = &mut self.dicts[dict as usize].map;
        if map.insert(key, value).is_none() {
            self.charge(1)?;
        }
        Ok(())
    }

    /// Defines the name `name` as `value` in dictionary `dict`.
    pub fn dict_put_name(&mut self, dict: u32, name: &str, value: Object) -> Result<(), Fault> {
        let key = Key::Name(self.name(name.as_bytes()));
        self.dict_put(dict, key, value)
    }

    /// A new file body.
    pub fn new_file(&mut self, file: File) -> Result<u32, Fault> {
        self.charge(1)?;
        let id = u32::try_from(self.files.len()).map_err(|_| Fault::Halt(Halt::Memory))?;
        self.files.push(file);
        Ok(id)
    }
}
