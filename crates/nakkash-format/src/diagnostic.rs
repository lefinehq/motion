//! [`Diagnostic`], the one error type of every nakkash crate: what was wrong,
//! where ([`Place`]: `card/title.fill`, an [`Address`] in a scene) and which
//! forms are accepted, so an agent can fix its input in one try.
//!
//! ```
//! use nakkash_format::{Diagnostic, Kind};
//!
//! let d = Diagnostic::syntax("`12` is not a valid time")
//!     .expected("`<number>f` or `<number>s`")
//!     .at("in")
//!     .under("layers[1]");
//! assert_eq!(d.kind, Kind::Syntax);
//! assert_eq!(
//!     d.to_string(),
//!     "layers[1].in: `12` is not a valid time; expected `<number>f` or `<number>s`"
//! );
//! ```

use std::borrow::Cow;
use std::path::Path;
use std::{fmt, io};

use winnow::combinator::{alt, delimited, eof, preceded, repeat, terminated};
use winnow::prelude::*;

use crate::address::{Address, Id, Step};
use crate::grammar::Parsed;

/// What sort of problem a [`Diagnostic`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A shorthand string or an edit line that cannot be parsed.
    Syntax,
    /// A name that is not in its table: a key, preset, curve, property or id.
    UnknownName,
    /// A number outside the values its key accepts.
    OutOfRange,
    /// A required key or value that is absent.
    Missing,
    /// Two parts of the input that cannot both hold, such as a duplicate id.
    Conflict,
    /// A JSON value of the wrong type, such as a number for a string.
    WrongType,
    /// A rule the value breaks, such as the id rule.
    Invalid,
    /// A file that cannot be read or written.
    Io,
}

impl Kind {
    /// The exit code of the `nakkash` CLI for this kind of failure.
    ///
    /// The match names every kind, so a new kind has to choose its code here.
    pub const fn exit_code(self) -> u8 {
        match self {
            Kind::Syntax
            | Kind::UnknownName
            | Kind::OutOfRange
            | Kind::Missing
            | Kind::Conflict
            | Kind::WrongType
            | Kind::Invalid
            | Kind::Io => 2,
        }
    }
}

/// A problem with the input, shown as `{path}: {message}; expected {expected}`,
/// without the parts that are absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// What sort of problem this is; decides the CLI exit code.
    pub kind: Kind,
    /// Where the problem is, e.g. `card/title.in`; `None` for input without structure.
    pub path: Option<Place>,
    /// What was wrong, including the offending input.
    pub message: String,
    /// The accepted forms, written for people, e.g. ``"`12f` or `0.4s`"``.
    pub expected: Option<Cow<'static, str>>,
}

/// The result of every fallible operation in nakkash.
pub type Result<T> = std::result::Result<T, Diagnostic>;

impl Diagnostic {
    /// A diagnostic of `kind` with `message`, without a path or expected form.
    pub fn new(kind: Kind, message: impl Into<String>) -> Self {
        Self {
            kind,
            path: None,
            message: message.into(),
            expected: None,
        }
    }

    /// A [`Kind::Syntax`] diagnostic: input that cannot be parsed.
    pub fn syntax(message: impl Into<String>) -> Self {
        Self::new(Kind::Syntax, message)
    }

    /// A [`Kind::UnknownName`] diagnostic: ``unknown {what} `{name}`; expected one of a, b``.
    ///
    /// Pass `allowed` from the table that holds the names, so the list cannot
    /// drift; with none allowed, there is no list to expect.
    pub fn unknown(
        what: impl fmt::Display,
        name: impl fmt::Display,
        allowed: impl IntoIterator<Item = impl fmt::Display>,
    ) -> Self {
        let allowed: Vec<String> = allowed.into_iter().map(|a| a.to_string()).collect();
        let unknown = Self::new(Kind::UnknownName, format!("unknown {what} `{name}`"));
        match allowed.is_empty() {
            true => unknown,
            false => unknown.expected(format!("one of {}", allowed.join(", "))),
        }
    }

    /// A [`Kind::Io`] diagnostic: ``cannot {action} `{path}`: {err}``.
    ///
    /// The file goes in the message, since [`path`](Diagnostic::path) points into a document.
    pub fn io(action: &str, path: &Path, err: &io::Error) -> Self {
        Self::new(
            Kind::Io,
            format!("cannot {action} `{}`: {err}", path.display()),
        )
    }

