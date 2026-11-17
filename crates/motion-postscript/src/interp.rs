//! The interpreter: the operand, dictionary, execution and graphics-state
//! stacks, and the loop that executes objects.
//!
//! The execution stack is explicit, a stack of [`Frame`]s, never Rust's own
//! call stack: procedures, loops and `stopped` contexts are frames the loop
//! steps, so a deep or endless program grows a vector the limits bound, and
//! `exit` and `stop` are a truncation of that vector. Only a procedure that
//! an operator has to run to finish its own work (a tint transform, a data
//! source) runs in a nested loop, and those nest at most
//! [`Limits::nesting`] deep.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::Limits;
use crate::graphics::GState;
use crate::object::{Arr, Key, Name, Object, Str, Value};
use crate::ops::{self, OPERATORS};
use crate::page::Builder;
use crate::vm::{File, Vm};

/// The errors of the language, which a program can catch with `stopped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum PsError {
    DictFull,
    DictStackOverflow,
    DictStackUnderflow,
    ExecStackOverflow,
    InvalidAccess,
    InvalidExit,
    InvalidFileAccess,
    InvalidFont,
    InvalidRestore,
    IoError,
    LimitCheck,
    NoCurrentPoint,
    RangeCheck,
    StackOverflow,
    StackUnderflow,
    SyntaxError,
    TypeCheck,
    Undefined,
    UndefinedFilename,
    UndefinedResource,
    UndefinedResult,
    UnmatchedMark,
    Unregistered,
    VmError,
    ConfigurationError,
}

impl PsError {
    /// Every error, for filling `errordict`.
    pub const ALL: [Self; 25] = [
        Self::DictFull,
        Self::DictStackOverflow,
        Self::DictStackUnderflow,
        Self::ExecStackOverflow,
        Self::InvalidAccess,
        Self::InvalidExit,
        Self::InvalidFileAccess,
        Self::InvalidFont,
        Self::InvalidRestore,
        Self::IoError,
        Self::LimitCheck,
        Self::NoCurrentPoint,
        Self::RangeCheck,
        Self::StackOverflow,
        Self::StackUnderflow,
        Self::SyntaxError,
        Self::TypeCheck,
        Self::Undefined,
        Self::UndefinedFilename,
        Self::UndefinedResource,
        Self::UndefinedResult,
        Self::UnmatchedMark,
        Self::Unregistered,
        Self::VmError,
        Self::ConfigurationError,
    ];

    /// The error's name in the language.
    pub const fn name(self) -> &'static str {
        match self {
            Self::DictFull => "dictfull",
            Self::DictStackOverflow => "dictstackoverflow",
            Self::DictStackUnderflow => "dictstackunderflow",
            Self::ExecStackOverflow => "execstackoverflow",
            Self::InvalidAccess => "invalidaccess",
            Self::InvalidExit => "invalidexit",
            Self::InvalidFileAccess => "invalidfileaccess",
            Self::InvalidFont => "invalidfont",
            Self::InvalidRestore => "invalidrestore",
            Self::IoError => "ioerror",
            Self::LimitCheck => "limitcheck",
            Self::NoCurrentPoint => "nocurrentpoint",
            Self::RangeCheck => "rangecheck",
            Self::StackOverflow => "stackoverflow",
            Self::StackUnderflow => "stackunderflow",
            Self::SyntaxError => "syntaxerror",
            Self::TypeCheck => "typecheck",
            Self::Undefined => "undefined",
            Self::UndefinedFilename => "undefinedfilename",
            Self::UndefinedResource => "undefinedresource",
            Self::UndefinedResult => "undefinedresult",
            Self::UnmatchedMark => "unmatchedmark",
            Self::Unregistered => "unregistered",
            Self::VmError => "VMerror",
            Self::ConfigurationError => "configurationerror",
        }
    }
}

/// Why a run ended before the program did. None of these can be caught by
/// the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Halt {
    /// The operation budget ran out.
    Budget,
    /// Procedures, loops or nested runs went deeper than the limits allow.
    Depth,
    /// The memory budget ran out.
    Memory,
    /// The program's output grew past the limits.
    Output,
    /// The program ended itself (`quit`, or `%%EOF` in the program file).
    Quit,
    /// An error no `stopped` caught: its name and the command that raised it.
    Error { name: String, command: String },
}

