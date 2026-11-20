//! [`Engine`]: compiles expressions once and runs them at any time, over a
//! [`Host`], deterministically. The JavaScript engine stays inside ([D37]).
//!
//! [D37]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d37-engines-stay-behind-our-own-types

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use boa_engine::JsObject;
use boa_engine::object::builtins::JsArray;
use boa_engine::{
    Context, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Script, Source,
    js_string,
};
use nakkash_format::{Diagnostic, Kind, Result};

use crate::Dialect;
use crate::host::{Host, Marker, Missing, NodeId, NodeKind, Select, Val};
use crate::noise;
use crate::rewrite::rewrite;

/// The language functions that need no host.
const LIBRARY: &str = include_str!("library.js");
/// The expression objects over the host.
const OBJECTS: &str = include_str!("host.js");
/// The legacy engine's aliases.
const LEGACY: &str = include_str!("legacy.js");

/// The stack an engine's thread should have. The parser and the
/// interpreter recurse: every corpus expression compiles in 4 MB in a debug
/// build and 2 MB in a release one, and this leaves room for deep runs.
pub const STACK_SIZE: usize = 32 << 20;

/// The globals the prelude starts with. `time` reads `$time`, which each
/// run sets, and marks the run as one that changes with time; the prelude
/// reads `$time` itself, through `$now`, which marks it only when what it
/// reads at that time changes. `time` cannot be deleted or redefined, so no
/// run can leave a `time` that reads unmarked.
const TIME: &str = "delete globalThis.Date; Math.random = function () { return __random(); };
var $time = 0;
Object.defineProperty(globalThis, 'time', {
  get: function () { __vary(-1, false); return $time; },
  set: function (v) { $time = v; },
  configurable: false
});
";

/// The marker of a compiled, encoded script, which cannot be read.
const ENCODED: &str = "@JSXBIN@";

/// How much one run may do before it is stopped: a runaway loop in an
/// expression is an error, not a hung import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Iterations of any one loop.
    pub loop_iterations: u64,
    /// Nested calls.
    pub recursion: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            loop_iterations: 1_000_000,
            recursion: 400,
        }
    }
}

/// A compiled expression of an [`Engine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Program(usize);

/// What compiling an expression gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    /// The program to run.
    pub program: Program,
    /// The legacy forms repaired before it compiled.
    pub repairs: Vec<String>,
}

/// What one run gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The expression's value.
    pub value: Val,
    /// Whether it used noise or randomness, which we cannot reproduce
    /// exactly: such a value is our own approximation.
    pub approximate: bool,
    /// Whether it read nothing that changes with time, so it gives this
    /// value at every time: not `time`, no value the host says changes,
    /// and no randomness seeded by the time.
    pub constant: bool,
}

/// Why a run gave no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// It read another expression's property, which has to be baked first.
    Pending(NodeId),
    /// It read something the import cannot know.
    Unknown(String),
    /// It read the rectangle of a layer's content, which the host has not
    /// measured yet ([`Missing::Unmeasured`]).
    Unmeasured,
    /// It threw, or gave something that is not a value.
    Error(String),
}

/// The state natives share, kept in the context.
struct State {
    host: Rc<dyn Host>,
    this: Cell<NodeId>,
    missing: RefCell<Option<Missing>>,
    approximate: Cell<bool>,
    random: Cell<u64>,
    /// Whether the random sequence was seeded without the time.
    timeless: Cell<bool>,
    /// Whether the run read something that changes with time.
    varies: Cell<bool>,
    time: Cell<f64>,
    /// The part of a text the runs evaluate an expression selector for: its
    /// index, from 1, and how many parts there are.
    part: Cell<Option<(u32, u32)>>,
}

/// Compiles and runs expressions over one host. Not `Send`: an engine
/// lives on one thread, one per thread, and that thread should have a
/// stack of [`STACK_SIZE`].
pub struct Engine {
    context: Context,
    programs: Vec<Script>,
    reset: Script,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("programs", &self.programs.len())
            .finish()
    }
}

