//! Ids and addresses ([D43]): the one way to name a layer anywhere in a
//! scene, and a value inside it.
//!
//! - An [`Id`] follows the id rule: lower-case letters, digits, `-` and
//!   `_`, starting with a letter.
//! - An [`Address`] is the ids from the scene down through the comp layers
//!   that show a layer, joined by `/`, then its keys, each after a `.`, with
//!   a list element chosen by `[id]` or `[index]`:
//!   `card/title.fx[glow].radius`. The scene itself is `scene`, which no
//!   layer may take as its id.
//!
//! [D43]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d43-one-layer-address-and-edits-per-instance

use std::fmt;
use std::str::FromStr;

use derive_more::{AsRef, Deref, Display};
use winnow::ascii::dec_uint;
use winnow::combinator::{alt, delimited, eof, not, opt, preceded, repeat, separated, terminated};
use winnow::prelude::*;
use winnow::token::{one_of, take_while};

use crate::doc::Form;
use crate::grammar::{Parsed, parse_whole};
use crate::{Diagnostic, Kind, Result};

/// The id rule, as an accepted form.
pub const ID_RULE: &str = "ids use lowercase letters, digits, `-` and `_`, starting with a letter";

/// The accepted form of an address.
const EXPECTED: &str = "`scene` or layer ids joined by `/`, then `.key`s, each with an optional `[id]` or `[index]`, e.g. `card/title.fx[glow].radius`";

/// The name of the scene itself in an address, which no layer may take.
pub const SCENE: &str = "scene";

/// An id: of a layer, a composition, a preset or an item.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Display, AsRef, Deref)]
#[as_ref(str)]
#[deref(forward)]
pub struct Id(String);

impl Id {
    /// Whether `text` follows the id rule.
    pub fn is_valid(text: &str) -> bool {
        terminated(id::<()>, eof).parse(text).is_ok()
    }
}

impl FromStr for Id {
    type Err = Diagnostic;

    fn from_str(text: &str) -> Result<Id> {
        match Id::is_valid(text) {
            true => Ok(Id(text.to_owned())),
            false => Err(
                Diagnostic::new(Kind::Invalid, format!("invalid id `{text}`")).expected(ID_RULE),
            ),
        }
    }
}

impl From<Id> for String {
    fn from(id: Id) -> String {
        id.0
    }
}

/// One step of a path inside a layer: a key of an object, or the element of
/// a list chosen by its id or by its index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// A key of an object, such as `fx` or `radius`.
    Key(String),
    /// The element of a list whose `id` this is.
    Id(Id),
    /// The element of a list at this index.
    Index(usize),
}

/// A place in a scene: a layer at any depth, or the scene itself, and a
/// path of keys inside it.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address {
    /// The ids from the scene down to the layer; none for the scene itself.
    pub layers: Vec<Id>,
    /// The keys inside it, in order.
    pub keys: Vec<Step>,
}

impl Address {
    /// The scene itself.
    pub fn scene() -> Address {
        Address::default()
    }

    /// The layer reached through `layers`.
    pub fn layer(layers: impl IntoIterator<Item = Id>) -> Address {
        Address {
            layers: layers.into_iter().collect(),
            keys: Vec::new(),
        }
    }

    /// Whether it names the scene rather than a layer.
    pub fn is_scene(&self) -> bool {
        self.layers.is_empty()
    }

    /// This address with `step` after its keys.
    #[must_use]
    pub fn then(mut self, step: Step) -> Address {
        self.keys.push(step);
        self
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.layers.split_first() {
            None => f.write_str(SCENE)?,
            Some((first, rest)) => {
                write!(f, "{first}")?;
                rest.iter().try_for_each(|id| write!(f, "/{id}"))?;
            }
        }
        self.keys.iter().try_for_each(|step| match step {
            Step::Key(key) => write!(f, ".{key}"),
            Step::Id(id) => write!(f, "[{id}]"),
            Step::Index(index) => write!(f, "[{index}]"),
        })
    }
}

impl FromStr for Address {
    type Err = Diagnostic;

    fn from_str(text: &str) -> Result<Address> {
        parse_whole(text, address, "address", EXPECTED)
    }
}

impl Form for Address {
    const FORM: &str = EXPECTED;
}

/// An id: a lower-case letter, then lower-case letters, digits, `-` and `_`.
pub fn id<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    (
        one_of(|c: char| c.is_ascii_lowercase()),
        take_while(0.., |c: char| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
        }),
    )
        .take()
        .parse_next(input)
}

/// The layer part of an address: `scene`, or layer ids joined by `/`. A
/// layer path never starts with `scene`.
pub fn target<R>(input: &mut &str) -> Parsed<Vec<Id>, R> {
    let ids = separated(1.., id.map(|id: &str| Id(id.to_owned())), '/');
    alt((
        terminated(SCENE, scene_ends).value(Vec::new()),
        ids.verify(|ids: &Vec<Id>| ids[0].0 != SCENE),
    ))
    .parse_next(input)
}

/// Succeeds where `scene` is the whole target: not followed by more of an
/// id, nor by a `/`.
fn scene_ends<R>(input: &mut &str) -> Parsed<(), R> {
    not(one_of(|c: char| {
        c == '/' || c.is_ascii_alphanumeric() || c == '-' || c == '_'
    }))
    .parse_next(input)
}

/// The keys of an address: each `.` and a key, with an optional `[id]` or
/// `[index]` after it.
pub fn keys<R>(input: &mut &str) -> Parsed<Vec<Step>, R> {
    repeat(0.., preceded('.', step))
        .fold(Vec::new, |mut steps: Vec<Step>, (key, selector)| {
            steps.push(key);
            steps.extend(selector);
            steps
        })
        .parse_next(input)
}

/// A path of keys on its own, as an edit writes it after its target: a
/// key, then [`keys`]: `fx[glow].radius`.
pub fn key_path<R>(input: &mut &str) -> Parsed<Vec<Step>, R> {
    (step, keys)
        .map(|((key, selector), rest)| {
            let mut steps = vec![key];
            steps.extend(selector);
            steps.extend(rest);
            steps
        })
        .parse_next(input)
}

/// One key and the optional `[id]` or `[index]` after it.
fn step<R>(input: &mut &str) -> Parsed<(Step, Option<Step>), R> {
    let selector = delimited(
        '[',
        alt((
            dec_uint.map(Step::Index),
            id.map(|id: &str| Step::Id(Id(id.to_owned()))),
        )),
        ']',
    );
    (key.map(|k: &str| Step::Key(k.to_owned())), opt(selector)).parse_next(input)
}

/// A key: letters, digits, `-` and `_`.
pub(crate) fn key<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_while(1.., |c: char| {
        c.is_ascii_alphanumeric() || c == '-' || c == '_'
    })
    .parse_next(input)
}

/// A whole address.
fn address<R>(input: &mut &str) -> Parsed<Address, R> {
    terminated((target, keys), eof)
        .map(|(layers, keys)| Address { layers, keys })
        .parse_next(input)
}

#[cfg(test)]
#[path = "../tests/unit/address.rs"]
mod tests;