/// How an operator, or a step of the loop, can fail.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Fault {
    /// An error of the language, raised through `errordict`.
    Error(PsError),
    /// A `stop` on its way to the nearest `stopped`.
    Stop,
    /// The end of the run.
    Halt(Halt),
}

impl From<PsError> for Fault {
    fn from(error: PsError) -> Self {
        Self::Error(error)
    }
}

/// The result of an operator.
pub(crate) type R<T = ()> = Result<T, Fault>;

/// One entry of the execution stack.
#[derive(Debug, Clone)]
pub(crate) enum Frame {
    /// One object to execute as if a name had produced it.
    Exec(Object),
    /// A procedure, executing its elements from `pos`.
    Proc { arr: Arr, pos: u32 },
    /// A file whose tokens execute one by one.
    File(u32),
    /// An executable string whose tokens execute one by one.
    Text { data: Rc<[u8]>, pos: usize },
    /// `for` with integers.
    ForInt {
        next: i64,
        step: i64,
        limit: i64,
        body: Object,
    },
    /// `for` with reals.
    ForReal {
        next: f64,
        step: f64,
        limit: f64,
        body: Object,
        count: u64,
    },
    /// `repeat`, with the iterations left.
    Repeat { left: i64, body: Object },
    /// `loop`.
    Loop { body: Object },
    /// `forall` over an array, a string or a dictionary.
    Forall {
        over: Over,
        index: usize,
        body: Object,
    },
    /// `pathforall`, over a copy of the path in user space.
    PathForall {
        elements: Rc<[PathElement]>,
        index: usize,
        procs: [Object; 4],
    },
    /// A `stopped` context: reaching it normally pushes `false`.
    Stopped,
}

/// What `forall` walks.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Over {
    Array(Arr),
    String(Str),
    Dict(u32),
}

/// One element of a path as `pathforall` hands it out, in user space.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PathElement {
    Move(f64, f64),
    Line(f64, f64),
    Curve([f64; 6]),
    Close,
}

/// How the scanner reads comments of the document structure.
#[derive(Debug, Default)]
pub(crate) struct Structure {
    /// How deep inside embedded documents the scanner is.
    pub documents: u32,
    /// A binary section the program declared, as a byte range of the
    /// program file.
    pub binary: Option<(usize, usize)>,
}

/// One run of a program.
pub(crate) struct Interp {
    pub vm: Vm,
    pub ostack: Vec<Object>,
    pub dstack: Vec<u32>,
    pub estack: Vec<Frame>,
    /// The depth of the execution stack the current loop returns at.
    pub floor: usize,
    /// How many nested loops are running.
    pub nesting: u32,
    /// The object being executed, which an error names.
    pub current: Object,
    pub g: GState,
    /// The graphics states `gsave` and `save` keep, with the save level each
    /// was made at.
    pub gstack: Vec<(GState, u32)>,
    /// The save level: how many `save`s are open.
    pub save_level: u32,
    /// Graphics states that `gstate` and `currentgstate` made.
    pub gstates: Vec<GState>,
    pub out: Builder,
    pub limits: Limits,
    /// Operations executed so far.
    pub steps: u64,
    pub systemdict: u32,
    pub globaldict: u32,
    pub userdict: u32,
    pub errordict: u32,
    pub error_info: u32,
    pub statusdict: u32,
    pub fontdirectory: u32,
    /// Resource categories by name: each a dictionary of instances.
    pub resources: u32,
    /// The program file.
    pub program: u32,
    pub structure: Structure,
    /// Features the program used that the interpreter does not draw, with
    /// how often.
    pub unsupported: BTreeMap<String, u64>,
    pub names: Names,
    /// Whether `setglobal` chose global memory; recorded, not separated.
    pub global: bool,
    /// Whether `setpacking` asked for packed arrays; recorded only.
    pub packing: bool,
    /// The state of the pseudo-random generator.
    pub random: i32,
    /// The next font identifier.
    pub next_font: u32,
    /// Whether `showpage` has ended the page.
    pub shown: bool,
}

/// Names the interpreter looks up itself.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Names {
    pub newerror: Name,
    pub errorname: Name,
    pub command: Name,
}