/// A host error as a JavaScript error, recorded so a `try` in the
/// expression cannot hide it.
fn missing(ctx: &mut Context, why: Missing) -> JsError {
    let message = match &why {
        Missing::Pending(node) => format!("waits for the expression on node {node}"),
        Missing::Unknown(what) => format!("reads {what}, which the import cannot know"),
        Missing::Unmeasured => "reads the size of a layer's content, not measured yet".into(),
    };
    if let Some(state) = ctx.get_data::<State>() {
        state.missing.borrow_mut().get_or_insert(why);
    }
    JsNativeError::error().with_message(message).into()
}

fn state(ctx: &Context) -> &State {
    ctx.get_data::<State>()
        .expect("the engine keeps its state in the context")
}

fn arg(args: &[JsValue], i: usize) -> JsValue {
    args.get(i).cloned().unwrap_or_default()
}

fn number(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<f64> {
    arg(args, i).to_number(ctx)
}

fn node(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<NodeId> {
    let n = number(args, i, ctx)?;
    if n >= 0.0 && n.fract() == 0.0 && n <= f64::from(u32::MAX) {
        Ok(n as NodeId)
    } else {
        Err(JsNativeError::range().with_message("not a node").into())
    }
}

fn id(found: Option<NodeId>) -> JsValue {
    found.map_or(JsValue::from(-1), JsValue::from)
}

/// A value from the host as JavaScript.
fn to_js(value: Val, ctx: &mut Context) -> JsValue {
    match value {
        Val::Number(n) => n.into(),
        Val::Vector(v) => JsArray::from_iter(v.into_iter().map(JsValue::from), ctx).into(),
        Val::Text(t) => JsString::from(t.as_str()).into(),
        Val::Marker(marker) => marker_js(&marker, ctx),
    }
}

/// A marker as the object a marker key spreads over itself: its fields
/// under the language's names, its cue-point parameters as an object.
fn marker_js(marker: &Marker, ctx: &mut Context) -> JsValue {
    let text = |s: &str| JsValue::from(JsString::from(s));
    let parameters = JsObject::with_object_proto(ctx.intrinsics());
    for (key, value) in &marker.parameters {
        parameters
            .set(JsString::from(key.as_str()), text(value), false, ctx)
            .expect("a plain object takes any key");
    }
    let fields = [
        ("comment", text(&marker.comment)),
        ("duration", marker.duration.into()),
        ("chapter", text(&marker.chapter)),
        ("url", text(&marker.url)),
        ("frameTarget", text(&marker.frame_target)),
        ("cuePointName", text(&marker.cue_point_name)),
        ("eventCuePoint", marker.event_cue_point.into()),
        ("protectedRegion", marker.protected_region.into()),
        ("parameters", parameters.into()),
        ("$marker", true.into()),
    ];
    let object = JsObject::with_object_proto(ctx.intrinsics());
    for (name, value) in fields {
        object
            .set(JsString::from(name), value, false, ctx)
            .expect("a plain object takes any key");
    }
    object.into()
}

/// A JavaScript value as a value, or why it is not one.
fn from_js(value: &JsValue, ctx: &mut Context) -> std::result::Result<Val, String> {
    if let Some(object) = value.as_object() {
        if let Ok(array) = JsArray::from_object(object.clone()) {
            let length = array.length(ctx).map_err(|e| e.to_string())?;
            let mut out = Vec::with_capacity(length as usize);
            for i in 0..length {
                let item = array.get(i, ctx).map_err(|e| e.to_string())?;
                let n = item.to_number(ctx).map_err(|e| e.to_string())?;
                out.push(n);
            }
            return Ok(Val::Vector(out));
        }
        let primitive = value
            .to_primitive(ctx, boa_engine::value::PreferredType::Number)
            .map_err(|e| e.to_string())?;
        return from_js(&primitive, ctx);
    }
    if let Some(s) = value.as_string() {
        return Ok(Val::Text(s.to_std_string_escaped()));
    }
    if let Some(b) = value.as_boolean() {
        return Ok(Val::Number(f64::from(u8::from(b))));
    }
    match value.as_number() {
        Some(n) => Ok(Val::Number(n)),
        None => Err(format!("it gave `{}`, not a value", value.display())),
    }
}

/// The host's natives.
fn natives() -> Vec<(&'static str, usize, NativeFunction)> {
    vec![
        (
            "__project",
            0,
            NativeFunction::from_fn_ptr(|_, _, ctx| Ok(state(ctx).host.project().into())),
        ),
        (
            "__this",
            0,
            NativeFunction::from_fn_ptr(|_, _, ctx| Ok(state(ctx).this.get().into())),
        ),
        (
            "__kind",
            1,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                Ok(match state(ctx).host.kind(n) {
                    NodeKind::Project => 0,
                    NodeKind::Comp => 1,
                    NodeKind::Layer => 2,
                    NodeKind::Group => 3,
                    NodeKind::Property => 4,
                }
                .into())
            }),
        ),
        (
            "__up",
            1,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                Ok(id(state(ctx).host.up(n)))
            }),
        ),
        (
            "__child",
            3,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let Ok(n) = node(a, 0, ctx) else {
                    return Ok((-1).into());
                };
                let by_attribute = arg(a, 2).to_boolean();
                let sel = arg(a, 1);
                let host = state(ctx).host.clone();
                let found = match sel.as_number() {
                    Some(i) if !by_attribute => host.child(n, Select::Index(i as i64)),
                    _ => {
                        let name = sel.to_string(ctx)?.to_std_string_escaped();
                        match by_attribute {
                            true => host.child(n, Select::Attribute(&name)),
                            false => host.child(n, Select::Name(&name)),
                        }
                    }
                };
                Ok(id(found))
            }),
        ),
        (
            "__link",
            2,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                let name = arg(a, 1).to_string(ctx)?.to_std_string_escaped();
                Ok(id(state(ctx).host.link(n, &name)))
            }),
        ),
        (
            "__attr",
            2,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let Ok(n) = node(a, 0, ctx) else {
                    return Ok(JsValue::undefined());
                };
                let name = arg(a, 1).to_string(ctx)?.to_std_string_escaped();
                let found = state(ctx).host.attribute(n, &name);
                Ok(found.map_or(JsValue::undefined(), |v| to_js(v, ctx)))
            }),
        ),
        (
            "__vary",
            2,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                // A negative id is a read of no one property.
                let n = node(a, 0, ctx).ok();
                let own = arg(a, 1).to_boolean();
                let st = state(ctx);
                let constant = n.is_some_and(|n| st.host.constant(n, own));
                if !constant {
                    st.varies.set(true);
                }
                Ok(JsValue::undefined())
            }),
        ),
        (
            "__valueAt",
            3,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                let t = number(a, 1, ctx)?;
                let own = arg(a, 2).to_boolean();
                let found = state(ctx).host.value_at(n, t, own);
                match found {
                    Ok(v) => Ok(to_js(v, ctx)),
                    Err(why) => Err(missing(ctx, why)),
                }
            }),
        ),
        (
            "__numKeys",
            1,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                Ok((state(ctx).host.key_count(n) as f64).into())
            }),
        ),
        (
            "__keyTime",
            2,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                let k = number(a, 1, ctx)? as usize;
                let found = state(ctx).host.key(n, k);
                found
                    .map(|(t, _)| t.into())
                    .ok_or_else(|| JsNativeError::range().with_message("no such key").into())
            }),
        ),
        (
            "__keyValue",
            2,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                let k = number(a, 1, ctx)? as usize;
                let found = state(ctx).host.key(n, k);
                match found {
                    Some((_, v)) => Ok(to_js(v, ctx)),
                    None => Err(JsNativeError::range().with_message("no such key").into()),
                }
            }),
        ),
        (
            "__rectAt",
            3,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = node(a, 0, ctx)?;
                let t = number(a, 1, ctx)?;
                let extents = arg(a, 2).to_boolean();
                let found = state(ctx).host.rect_at(n, t, extents);
                match found {
                    Ok(r) => Ok(to_js(Val::Vector(r.to_vec()), ctx)),
                    Err(why) => Err(missing(ctx, why)),
                }
            }),
        ),
        (
            "__noise",
            4,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let n = number(a, 0, ctx)?;
                let dim = number(a, 1, ctx)?;
                let octave = number(a, 2, ctx)?;
                let x = number(a, 3, ctx)?;
                let st = state(ctx);
                st.approximate.set(true);
                let seed = noise::mix(&[
                    n.to_bits(),
                    dim.to_bits(),
                    octave.to_bits(),
                    u64::from(st.this.get()),
                ]);
                Ok(noise::gradient(seed, x).into())
            }),
        ),
        (
            "__random",
            0,
            NativeFunction::from_fn_ptr(|_, _, ctx| {
                let st = state(ctx);
                st.approximate.set(true);
                if !st.timeless.get() {
                    st.varies.set(true);
                }
                let (next, value) = noise::next(st.random.get());
                st.random.set(next);
                Ok(value.into())
            }),
        ),
        (
            "__textPart",
            1,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let which = number(a, 0, ctx)?;
                match state(ctx).part.get() {
                    Some((index, _)) if which == 0.0 => Ok(index.into()),
                    Some((_, total)) => Ok(total.into()),
                    None => Err(missing(
                        ctx,
                        Missing::Unknown(
                            "the part of the text it chooses, which only an expression \
                             selector has"
                                .into(),
                        ),
                    )),
                }
            }),
        ),
        (
            "__seedRandom",
            2,
            NativeFunction::from_fn_ptr(|_, a, ctx| {
                let offset = number(a, 0, ctx)?;
                let timeless = arg(a, 1).to_boolean();
                let st = state(ctx);
                st.timeless.set(timeless);
                let time = if timeless { 0.0 } else { st.time.get() };
                st.random.set(noise::mix(&[
                    u64::from(st.this.get()),
                    offset.to_bits(),
                    time.to_bits(),
                ]));
                Ok(JsValue::undefined())
            }),
        ),
    ]
}

