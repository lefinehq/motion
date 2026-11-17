//! The objects of the language: small values that copy freely, and handles
//! to the composite ones (strings, arrays, dictionaries, files) whose bodies
//! live in the [`Vm`](crate::vm::Vm).
//!
//! Every object is either literal or executable. A composite handle names a
//! body and, for strings and arrays, the stretch of it the object sees, so
//! `getinterval` shares the body as the language requires.

/// An interned name, by its index in the VM's name table.
pub(crate) type Name = u32;

/// A view of a string body: `len` bytes from `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Str {
    pub id: u32,
    pub start: u32,
    pub len: u32,
}

/// A view of an array body: `len` objects from `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Arr {
    pub id: u32,
    pub start: u32,
    pub len: u32,
}

/// What an object holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Value {
    Null,
    Int(i32),
    Real(f64),
    Bool(bool),
    Name(Name),
    Mark,
    /// A built-in operator, by its index in the operator table.
    Operator(u16),
    String(Str),
    Array(Arr),
    /// A packed array: an array that `packedarray` or a packing scanner made.
    Packed(Arr),
    Dict(u32),
    File(u32),
    /// What `save` returned, by the depth of the save.
    Save(u32),
    /// A graphics state object, by its index in the VM.
    GState(u32),
    /// A font identifier (`fontID`), by its index.
    FontId(u32),
}

/// One object: a value and whether it is executable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Object {
    pub value: Value,
    pub exec: bool,
}

impl Object {
    pub const NULL: Self = Self::lit(Value::Null);
    pub const MARK: Self = Self::lit(Value::Mark);

    /// A literal object of `value`.
    pub const fn lit(value: Value) -> Self {
        Self { value, exec: false }
    }

    /// An executable object of `value`.
    pub const fn exe(value: Value) -> Self {
        Self { value, exec: true }
    }

    pub const fn int(value: i32) -> Self {
        Self::lit(Value::Int(value))
    }

    pub const fn real(value: f64) -> Self {
        Self::lit(Value::Real(value))
    }

    pub const fn bool(value: bool) -> Self {
        Self::lit(Value::Bool(value))
    }

    /// A number: an integer when `value` is whole and fits, a real otherwise.
    pub fn number(value: f64) -> Self {
        if value.fract() == 0.0 && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX) {
            // Whole and in range, so the cast is exact.
            #[allow(clippy::cast_possible_truncation)]
            Self::int(value as i32)
        } else {
            Self::real(value)
        }
    }

    /// An integer result of integer arithmetic, a real where it overflows.
    pub fn from_i64(value: i64) -> Self {
        match i32::try_from(value) {
            Ok(int) => Self::int(int),
            #[allow(clippy::cast_precision_loss)]
            Err(_) => Self::real(value as f64),
        }
    }

    /// This object, made executable.
    pub const fn cvx(self) -> Self {
        Self {
            value: self.value,
            exec: true,
        }
    }

    /// This object, made literal.
    pub const fn cvlit(self) -> Self {
        Self {
            value: self.value,
            exec: false,
        }
    }

    /// The number this object is, if it is one.
    pub fn as_f64(self) -> Option<f64> {
        match self.value {
            Value::Int(int) => Some(f64::from(int)),
            Value::Real(real) => Some(real),
            _ => None,
        }
    }

    /// The array view, for arrays and packed arrays.
    pub fn as_arr(self) -> Option<Arr> {
        match self.value {
            Value::Array(arr) | Value::Packed(arr) => Some(arr),
            _ => None,
        }
    }

    /// The language's name for this object's type, as `type` returns it.
    pub fn type_name(self) -> &'static str {
        match self.value {
            Value::Null => "nulltype",
            Value::Int(_) => "integertype",
            Value::Real(_) => "realtype",
            Value::Bool(_) => "booleantype",
            Value::Name(_) => "nametype",
            Value::Mark => "marktype",
            Value::Operator(_) => "operatortype",
            Value::String(_) => "stringtype",
            Value::Array(_) => "arraytype",
            Value::Packed(_) => "packedarraytype",
            Value::Dict(_) => "dicttype",
            Value::File(_) => "filetype",
            Value::Save(_) => "savetype",
            Value::GState(_) => "gstatetype",
            Value::FontId(_) => "fonttype",
        }
    }
}

/// A dictionary key. Strings become names, and reals that are whole become
/// integers, so `(a)` and `/a`, and `1.0` and `1`, find the same entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Null,
    Int(i64),
    /// The bits of a real that is not whole.
    Real(u64),
    Bool(bool),
    Name(Name),
    Mark,
    Operator(u16),
    Array(Arr),
    Dict(u32),
    File(u32),
    Save(u32),
    GState(u32),
    FontId(u32),
}