impl Interp {
    /// A fresh interpreter with the standard dictionaries, ready to run
    /// `program`.
    pub fn new(program: &[u8], limits: Limits, out: Builder) -> Result<Self, Fault> {
        let mut vm = Vm::new(limits.memory);
        let systemdict = vm.new_dict(512)?;
        let globaldict = vm.new_dict(64)?;
        let userdict = vm.new_dict(256)?;
        let errordict = vm.new_dict(32)?;
        let error_info = vm.new_dict(16)?;
        let statusdict = vm.new_dict(32)?;
        let fontdirectory = vm.new_dict(16)?;
        let resources = vm.new_dict(32)?;
        let program_file = vm.new_file(File::Bytes {
            data: program.into(),
            pos: 0,
        })?;
        let names = Names {
            newerror: vm.name(b"newerror"),
            errorname: vm.name(b"errorname"),
            command: vm.name(b"command"),
        };
        let initial = out.initial_matrix();
        let mut interp = Self {
            vm,
            ostack: Vec::new(),
            dstack: vec![systemdict, globaldict, userdict],
            estack: Vec::new(),
            floor: 0,
            nesting: 0,
            current: Object::NULL,
            g: GState::new(initial),
            gstack: Vec::new(),
            save_level: 0,
            gstates: Vec::new(),
            out,
            limits,
            steps: 0,
            systemdict,
            globaldict,
            userdict,
            errordict,
            error_info,
            statusdict,
            fontdirectory,
            resources,
            program: program_file,
            structure: Structure::default(),
            unsupported: BTreeMap::new(),
            names,
            global: false,
            packing: false,
            random: 1,
            next_font: 0,
            shown: false,
        };
        ops::install(&mut interp)?;
        interp.estack.push(Frame::File(program_file));
        Ok(interp)
    }

    /// Counts `cost` operations against the budget.
    pub fn tick(&mut self, cost: u64) -> R {
        self.steps = self.steps.saturating_add(cost);
        if self.steps > self.limits.operations {
            return Err(Fault::Halt(Halt::Budget));
        }
        Ok(())
    }

    /// Records that the program used `feature`, which is not drawn.
    pub fn unsupported(&mut self, feature: impl Into<String>) {
        *self.unsupported.entry(feature.into()).or_default() += 1;
    }

    // ----- operand stack -------------------------------------------------

    /// Pushes `object`.
    pub fn push(&mut self, object: Object) -> R {
        if self.ostack.len() >= self.limits.operands {
            return Err(PsError::StackOverflow.into());
        }
        self.ostack.push(object);
        Ok(())
    }

    /// Pops the top object.
    pub fn pop(&mut self) -> R<Object> {
        self.ostack
            .pop()
            .ok_or(Fault::Error(PsError::StackUnderflow))
    }

    /// The object `depth` places below the top, without popping it.
    pub fn peek(&self, depth: usize) -> R<Object> {
        let len = self.ostack.len();
        if depth >= len {
            return Err(PsError::StackUnderflow.into());
        }
        Ok(self.ostack[len - 1 - depth])
    }

    /// Checks that `n` operands are there.
    pub fn need(&self, n: usize) -> R {
        if self.ostack.len() < n {
            return Err(PsError::StackUnderflow.into());
        }
        Ok(())
    }

    /// Drops `n` operands, which the caller has checked are there.
    pub fn drop_n(&mut self, n: usize) {
        let len = self.ostack.len().saturating_sub(n);
        self.ostack.truncate(len);
    }

    /// The number `depth` places below the top.
    pub fn num(&self, depth: usize) -> R<f64> {
        self.peek(depth)?
            .as_f64()
            .ok_or(Fault::Error(PsError::TypeCheck))
    }

    /// The integer `depth` places below the top.
    pub fn int(&self, depth: usize) -> R<i32> {
        match self.peek(depth)?.value {
            Value::Int(int) => Ok(int),
            _ => Err(PsError::TypeCheck.into()),
        }
    }

    /// The boolean `depth` places below the top.
    pub fn boolean(&self, depth: usize) -> R<bool> {
        match self.peek(depth)?.value {
            Value::Bool(b) => Ok(b),
            _ => Err(PsError::TypeCheck.into()),
        }
    }

    /// The procedure (or any object `if` may execute) `depth` places below.
    pub fn proc(&self, depth: usize) -> R<Object> {
        self.peek(depth)
    }

    /// The dictionary `depth` places below the top.
    pub fn dict(&self, depth: usize) -> R<u32> {
        match self.peek(depth)?.value {
            Value::Dict(id) => Ok(id),
            _ => Err(PsError::TypeCheck.into()),
        }
    }

    /// The string `depth` places below the top.
    pub fn string(&self, depth: usize) -> R<Str> {
        match self.peek(depth)?.value {
            Value::String(s) => Ok(s),
            _ => Err(PsError::TypeCheck.into()),
        }
    }