/// `String.prototype` methods that both of the original's engines have and
/// the embedded one lacks: those of the language's Annex B, which the
/// embedded engine leaves out ([`substr`]).
fn strings() -> Vec<(&'static str, usize, NativeFunction)> {
    vec![("substr", 2, NativeFunction::from_fn_ptr(substr))]
}

/// `String.prototype.substr(start, length)`, as ECMA-262 Annex B.2.2.1
/// gives it: the `length` code units of the string from `start`. A negative
/// `start` counts from the end; a start past either end stops there; a
/// `length` left out or undefined runs to the end, and any other is
/// clamped between 0 and the string's length. Numbers become integers as
/// `ToIntegerOrInfinity` makes them: NaN is 0, a fraction is cut toward 0.
///
/// # Errors
/// A `TypeError` on `null` or `undefined`, and whatever converting the
/// string or a number throws.
fn substr(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let text = this.require_object_coercible()?.to_string(ctx)?;
    #[expect(clippy::cast_precision_loss, reason = "string lengths far below 2^52")]
    let size = text.len() as f64;
    let start = integer_or_infinity(&arg(args, 0), ctx)?;
    let start = match start < 0.0 {
        true => (size + start).max(0.0),
        false => start.min(size),
    };
    let length = match arg(args, 1) {
        length if length.is_undefined() => size,
        length => integer_or_infinity(&length, ctx)?.clamp(0.0, size),
    };
    let end = (start + length).min(size);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole numbers between 0 and the string's length"
    )]
    let range = start as usize..end as usize;
    Ok(text.get(range).unwrap_or_default().into())
}