    /// Sets the place of the offending input, replacing any place already set.
    #[must_use]
    pub fn at(mut self, place: impl Into<Place>) -> Self {
        self.path = Some(place.into());
        self
    }

    /// Places the diagnostic inside `outer`: `outer.path`, `outer[i]…`,
    /// `outer/layer…` or `outer`.
    ///
    /// An empty `outer` (the top level) keeps the place, so callers need no
    /// root case.
    #[must_use]
    pub fn under(mut self, outer: impl Into<Place>) -> Self {
        let outer = outer.into();
        self.path = match self.path.take() {
            None if outer.is_top() => None,
            None => Some(outer),
            Some(inner) => Some(outer.join(inner)),
        };
        self
    }

    /// The place, as it prints.
    pub fn path_text(&self) -> Option<String> {
        self.path.as_ref().map(ToString::to_string)
    }

    /// Sets the accepted forms, replacing any already set.
    #[must_use]
    pub fn expected(mut self, form: impl Into<Cow<'static, str>>) -> Self {
        self.expected = Some(form.into());
        self
    }

    /// Adds an accepted form after those already set, as `{expected}, or {form}`.
    #[must_use]
    pub fn or_expected(mut self, form: impl fmt::Display) -> Self {
        let joined = match self.expected.take() {
            Some(expected) => format!("{expected}, or {form}"),
            None => form.to_string(),
        };
        self.expected = Some(joined.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{path}: ")?;
        }
        f.write_str(&self.message)?;
        if let Some(expected) = &self.expected {
            write!(f, "; expected {expected}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// Where a [`Diagnostic`] points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// In a scene: a layer at any depth and keys inside it, or keys of the
    /// scene itself. Until it is put under a layer, it is relative to
    /// whatever holds it.
    Scene(Address),
    /// In another input with places of its own, as it names them: a
    /// project's chunks, a file.
    Other(String),
}

impl Place {
    /// The layer `id`, as a place to put a diagnostic under. An id that
    /// cannot name a layer (one that breaks the id rule, or `scene`) is the
    /// layer at `index` of its list instead.
    pub fn layer(id: &str, index: usize) -> Place {
        match naming(id) {
            Some(id) => Place::Scene(Address::layer([id])),
            None => Place::Scene(Address {
                layers: Vec::new(),
                keys: vec![Step::Key(crate::key::LAYERS.into()), Step::Index(index)],
            }),
        }
    }

    /// The layer `id` of a scene that passed validation, whose ids follow
    /// the id rule. An id that does not is kept as it is written.
    pub fn of_layer(id: &str) -> Place {
        match naming(id) {
            Some(id) => Place::Scene(Address::layer([id])),
            None => Place::Other(id.to_owned()),
        }
    }

    /// This place in the scene `raw`, with each `layers[i]` named by its
    /// layer's id where that id follows the id rule, as validation names
    /// layers: `layers[1].text` becomes `title.text`, and
    /// `comps.card.layers[0].text` becomes `comps.card.layers[title].text`.
    pub(crate) fn by_layer_ids(self, raw: &serde_json::Value) -> Place {
        let Place::Scene(address) = self else {
            return self;
        };
        let mut named = Address {
            layers: address.layers,
            keys: Vec::new(),
        };
        let mut node = Some(raw);
        let mut steps = address.keys.into_iter().peekable();
        while let Some(step) = steps.next() {
            let layer = match (&step, steps.peek()) {
                (Step::Key(key), Some(Step::Index(i))) if key == crate::key::LAYERS => {
                    let layer = node.and_then(|n| n.get(key)).and_then(|l| l.get(*i));
                    let id = layer
                        .and_then(|l| l.get(crate::key::ID))
                        .and_then(serde_json::Value::as_str)
                        .and_then(naming);
                    id.map(|id| (id, layer))
                }
                _ => None,
            };
            match layer {
                Some((id, layer)) => {
                    steps.next();
                    node = layer;
                    match named.keys.is_empty() {
                        true => named.layers.push(id),
                        false => named
                            .keys
                            .extend([Step::Key(crate::key::LAYERS.into()), Step::Id(id)]),
                    }
                }
                None => {
                    node = node.and_then(|n| match &step {
                        Step::Key(key) => n.get(key),
                        Step::Index(i) => n.get(*i),
                        Step::Id(_) => None,
                    });
                    named.keys.push(step);
                }
            }
        }
        Place::Scene(named)
    }

    /// A place in an input that is not a scene, as it names it.
    pub fn other(text: impl Into<String>) -> Place {
        Place::Other(text.into())
    }

    /// Whether it is the top level itself: a place in a scene with no layer
    /// and no key.
    fn is_top(&self) -> bool {
        matches!(self, Place::Scene(a) if a.layers.is_empty() && a.keys.is_empty())
    }

    /// The address, for a place in a scene.
    pub fn address(&self) -> Option<&Address> {
        match self {
            Place::Scene(address) => Some(address),
            Place::Other(_) => None,
        }
    }

    /// `inner` inside `self`. Layers put under keys are the elements of
    /// those keys' `layers` list, as in a composition's own definition.
    pub fn join(self, inner: Place) -> Place {
        match (self, inner) {
            (Place::Scene(outer), Place::Scene(inner)) if outer.keys.is_empty() => {
                Place::Scene(Address {
                    layers: outer.layers.into_iter().chain(inner.layers).collect(),
                    keys: inner.keys,
                })
            }
            (Place::Scene(mut outer), Place::Scene(inner)) => {
                let listed = inner
                    .layers
                    .into_iter()
                    .flat_map(|id| [Step::Key(crate::key::LAYERS.into()), Step::Id(id)]);
                outer.keys.extend(listed.chain(inner.keys));
                Place::Scene(outer)
            }
            (outer, inner) => {
                let (outer, inner) = (outer.to_string(), inner.to_string());
                match (outer.is_empty(), inner.starts_with('[')) {
                    (true, _) => Place::Other(inner),
                    (false, true) => Place::Other(format!("{outer}{inner}")),
                    (false, false) => Place::Other(format!("{outer}.{inner}")),
                }
            }
        }
    }
}

/// A place in a scene printed relative to what holds it: its layers joined
/// by `/`, then its keys, the first without a `.` when there is no layer.
impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let address = match self {
            Place::Other(text) => return f.write_str(text),
            Place::Scene(address) if !address.is_scene() => return write!(f, "{address}"),
            Place::Scene(address) => address,
        };
        f.write_str(&crate::address::key_text(&address.keys))
    }
}

impl From<Address> for Place {
    fn from(address: Address) -> Place {
        Place::Scene(address)
    }
}

/// A path of keys, such as `anim.x[2]` or `runs[0].range`, read as a place
/// in a scene; the empty path is the top level. Text that is not a path of
/// keys is kept as it is written.
impl From<&str> for Place {
    fn from(text: &str) -> Place {
        match terminated(relative_keys::<()>, eof).parse(text) {
            Ok(keys) => Place::Scene(Address {
                layers: Vec::new(),
                keys,
            }),
            Err(_) => Place::Other(text.to_owned()),
        }
    }
}

impl From<String> for Place {
    fn from(text: String) -> Place {
        Place::from(text.as_str())
    }
}

impl From<&String> for Place {
    fn from(text: &String) -> Place {
        Place::from(text.as_str())
    }
}

/// The id that names a layer in an address: one that follows the id rule
/// and is not `scene`.
fn naming(id: &str) -> Option<Id> {
    id.parse::<Id>()
        .ok()
        .filter(|id| id.as_ref() != crate::address::SCENE)
}

/// Keys with no leading `.`: a key or a selector, then `.key`s and
/// selectors; or nothing.
fn relative_keys<R>(input: &mut &str) -> Parsed<Vec<Step>, R> {
    let selector = || {
        delimited(
            '[',
            alt((
                winnow::ascii::dec_uint.map(Step::Index),
                crate::address::id.map(|id: &str| Step::Id(id.parse().expect("an id"))),
            )),
            ']',
        )
    };
    let key = || crate::address::key.map(|k: &str| Step::Key(k.to_owned()));
    let first = alt((key(), selector()));
    let rest = repeat(0.., alt((selector(), preceded('.', key())))).fold(
        Vec::new,
        |mut v: Vec<Step>, s| {
            v.push(s);
            v
        },
    );
    winnow::combinator::opt((first, rest))
        .map(|steps| match steps {
            None => Vec::new(),
            Some((first, mut rest)) => {
                rest.insert(0, first);
                rest
            }
        })
        .parse_next(input)
}

#[cfg(test)]
#[path = "../tests/unit/diagnostic.rs"]
mod tests;