    /// The array or packed array `depth` places below the top.
    pub fn array(&self, depth: usize) -> R<Arr> {
        self.peek(depth)?
            .as_arr()
            .ok_or(Fault::Error(PsError::TypeCheck))
    }

    /// The numbers of an array of numbers.
    pub fn numbers(&self, arr: Arr) -> R<Vec<f64>> {
        self.vm
            .arr(arr)
            .iter()
            .map(|o| o.as_f64().ok_or(Fault::Error(PsError::TypeCheck)))
            .collect()
    }

    // ----- dictionaries --------------------------------------------------

    /// The value of `key` in the dictionary stack, from the top.
    pub fn lookup_key(&self, key: &Key) -> Option<Object> {
        self.dstack
            .iter()
            .rev()
            .find_map(|&dict| self.vm.dict_get(dict, key))
    }

    /// The value of the name `name` in the dictionary stack.
    pub fn lookup(&self, name: Name) -> Option<Object> {
        self.lookup_key(&Key::Name(name))
    }

    /// The value of the name spelled `name`, interning it.
    pub fn lookup_str(&mut self, name: &str) -> Option<Object> {
        let name = self.vm.name(name.as_bytes());
        self.lookup(name)
    }

    /// A literal name object for `name`.
    pub fn name_obj(&mut self, name: &str) -> Object {
        Object::lit(Value::Name(self.vm.name(name.as_bytes())))
    }

    /// The dictionary on top of the dictionary stack.
    pub fn current_dict(&self) -> u32 {
        self.dstack.last().copied().unwrap_or(self.userdict)
    }

    // ----- execution -----------------------------------------------------

    /// Pushes a frame, within the depth limit.
    pub fn push_frame(&mut self, frame: Frame) -> R {
        if self.estack.len() >= self.limits.depth {
            return Err(Fault::Halt(Halt::Depth));
        }
        self.estack.push(frame);
        Ok(())
    }

    /// Runs until the execution stack is back at the current floor.
    pub fn run(&mut self) -> R {
        while self.estack.len() > self.floor {
            self.tick(1)?;
            if let Err(fault) = self.step() {
                match fault {
                    Fault::Error(error) => self.raise(error)?,
                    Fault::Stop => self.unwind_stop()?,
                    Fault::Halt(halt) => return Err(Fault::Halt(halt)),
                }
            }
        }
        Ok(())
    }

    /// Runs `object` to its end in a nested loop, as an operator that needs
    /// its result does. A `stop` that nothing inside catches comes out as
    /// [`Fault::Stop`] for the caller's loop to carry on.
    pub fn call(&mut self, object: Object) -> R {
        if self.nesting >= self.limits.nesting {
            return Err(Fault::Halt(Halt::Depth));
        }
        let floor = self.floor;
        self.floor = self.estack.len();
        self.nesting += 1;
        let result = self
            .push_frame(Frame::Exec(object))
            .and_then(|()| self.run());
        self.nesting -= 1;
        self.estack.truncate(self.floor);
        self.floor = floor;
        result
    }