/// ECMA-262's `ToIntegerOrInfinity`: `value` as a number, NaN as 0, cut
/// toward 0, infinities kept.
fn integer_or_infinity(value: &JsValue, ctx: &mut Context) -> JsResult<f64> {
    let n = value.to_number(ctx)?;
    Ok(if n.is_nan() { 0.0 } else { n.trunc() })
}

/// `Math` functions that are not exact in IEEE arithmetic, on `libm`, so a
/// bake gives the same bits on every platform.
fn maths() -> Vec<(&'static str, usize, NativeFunction)> {
    macro_rules! one {
        ($name:literal, $f:path) => {
            (
                $name,
                1,
                NativeFunction::from_fn_ptr(|_, a, ctx| Ok($f(number(a, 0, ctx)?).into())),
            )
        };
    }
    macro_rules! two {
        ($name:literal, $f:path) => {
            (
                $name,
                2,
                NativeFunction::from_fn_ptr(|_, a, ctx| {
                    let x = number(a, 0, ctx)?;
                    Ok($f(x, number(a, 1, ctx)?).into())
                }),
            )
        };
    }
    vec![
        one!("sin", libm::sin),
        one!("cos", libm::cos),
        one!("tan", libm::tan),
        one!("asin", libm::asin),
        one!("acos", libm::acos),
        one!("atan", libm::atan),
        one!("sinh", libm::sinh),
        one!("cosh", libm::cosh),
        one!("tanh", libm::tanh),
        one!("asinh", libm::asinh),
        one!("acosh", libm::acosh),
        one!("atanh", libm::atanh),
        one!("exp", libm::exp),
        one!("expm1", libm::expm1),
        one!("log", libm::log),
        one!("log1p", libm::log1p),
        one!("log2", libm::log2),
        one!("log10", libm::log10),
        one!("cbrt", libm::cbrt),
        two!("atan2", libm::atan2),
        two!("pow", js_pow),
        two!("hypot", libm::hypot),
    ]
}

/// `Math.pow` as the language defines it, where it differs from C's `pow`:
/// a NaN exponent gives NaN, and ±1 to an infinite power gives NaN.
fn js_pow(x: f64, y: f64) -> f64 {
    if y.is_nan() || (x.abs() == 1.0 && y.is_infinite()) {
        return f64::NAN;
    }
    libm::pow(x, y)
}

impl Engine {
    /// An engine over `host` for projects of `dialect`, with `limits`.
    ///
    /// # Errors
    /// A diagnostic when the engine's own prelude does not load, which is a
    /// bug.
    pub fn new(host: Rc<dyn Host>, dialect: Dialect, limits: Limits) -> Result<Self> {
        let mut context = Context::default();
        context
            .runtime_limits_mut()
            .set_loop_iteration_limit(limits.loop_iterations);
        context
            .runtime_limits_mut()
            .set_recursion_limit(limits.recursion);
        context.insert_data(State {
            host,
            this: Cell::new(0),
            missing: RefCell::new(None),
            approximate: Cell::new(false),
            random: Cell::new(0),
            timeless: Cell::new(false),
            varies: Cell::new(false),
            time: Cell::new(0.0),
            part: Cell::new(None),
        });
        let failed = |e: JsError| {
            Diagnostic::new(Kind::Invalid, format!("the expression prelude failed: {e}"))
        };
        for (name, length, function) in natives() {
            context
                .register_global_callable(JsString::from(name), length, function)
                .map_err(failed)?;
        }
        let math = context
            .global_object()
            .get(js_string!("Math"), &mut context)
            .map_err(failed)?
            .as_object()
            .expect("Math is an object")
            .clone();
        for (name, length, function) in maths() {
            let f = boa_engine::object::FunctionObjectBuilder::new(context.realm(), function)
                .name(JsString::from(name))
                .length(length)
                .build();
            math.set(JsString::from(name), f, false, &mut context)
                .map_err(failed)?;
        }
        // Built-in methods, as the language's own are: writable,
        // configurable and not enumerated.
        let string = context.intrinsics().constructors().string().prototype();
        for (name, length, function) in strings() {
            let f = boa_engine::object::FunctionObjectBuilder::new(context.realm(), function)
                .name(JsString::from(name))
                .length(length)
                .build();
            let method = boa_engine::property::PropertyDescriptor::builder()
                .value(f)
                .writable(true)
                .enumerable(false)
                .configurable(true);
            string
                .define_property_or_throw(JsString::from(name), method, &mut context)
                .map_err(failed)?;
        }
        let mut prelude = String::from(TIME);
        // The prelude's own arithmetic is not rewritten: the helpers are made of it.
        prelude.push_str(LIBRARY);
        prelude.push('\n');
        prelude.push_str(OBJECTS);
        if dialect == Dialect::Legacy {
            prelude.push('\n');
            prelude.push_str(LEGACY);
        }
        prelude.push_str("\n$mark();");
        context.eval(Source::from_bytes(&prelude)).map_err(failed)?;
        let reset =
            Script::parse(Source::from_bytes("$reset();"), None, &mut context).map_err(failed)?;
        Ok(Self {
            context,
            programs: Vec::new(),
            reset,
        })
    }