    /// Executes one step of the frame on top.
    fn step(&mut self) -> R {
        let last = self.estack.len() - 1;
        match &mut self.estack[last] {
            Frame::Exec(object) => {
                let object = *object;
                self.estack.pop();
                self.exec(object, true)
            }
            Frame::Proc { arr, pos } => {
                let arr = *arr;
                let index = *pos;
                *pos += 1;
                if index + 1 >= arr.len {
                    // The last element runs with the procedure already
                    // gone, so a procedure that ends by calling itself
                    // loops without growing the stack.
                    self.estack.pop();
                }
                match self.vm.arr_get(arr, index as usize) {
                    Some(object) => self.exec(object, false),
                    None => Ok(()),
                }
            }
            Frame::File(file) => {
                let file = *file;
                match self.scan_file(file)? {
                    Some(object) => self.exec(object, false),
                    None => {
                        self.pop_frame_at(last);
                        Ok(())
                    }
                }
            }
            Frame::Text { data, pos } => {
                let data = Rc::clone(data);
                let mut at = *pos;
                let token = self.scan(&data, &mut at, None);
                if let Some(Frame::Text { pos, .. }) = self.estack.get_mut(last) {
                    *pos = at;
                }
                match token? {
                    Some(object) => self.exec(object, false),
                    None => {
                        self.pop_frame_at(last);
                        Ok(())
                    }
                }
            }
            Frame::ForInt {
                next,
                step,
                limit,
                body,
            } => {
                let (value, body) = (*next, *body);
                let done = if *step >= 0 {
                    value > *limit
                } else {
                    value < *limit
                };
                if done {
                    self.estack.pop();
                    return Ok(());
                }
                *next = value.saturating_add(*step);
                self.push(Object::from_i64(value))?;
                self.push_frame(Frame::Exec(body))
            }
            Frame::ForReal {
                next,
                step,
                limit,
                body,
                count,
            } => {
                let (value, body) = (*next, *body);
                let done = if *step >= 0.0 {
                    value > *limit
                } else {
                    value < *limit
                };
                if done {
                    self.estack.pop();
                    return Ok(());
                }
                *count += 1;
                *next = value + *step;
                self.push(Object::real(value))?;
                self.push_frame(Frame::Exec(body))
            }
            Frame::Repeat { left, body } => {
                if *left <= 0 {
                    self.estack.pop();
                    return Ok(());
                }
                *left -= 1;
                let body = *body;
                self.push_frame(Frame::Exec(body))
            }
            Frame::Loop { body } => {
                let body = *body;
                self.push_frame(Frame::Exec(body))
            }
            Frame::Forall { over, index, body } => {
                let (over, at, body) = (*over, *index, *body);
                *index += 1;
                match over {
                    Over::Array(arr) => match self.vm.arr_get(arr, at) {
                        Some(object) => self.push(object)?,
                        None => {
                            self.estack.pop();
                            return Ok(());
                        }
                    },
                    Over::String(s) => match self.vm.str_bytes(s).get(at) {
                        Some(&byte) => self.push(Object::int(i32::from(byte)))?,
                        None => {
                            self.estack.pop();
                            return Ok(());
                        }
                    },
                    Over::Dict(dict) => {
                        let entry = self.vm.dicts[dict as usize]
                            .map
                            .get_index(at)
                            .map(|(key, value)| (*key, *value));
                        match entry {
                            Some((key, value)) => {
                                self.push(Vm::key_object(key))?;
                                self.push(value)?;
                            }
                            None => {
                                self.estack.pop();
                                return Ok(());
                            }
                        }
                    }
                }
                self.push_frame(Frame::Exec(body))
            }
            Frame::PathForall {
                elements,
                index,
                procs,
            } => {
                let Some(&element) = elements.get(*index) else {
                    self.estack.pop();
                    return Ok(());
                };
                *index += 1;
                let procs = *procs;
                let body = match element {
                    PathElement::Move(x, y) => {
                        self.push(Object::real(x))?;
                        self.push(Object::real(y))?;
                        procs[0]
                    }
                    PathElement::Line(x, y) => {
                        self.push(Object::real(x))?;
                        self.push(Object::real(y))?;
                        procs[1]
                    }
                    PathElement::Curve(c) => {
                        for v in c {
                            self.push(Object::real(v))?;
                        }
                        procs[2]
                    }
                    PathElement::Close => procs[3],
                };
                self.push_frame(Frame::Exec(body))
            }
            Frame::Stopped => {
                self.estack.pop();
                self.push(Object::bool(false))
            }
        }
    }

    /// Removes the frame at `index`, if it is still there.
    fn pop_frame_at(&mut self, index: usize) {
        if index < self.estack.len() {
            self.estack.remove(index);
        }
    }

    /// Executes `object`. `direct` is true when a name or an operator such
    /// as `exec` produced it, so a procedure runs rather than being pushed.
    pub fn exec(&mut self, object: Object, direct: bool) -> R {
        let mut object = object;
        let mut direct = direct;
        // An executable name may name another executable name; follow a
        // bounded number of them so a name defined as itself cannot hang.
        for _ in 0..64 {
            if !object.exec {
                return self.push(object);
            }
            match object.value {
                Value::Name(name) => {
                    self.current = object;
                    let Some(value) = self.lookup(name) else {
                        return Err(PsError::Undefined.into());
                    };
                    object = value;
                    direct = true;
                }
                Value::Operator(op) => {
                    self.current = object;
                    let Some(def) = OPERATORS.get(op as usize) else {
                        return Err(PsError::Undefined.into());
                    };
                    return (def.run)(self);
                }
                Value::Array(arr) | Value::Packed(arr) => {
                    if !direct {
                        return self.push(object);
                    }
                    if arr.len == 0 {
                        return Ok(());
                    }
                    return self.push_frame(Frame::Proc { arr, pos: 0 });
                }
                Value::String(s) => {
                    if !direct {
                        return self.push(object);
                    }
                    let data: Rc<[u8]> = self.vm.str_bytes(s).into();
                    return self.push_frame(Frame::Text { data, pos: 0 });
                }
                Value::File(file) => {
                    if !direct {
                        return self.push(object);
                    }
                    let file = self.executable_file(file)?;
                    return self.push_frame(Frame::File(file));
                }
                Value::Null => return Ok(()),
                _ => return self.push(object),
            }
        }
        Err(PsError::LimitCheck.into())
    }

    /// Raises `error` with the current command: the handler in `errordict`
    /// runs, which by default records the error and stops.
    fn raise(&mut self, error: PsError) -> R {
        let command = self.current;
        if self.ostack.len() < self.limits.operands {
            self.ostack.push(command);
        }
        let name = self.vm.name(error.name().as_bytes());
        let handler = self.vm.dict_get(self.errordict, &Key::Name(name));
        match handler {
            Some(handler) => self.push_frame(Frame::Exec(handler)),
            None => {
                self.record_error(name, command)?;
                self.unwind_stop()
            }
        }
    }

    /// Writes an error into `$error`, as the standard handlers do.
    pub fn record_error(&mut self, name: Name, command: Object) -> R {
        let info = self.error_info;
        self.vm
            .dict_put(info, Key::Name(self.names.newerror), Object::bool(true))?;
        self.vm.dict_put(
            info,
            Key::Name(self.names.errorname),
            Object::lit(Value::Name(name)),
        )?;
        self.vm
            .dict_put(info, Key::Name(self.names.command), command)
    }

    /// Carries a `stop` to the nearest `stopped` above the floor, which then
    /// pushes `true`. With none, a nested loop hands the stop to its caller
    /// and the outermost one ends the run.
    pub fn unwind_stop(&mut self) -> R {
        let found = (self.floor..self.estack.len())
            .rev()
            .find(|&i| matches!(self.estack[i], Frame::Stopped));
        if let Some(index) = found {
            self.estack.truncate(index);
            return self.push(Object::bool(true));
        }
        self.estack.truncate(self.floor);
        if self.nesting > 0 {
            return Err(Fault::Stop);
        }
        Err(Fault::Halt(self.uncaught()))
    }

    /// The error a run ends with when a stop reaches the top.
    fn uncaught(&mut self) -> Halt {
        let info = self.error_info;
        let newerror = self.vm.dict_get(info, &Key::Name(self.names.newerror));
        if newerror != Some(Object::bool(true)) {
            return Halt::Error {
                name: "stop".into(),
                command: "stop".into(),
            };
        }
        let name = match self.vm.dict_get(info, &Key::Name(self.names.errorname)) {
            Some(Object {
                value: Value::Name(name),
                ..
            }) => self.vm.name_text(name),
            _ => "error".into(),
        };
        let mut command = self
            .vm
            .dict_get(info, &Key::Name(self.names.command))
            .map_or_else(String::new, |command| self.describe(command));
        // The operands it was given, nearest last, help say which call.
        let shown: Vec<String> = self
            .ostack
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(|o| self.describe(*o))
            .collect();
        if !shown.is_empty() {
            command = format!("{} {command}", shown.join(" "));
        }
        Halt::Error { name, command }
    }

    /// A short text for an object, as error messages name a command.
    pub fn describe(&self, object: Object) -> String {
        match object.value {
            Value::Name(name) if object.exec => self.vm.name_text(name),
            Value::Name(name) => format!("/{}", self.vm.name_text(name)),
            Value::Operator(op) => OPERATORS
                .get(op as usize)
                .map_or_else(|| "operator".into(), |def| def.name.into()),
            Value::Int(int) => int.to_string(),
            Value::Real(real) => real.to_string(),
            Value::String(s) => {
                let bytes = self.vm.str_bytes(s);
                let shown = &bytes[..bytes.len().min(32)];
                format!("({})", String::from_utf8_lossy(shown))
            }
            _ => format!("--{}--", object.type_name()),
        }
    }

    /// The file the innermost file frame reads: `currentfile`.
    pub fn current_file(&self) -> Option<u32> {
        self.estack.iter().rev().find_map(|frame| match frame {
            Frame::File(file) => Some(*file),
            _ => None,
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/interp.rs"]
mod tests;