    /// Compiles `source`, written for `dialect`.
    ///
    /// # Errors
    /// A syntax diagnostic when it does not parse, after the legacy repairs,
    /// and an invalid one when it runs a compiled, encoded script.
    pub fn compile(&mut self, source: &str, dialect: Dialect) -> Result<Compiled> {
        if source.contains(ENCODED) {
            return Err(Diagnostic::new(
                Kind::Invalid,
                "the expression runs a compiled, encoded script, which cannot be read",
            ));
        }
        let rewritten = rewrite(source, dialect)?;
        let script = Script::parse(
            Source::from_bytes(&rewritten.source),
            None,
            &mut self.context,
        )
        .map_err(|e| {
            Diagnostic::new(
                Kind::Syntax,
                format!("the expression does not compile: {e}"),
            )
        })?;
        self.programs.push(script);
        Ok(Compiled {
            program: Program(self.programs.len() - 1),
            repairs: rewritten.repairs,
        })
    }

    /// Sets the part of a text the next runs evaluate an expression
    /// selector for: its index, from 1, and how many parts there are; `None`
    /// for any other expression. `textIndex`, `textTotal` and
    /// `selectorValue` read it, and fail without one.
    pub fn set_part(&mut self, part: Option<(u32, u32)>) {
        state(&self.context).part.set(part);
    }

    /// Runs `program` as the expression of the property `this`, at `time`
    /// in its comp's seconds. Nothing a run sets survives it.
    ///
    /// # Errors
    /// A [`Failure`]: the run waits for another expression, reads what the
    /// import cannot know, or threw.
    pub fn run(
        &mut self,
        program: Program,
        this: NodeId,
        time: f64,
    ) -> std::result::Result<Run, Failure> {
        let st = state(&self.context);
        st.this.set(this);
        st.time.set(time);
        st.approximate.set(false);
        st.missing.replace(None);
        st.timeless.set(false);
        st.varies.set(false);
        st.random
            .set(noise::mix(&[u64::from(this), 0, time.to_bits()]));
        let global = self.context.global_object();
        let set = global.set(js_string!("time"), time, false, &mut self.context);
        let result = set.and_then(|_| self.programs[program.0].evaluate(&mut self.context));
        let value = match &result {
            Ok(value) => from_js(value, &mut self.context),
            Err(error) => Err(error.to_string()),
        };
        let _ = self.reset.evaluate(&mut self.context);
        let st = state(&self.context);
        if let Some(why) = st.missing.take() {
            return Err(match why {
                Missing::Pending(node) => Failure::Pending(node),
                Missing::Unknown(what) => Failure::Unknown(what),
                Missing::Unmeasured => Failure::Unmeasured,
            });
        }
        let (approximate, constant) = (st.approximate.get(), !st.varies.get());
        value
            .map(|value| Run {
                value,
                approximate,
                constant,
            })
            .map_err(Failure::Error)
    }
}

#[cfg(test)]
#[path = "../tests/unit/engine.rs"]
mod tests;
