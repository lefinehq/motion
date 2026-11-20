//! Slots ([D44]): named values a scene or a composition declares, which any
//! value follows by being written `"$id"`.
//!
//! References are resolved on a copy of the document's JSON before its
//! values are read, so every reader after this one sees ordinary values,
//! and the document keeps the references as written. A reference names a
//! slot of the composition it is written in, then one of the scene. A string
//! that starts with `$$` stands for itself with one `$` fewer.
//!
//! [D44]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d44-slots-are-named-values-that-layers-follow

use std::borrow::Cow;
use std::fmt;

use serde_json::Value as Json;
use strum::VariantNames;

use crate::oklch::Shift;
use crate::parse::key::{BG, COMPS, DUR, ID, LAYERS, SLOTS, comp_path};
use crate::{Address, Diagnostic, Id, Kind, Place, Result, Step};

/// What kind of value a slot holds, which the values following it take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotType {
    /// A number, such as an opacity or a speed.
    Number,
    /// Two numbers, `[x, y]`.
    Point,
    /// A colour, `#RRGGBB`.
    Color,
    /// Words, for a text layer.
    Text,
    /// A picture or vector footage file, for an image layer, or `null` for
    /// none yet ([D77]).
    ///
    /// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
    Image,
    /// A clip, for a video layer, or `null` for none yet ([D77]).
    ///
    /// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
    Video,
    /// A word from a closed list, `values`, which any string of the scene
    /// may follow, a `comp` key included, so a section can be chosen
    /// ([D91]).
    ///
    /// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
    Choice,
}

impl SlotType {
    /// Every type, by the name a declaration writes.
    pub const NAMES: [(&str, SlotType); 7] = [
        ("number", SlotType::Number),
        ("point", SlotType::Point),
        ("color", SlotType::Color),
        ("text", SlotType::Text),
        ("image", SlotType::Image),
        ("video", SlotType::Video),
        ("choice", SlotType::Choice),
    ];

    /// Whether it holds a file: a picture or a clip.
    pub fn is_file(self) -> bool {
        matches!(self, SlotType::Image | SlotType::Video)
    }

    /// The type named `name`.
    pub fn named(name: &str) -> Option<SlotType> {
        SlotType::NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, ty)| ty)
    }

    /// Its name, as a declaration writes it.
    pub fn name(self) -> &'static str {
        SlotType::NAMES
            .iter()
            .find(|&&(_, ty)| ty == self)
            .map_or("", |(n, _)| n)
    }
}

impl fmt::Display for SlotType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One slot as declared.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    /// Its id, which references write after `$`.
    pub id: Id,
    /// What it holds.
    pub ty: SlotType,
    /// Its value, as the values following it are written.
    pub value: Json,
    /// A name for people, such as the control a template exposed.
    pub label: Option<String>,
    /// What it is for, such as `title`, `accent` or `logo` ([D77]).
    ///
    /// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
    pub role: Option<Id>,
    /// Whether changing it was last seen to change the render: `false` when
    /// `nakkash slots --verify --mark` found it changes nothing, so a
    /// variant leaves it as it is ([D77]). Most often what follows it is
    /// drawn under something or not drawn yet, a gap a render fix closes.
    ///
    /// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
    pub seen: bool,
    /// What it was made from, which a later import of the same project
    /// gives again, so an instance can find it under another id ([D91]).
    ///
    /// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
    pub uid: Option<String>,
    /// What an instance may give it, and what `check` measures it against.
    pub contract: Contract,
}

/// How a text that does not fit its `fit` is treated ([D91]).
///
/// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display, strum::VariantNames,
)]
#[strum(serialize_all = "lowercase")]
pub enum Overflow {
    /// Set smaller, down to `min-size`.
    Shrink,
    /// Broken into more lines, up to `max-lines`, then set smaller.
    Wrap,
    /// Refused: a text longer than the size it was laid out for is an error.
    Reject,
}

/// What a file slot stands for ([D91]).
///
/// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display, strum::VariantNames,
)]
#[strum(serialize_all = "lowercase")]
pub enum MediaKind {
    /// A product's screen: an app, a page, a dashboard.
    Screen,
    /// A brand's mark.
    Logo,
    /// A picture.
    Image,
    /// A clip.
    Video,
    /// A grain, a leak, an overlay: drawn over or under the design.
    Texture,
}

/// What a slot declares beyond its value: the bounds an instance's value
/// is checked against, and what the brief says of it ([D91]). Every field
/// is optional; each belongs to the types it names.
///
/// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contract {
    /// Other ids an instance may name it by, such as an id it had before.
    pub aliases: Vec<String>,
    /// A number's least value.
    pub min: Option<f64>,
    /// A number's greatest value.
    pub max: Option<f64>,
    /// A choice's words, in order; its value is one of them.
    pub values: Vec<String>,
    /// A text's treatment when it does not fit.
    pub overflow: Option<Overflow>,
    /// The smallest a text may be set, in px, when it shrinks.
    pub min_size: Option<f64>,
    /// The most lines a text may break into.
    pub max_lines: Option<u32>,
    /// The colour slot a colour is read against.
    pub pair: Option<Id>,
    /// The least contrast ratio between a colour and its pair, from 1 to 21.
    pub contrast: Option<f64>,
    /// What a file slot stands for.
    pub kind: Option<MediaKind>,
    /// What a file slot takes besides its own type: a picture slot that
    /// accepts `video` plays a clip in the same box.
    pub accepts: Vec<SlotType>,
    /// The least size of a file, in px across and down.
    pub min_px: Option<[f64; 2]>,
    /// The proportions a file is shown at, `width:height`.
    pub aspect: Option<String>,
}

/// The keys of a declaration that are not its type.
const CONTRACT_KEYS: [&str; 17] = [
    LABEL, ROLE, SEEN, UID, ALIASES, VALUES, MIN, MAX, OVERFLOW, MIN_SIZE, MAX_LINES, PAIR,
    CONTRAST, KIND, ACCEPTS, MIN_PX, ASPECT,
];
const UID: &str = "uid";
const ALIASES: &str = "aliases";
const VALUES: &str = "values";
const MIN: &str = "min";
const MAX: &str = "max";
const OVERFLOW: &str = "overflow";
const MIN_SIZE: &str = "min-size";
const MAX_LINES: &str = "max-lines";
const PAIR: &str = "pair";
const CONTRAST: &str = "contrast";
const KIND: &str = "kind";
const ACCEPTS: &str = "accepts";
const MIN_PX: &str = "min-px";
const ASPECT: &str = "aspect";

impl Contract {
    /// The contract of a declaration's `fields`, for a slot of type `ty`.
    fn read(fields: &serde_json::Map<String, Json>, ty: SlotType) -> Result<Contract> {
        let bad = |key: &str, form: &str| {
            Diagnostic::new(Kind::Invalid, format!("`{}` for `{key}`", fields[key]))
                .expected(form.to_owned())
                .under(key.to_owned())
        };
        let only = |key: &str, types: &[SlotType]| -> Result<()> {
            match fields.contains_key(key) && !types.contains(&ty) {
                true => Err(
                    Diagnostic::new(Kind::Invalid, format!("`{key}` on a {ty} slot")).expected(
                        format!(
                            "`{key}` on a {} slot",
                            types
                                .iter()
                                .map(|t| t.name())
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ),
                    ),
                ),
                false => Ok(()),
            }
        };
        let words = |key: &str| -> Result<Vec<String>> {
            match fields.get(key) {
                None => Ok(Vec::new()),
                Some(Json::Array(items)) => items
                    .iter()
                    .map(|i| {
                        i.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| bad(key, "a list of words"))
                    })
                    .collect(),
                Some(_) => Err(bad(key, "a list of words")),
            }
        };
        let number = |key: &str, least: f64| -> Result<Option<f64>> {
            match fields.get(key) {
                None => Ok(None),
                Some(Json::Number(n)) => match n.as_f64() {
                    Some(n) if n >= least && n.is_finite() => Ok(Some(n)),
                    _ => Err(bad(key, &format!("a number of at least {least}"))),
                },
                Some(_) => Err(bad(key, "a number")),
            }
        };
        let named = |key: &str, choices: &[&str]| -> Result<Option<String>> {
            match fields.get(key) {
                None => Ok(None),
                Some(Json::String(word)) if choices.contains(&word.as_str()) => {
                    Ok(Some(word.clone()))
                }
                Some(_) => Err(bad(key, &format!("one of {}", choices.join(", ")))),
            }
        };
        use SlotType::{Choice, Color, Image, Number, Text, Video};
        only(MIN, &[Number])?;
        only(MAX, &[Number])?;
        only(VALUES, &[Choice])?;
        only(OVERFLOW, &[Text])?;
        only(MIN_SIZE, &[Text])?;
        only(MAX_LINES, &[Text])?;
        only(PAIR, &[Color])?;
        only(CONTRAST, &[Color])?;
        for key in [KIND, ACCEPTS, MIN_PX, ASPECT] {
            only(key, &[Image, Video])?;
        }
        let min = number(MIN, f64::MIN)?;
        let max = number(MAX, f64::MIN)?;
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err(bad(MAX, &format!("a number of at least `min`, {min}")));
        }
        let values = words(VALUES)?;
        if ty == Choice && values.is_empty() {
            return Err(Diagnostic::new(Kind::Missing, "a choice with no `values`")
                .expected(r#"{"choice":"centred","values":["centred","left"]}"#));
        }
        let max_lines = match number(MAX_LINES, 1.0)? {
            Some(n) if n.fract() != 0.0 => {
                return Err(bad(MAX_LINES, "a whole number of at least 1"));
            }
            n => n.map(|n| n as u32),
        };
        let pair = match fields.get(PAIR) {
            None => None,
            Some(Json::String(id)) => Some(id.parse::<Id>().map_err(|e| e.under(PAIR))?),
            Some(_) => return Err(bad(PAIR, "the id of a colour slot")),
        };
        let contrast = number(CONTRAST, 1.0)?;
        if contrast.is_some_and(|c| c > 21.0) {
            return Err(bad(CONTRAST, "a contrast ratio from 1 to 21"));
        }
        let accepts = words(ACCEPTS)?
            .iter()
            .map(|name| match SlotType::named(name) {
                Some(t) if t.is_file() => Ok(t),
                _ => Err(bad(ACCEPTS, "a list of `image` and `video`")),
            })
            .collect::<Result<Vec<_>>>()?;
        let min_px = match fields.get(MIN_PX) {
            None => None,
            Some(Json::Array(xy))
                if xy.len() == 2 && xy.iter().all(|n| n.as_f64().is_some_and(|n| n > 0.0)) =>
            {
                Some([xy[0].as_f64().unwrap_or(0.0), xy[1].as_f64().unwrap_or(0.0)])
            }
            Some(_) => return Err(bad(MIN_PX, "[width, height] in px, above zero")),
        };
        let aspect = match fields.get(ASPECT) {
            None => None,
            Some(Json::String(text)) if aspect_ratio(text).is_some() => Some(text.clone()),
            Some(_) => return Err(bad(ASPECT, "`width:height`, such as `16:9`")),
        };
        Ok(Contract {
            aliases: words(ALIASES)?,
            min,
            max,
            values,
            overflow: named(OVERFLOW, Overflow::VARIANTS)?.and_then(|w| w.parse().ok()),
            min_size: number(MIN_SIZE, 0.0)?,
            max_lines,
            pair,
            contrast,
            kind: named(KIND, MediaKind::VARIANTS)?.and_then(|w| w.parse().ok()),
            accepts,
            min_px,
            aspect,
        })
    }
}

/// The proportions `text` writes, `width:height`, as width over height.
pub fn aspect_ratio(text: &str) -> Option<f64> {
    let (w, h) = text.split_once(':')?;
    let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
    (w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()).then_some(w / h)
}

/// The slots of a scene: its own, and each composition's by id.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Declared {
    pub(crate) scene: Vec<Slot>,
    pub(crate) comps: Vec<(String, Vec<Slot>)>,
}

impl Declared {
    /// The slots composition `id` declares.
    pub(crate) fn of_comp(&self, id: &str) -> &[Slot] {
        self.comps
            .iter()
            .find(|(comp, _)| comp == id)
            .map_or(&[], |(_, slots)| slots)
    }
}

/// The written form of a slot's declaration.
const FORM: &str = r##"a colour, a number, [x, y], {"text":"…"}, {"image":"…"}, {"video":"…"} or {"choice":"…","values":[…]}, optionally typed, labelled, given a role, bounds and a uid, and marked unseen, e.g. {"color":"#E63946","label":"Accent","role":"accent"}"##;

/// The slots `raw`, a scene's JSON, declares on itself and on its
/// compositions.
///
/// # Errors
/// An invalid-value diagnostic at the slot whose id or value does not read.
pub(crate) fn declared(raw: &Json) -> Result<Declared> {
    let scene = slots_of(raw).map_err(|e| e.under(SLOTS))?;
    let comps = match raw.get(COMPS).and_then(Json::as_object) {
        Some(comps) => comps
            .iter()
            .map(|(id, comp)| {
                let slots = slots_of(comp).map_err(|e| e.under(SLOTS).under(comp_path(id)))?;
                Ok((id.clone(), slots))
            })
            .collect::<Result<_>>()?,
        None => Vec::new(),
    };
    Ok(Declared { scene, comps })
}

/// The slots one scene or composition object declares.
fn slots_of(owner: &Json) -> Result<Vec<Slot>> {
    let Some(written) = owner.get(SLOTS) else {
        return Ok(Vec::new());
    };
    let Some(written) = written.as_object() else {
        return Err(
            Diagnostic::new(Kind::Invalid, "slots that are not an object")
                .expected(r##"an object of slots by id, e.g. {"accent":"#E63946"}"##),
        );
    };
    written
        .iter()
        .map(|(id, value)| {
            slot(id, value).map_err(|e| {
                e.at(Place::Scene(Address {
                    layers: Vec::new(),
                    keys: vec![Step::Key(id.clone())],
                }))
            })
        })
        .collect()
}

/// One slot, `id` declared as `written`.
fn slot(id: &str, written: &Json) -> Result<Slot> {
    let id: Id = id.parse()?;
    let (ty, value, label, role, seen, uid, contract) = match written {
        Json::Object(fields) => {
            let label = match fields.get(LABEL) {
                None => None,
                Some(Json::String(label)) => Some(label.clone()),
                Some(_) => return Err(invalid(written)),
            };
            let role = match fields.get(ROLE) {
                None => None,
                Some(Json::String(role)) => Some(role.parse::<Id>().map_err(|e| e.under(ROLE))?),
                Some(_) => return Err(invalid(written)),
            };
            let seen = match fields.get(SEEN) {
                None => true,
                Some(Json::Bool(seen)) => *seen,
                Some(_) => return Err(invalid(written)),
            };
            let uid = match fields.get(UID) {
                None => None,
                Some(Json::String(uid)) if !uid.is_empty() => Some(uid.clone()),
                Some(_) => return Err(invalid(written)),
            };
            let mut typed = fields
                .iter()
                .filter(|(name, _)| !CONTRACT_KEYS.contains(&name.as_str()));
            let (Some((name, value)), None) = (typed.next(), typed.next()) else {
                return Err(invalid(written));
            };
            let ty = SlotType::named(name).ok_or_else(|| {
                Diagnostic::unknown("slot type", name, SlotType::NAMES.map(|(n, _)| n))
            })?;
            let contract = Contract::read(fields, ty)?;
            (ty, value.clone(), label, role, seen, uid, contract)
        }
        Json::Number(_) => plain(SlotType::Number, written),
        // Two numbers are a point; three or four, a colour's channels.
        Json::Array(items) if items.len() == 2 => plain(SlotType::Point, written),
        Json::Array(_) => plain(SlotType::Color, written),
        Json::String(_) => plain(SlotType::Color, written),
        _ => return Err(invalid(written)),
    };
    check_fits(ty, &value).map_err(|d| d.expected(FORM))?;
    let slot = Slot {
        id,
        ty,
        value,
        label,
        role,
        seen,
        uid,
        contract,
    };
    slot.within_bounds(&slot.value)?;
    Ok(slot)
}

/// A slot of type `ty` written as its bare value.
type Read = (
    SlotType,
    Json,
    Option<String>,
    Option<Id>,
    bool,
    Option<String>,
    Contract,
);
fn plain(ty: SlotType, written: &Json) -> Read {
    (
        ty,
        written.clone(),
        None,
        None,
        true,
        None,
        Contract::default(),
    )
}

impl Slot {
    /// `Ok` when `value` is a value of the slot's type, inside its bounds.
    ///
    /// # Errors
    /// An invalid diagnostic for a value of another type, else as
    /// [`Slot::within_bounds`].
    pub fn fits(&self, value: &Json) -> Result<()> {
        check_fits(self.ty, value).and_then(|()| self.within_bounds(value))
    }

    /// `Ok` when `value`, a value of the slot's type, keeps inside the
    /// bounds the slot declares: a number between `min` and `max`, a choice
    /// one of its `values`.
    ///
    /// # Errors
    /// An out-of-range diagnostic naming the bound, or an unknown-name one
    /// listing the choices.
    pub fn within_bounds(&self, value: &Json) -> Result<()> {
        let c = &self.contract;
        if let Some(n) = value.as_f64() {
            if let Some(min) = c.min.filter(|&min| n < min) {
                return Err(
                    Diagnostic::new(Kind::OutOfRange, format!("{n} is below {min}"))
                        .expected(format!("a number from {min}")),
                );
            }
            if let Some(max) = c.max.filter(|&max| n > max) {
                return Err(
                    Diagnostic::new(Kind::OutOfRange, format!("{n} is above {max}"))
                        .expected(format!("a number up to {max}")),
                );
            }
        }
        if self.ty == SlotType::Choice
            && let Some(word) = value.as_str()
            && !c.values.iter().any(|v| v == word)
        {
            return Err(Diagnostic::unknown("choice", word, &c.values));
        }
        Ok(())
    }
}

/// The key of a slot's label.
const LABEL: &str = "label";
/// The key of a slot's role.
const ROLE: &str = "role";
/// The key that says a slot was seen to change nothing.
pub const SEEN: &str = "seen";

/// `Ok` when `value` is a value of type `ty`.
fn check_fits(ty: SlotType, value: &Json) -> Result<()> {
    let fits = match (ty, value) {
        (SlotType::Number, Json::Number(_))
        | (SlotType::Text | SlotType::Choice, Json::String(_)) => true,
        (SlotType::Point, Json::Array(xy)) => xy.len() == 2 && xy.iter().all(Json::is_number),
        (SlotType::Color, Json::String(_) | Json::Array(_)) => crate::text::color(value).is_ok(),
        (SlotType::Image | SlotType::Video, Json::String(path)) => !path.is_empty(),
        (SlotType::Image | SlotType::Video, Json::Null) => true,
        _ => false,
    };
    match fits {
        true => Ok(()),
        false => Err(Diagnostic::new(
            Kind::Invalid,
            format!("`{value}` for a {ty} slot"),
        )),
    }
}

/// The slot `key` names among `declared`: `id` for one of the scene's, or
/// `comp/id` for one a composition declares ([D91]).
///
/// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
pub(crate) fn named<'d>(declared: &'d Declared, key: &str) -> Option<(Option<&'d str>, &'d Slot)> {
    match key.split_once('/') {
        None => declared
            .scene
            .iter()
            .find(|s| &*s.id == key)
            .map(|s| (None, s)),
        Some((comp, id)) => {
            declared
                .comps
                .iter()
                .find(|(c, _)| c == comp)
                .and_then(|(c, slots)| {
                    slots
                        .iter()
                        .find(|s| &*s.id == id)
                        .map(|s| (Some(c.as_str()), s))
                })
        }
    }
}

/// Checks a scene's `looks` ([D91]): named sets of slot values, each value
/// naming a slot of the scene, `id`, or of a composition, `comp/id`, and
/// fitting it.
///
/// # Errors
/// An invalid diagnostic for looks that are not objects of objects, an
/// unknown-name one for a look's name or slot that does not read, and the
/// slot's own diagnostic for a value that does not fit it, each at
/// `looks.<name>.<slot>`.
///
/// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
pub(crate) fn read_looks(written: Option<&Json>, declared: &Declared) -> Result<()> {
    use crate::parse::key::LOOKS;
    let Some(written) = written else {
        return Ok(());
    };
    let form = r##"an object of looks by name, each an object of slot values, e.g. {"dark":{"background":"#0B0D12","text":"#FFFFFF"}}"##;
    let place = |keys: Vec<Step>| {
        Place::Scene(Address {
            layers: Vec::new(),
            keys,
        })
    };
    let Some(looks) = written.as_object() else {
        return Err(
            Diagnostic::new(Kind::Invalid, "looks that are not an object")
                .expected(form)
                .at(place(vec![Step::Key(LOOKS.into())])),
        );
    };
    for (name, values) in looks {
        let at = |more: &[&str]| {
            let mut keys = vec![Step::Key(LOOKS.into()), Step::Key(name.clone())];
            keys.extend(more.iter().map(|k| Step::Key((*k).to_owned())));
            place(keys)
        };
        name.parse::<Id>().map_err(|d| d.at(at(&[])))?;
        let Some(values) = values.as_object() else {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("the look `{name}` is not an object"),
            )
            .expected(form)
            .at(at(&[])));
        };
        for (key, value) in values {
            let Some((_, slot)) = named(declared, key) else {
                let known = declared.scene.iter().map(|s| s.id.to_string()).chain(
                    declared
                        .comps
                        .iter()
                        .flat_map(|(c, slots)| slots.iter().map(move |s| format!("{c}/{}", s.id))),
                );
                return Err(Diagnostic::unknown("slot", key, known).at(at(&[key])));
            };
            check_fits(slot.ty, value)
                .and_then(|()| slot.within_bounds(value))
                .map_err(|d| {
                    d.expected(format!("a {}, like `{}`", slot.ty, slot.value))
                        .at(at(&[key]))
                })?;
        }
    }
    Ok(())
}

/// The slots a composition `declared`, as one instance of it gives them:
/// `entries`, the `$id` keys of its `overrides` with their values.
///
/// # Errors
/// An unknown-name diagnostic for an id the composition does not declare,
/// or an invalid-value one for a value that does not fit its slot, each at
/// the entry.
pub(crate) fn given<'g>(
    declared: &[Slot],
    entries: impl IntoIterator<Item = (&'g str, &'g Json)>,
) -> Result<Vec<Slot>> {
    entries
        .into_iter()
        .map(|(id, value)| {
            let at = Place::Scene(Address {
                layers: Vec::new(),
                keys: vec![Step::Key(format!("${id}"))],
            });
            let Some(slot) = declared.iter().find(|s| &*s.id == id) else {
                let known = declared.iter().map(|s| format!("${}", s.id));
                let unknown = Diagnostic::unknown("slot", format!("${id}"), known);
                let unknown = match declared.is_empty() {
                    true => unknown.expected("a slot of the composition, which declares none"),
                    false => unknown,
                };
                return Err(unknown.at(at));
            };
            check_fits(slot.ty, value)
                .and_then(|()| slot.within_bounds(value))
                .map_err(|d| {
                    d.expected(format!("a {}, like `{}`", slot.ty, slot.value))
                        .at(at)
                })?;
            Ok(Slot {
                value: value.clone(),
                ..slot.clone()
            })
        })
        .collect()
}

fn invalid(written: &Json) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("the slot `{written}`")).expected(FORM)
}

/// What a string value says.
enum Written<'a> {
    /// `$id`: the value of slot `id`.
    Reference(&'a str),
    /// `$id l-0.02 / 0.5`: the colour of slot `id`, changed ([D77]).
    ///
    /// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
    Changed(&'a str, Shift),
    /// `$$…`: itself, one `$` fewer.
    Escaped(&'a str),
    /// Anything else, itself.
    Plain,
}

fn written(text: &str) -> Written<'_> {
    if let Some(rest) = text.strip_prefix("$$") {
        return Written::Escaped(rest);
    }
    let Some(rest) = text.strip_prefix('$') else {
        return Written::Plain;
    };
    if Id::is_valid(rest) {
        return Written::Reference(rest);
    }
    // A change follows the id after a space; anything else, as `$99 off`,
    // is plain text.
    match rest.split_once(' ') {
        Some((id, change)) if Id::is_valid(id) => match Shift::parse(change) {
            Some(shift) => Written::Changed(id, shift),
            None => Written::Plain,
        },
        _ => Written::Plain,
    }
}

/// Whether the format reads `text` as other than itself: a reference to a
/// slot, a changed colour of one, or an escape, `$$`. An import escapes such
/// a string the project holds, `$$title`, so it stays the text it was.
pub fn is_reference(text: &str) -> bool {
    !matches!(written(text), Written::Plain)
}

/// Where a reference was replaced by its slot's value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Followed {
    /// The value's place, as validation names it.
    pub(crate) place: String,
    /// The slot it follows.
    pub(crate) slot: Id,
    /// The composition that declares the slot; none for the scene.
    pub(crate) comp: Option<String>,
}

/// Every reference followed while resolving, so a value that does not fit
/// can name the slot it came from.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Resolved {
    pub(crate) followed: Vec<Followed>,
}

impl Resolved {
    /// `diagnostic`, naming the slot when it is at a value that followed one.
    pub(crate) fn explain(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        let Some(path) = diagnostic.path_text() else {
            return diagnostic;
        };
        if let Some(followed) = self.followed.iter().find(|f| f.place == path) {
            diagnostic.message = format!(
                "{} (the value of slot `${}`)",
                diagnostic.message, followed.slot
            );
        }
        diagnostic
    }
}

/// `raw`, a scene's JSON, with every reference in its layers replaced by its
/// slot's value and every `$$` unescaped; `raw` itself when it has neither.
///
/// # Errors
/// An unknown-name diagnostic at a reference to no slot in scope.
pub(crate) fn resolve<'j>(raw: &'j Json, declared: &Declared) -> Result<(Cow<'j, Json>, Resolved)> {
    if !has_dollar(raw) {
        return Ok((Cow::Borrowed(raw), Resolved::default()));
    }
    let mut copy = raw.clone();
    let mut resolved = Resolved::default();
    let scene = [declared.scene.as_slice()];
    resolve_canvas(&mut copy, &scene, None, &mut resolved)?;
    if let Some(layers) = copy.get_mut(LAYERS) {
        resolve_layers(layers, &scene, None, &mut resolved)?;
    }
    if let Some(comps) = copy.get_mut(COMPS).and_then(Json::as_object_mut) {
        for (id, comp) in comps {
            let scopes = [declared.of_comp(id), declared.scene.as_slice()];
            resolve_canvas(comp, &scopes, Some(id), &mut resolved)?;
            if let Some(layers) = comp.get_mut(LAYERS) {
                resolve_layers(layers, &scopes, Some(id), &mut resolved)?;
            }
        }
    }
    Ok((Cow::Owned(copy), resolved))
}

/// Resolves the references in the canvas keys of the scene or of the
/// composition `comp`, `owner`: its `bg`, written in hex whatever form its
/// slot has, and its `dur`, which a number slot gives in seconds ([D77]).
///
/// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
fn resolve_canvas(
    owner: &mut Json,
    scopes: &[&[Slot]],
    comp: Option<&str>,
    resolved: &mut Resolved,
) -> Result<()> {
    let place = comp.map_or_else(|| Place::Scene(Address::default()), comp_path);
    for key in [BG, DUR] {
        let Some(value) = owner.get_mut(key) else {
            continue;
        };
        let mut steps = vec![Step::Key(key.to_owned())];
        resolve_value(value, (scopes, comp), &place, &mut steps, resolved)?;
        // The canvas keys are written as text: a time, and a colour in hex.
        match (key, &*value) {
            (DUR, Json::Number(seconds)) => *value = Json::String(format!("{seconds}s")),
            (BG, Json::Array(_)) => {
                if let Some(color) = crate::color(value) {
                    *value = Json::String(hex(color));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `raw`, a scene's JSON, with every reference replaced by its slot's value
/// and every `$$` unescaped, as the scene is read: what an export compares
/// and a listing counts ([D77]).
///
/// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
///
/// # Errors
/// The diagnostic of a slot that does not read, or of a reference to no
/// slot in scope.
pub fn resolved(raw: &Json) -> Result<Json> {
    let raw = crate::styles::expand(raw)?;
    let declared = declared(&raw)?;
    resolve(&raw, &declared).map(|(json, _)| json.into_owned())
}

/// Whether any string in `json` starts with `$`.
fn has_dollar(json: &Json) -> bool {
    match json {
        Json::String(text) => text.starts_with('$'),
        Json::Array(items) => items.iter().any(has_dollar),
        Json::Object(fields) => fields.values().any(has_dollar),
        _ => false,
    }
}

/// Resolves the references in a list of layers of composition `comp`, none
/// for the scene's own. The scopes are searched in order, and the last is
/// the scene's slots: a slot found in any other is the composition's.
pub(crate) fn resolve_layers(
    layers: &mut Json,
    scopes: &[&[Slot]],
    comp: Option<&str>,
    resolved: &mut Resolved,
) -> Result<()> {
    let owner = comp.map(comp_path);
    let Some(layers) = layers.as_array_mut() else {
        return Ok(());
    };
    for (index, layer) in layers.iter_mut().enumerate() {
        let id = layer
            .get(ID)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned();
        let place = match &owner {
            Some(owner) => owner.clone().join(Place::layer(&id, index)),
            None => Place::layer(&id, index),
        };
        let Some(fields) = layer.as_object_mut() else {
            continue;
        };
        for (key, value) in fields.iter_mut().filter(|(key, _)| *key != ID) {
            let mut steps = vec![Step::Key(key.clone())];
            resolve_value(value, (scopes, comp), &place, &mut steps, resolved)?;
        }
    }
    Ok(())
}

/// Resolves the references in one value, at `steps` within `layer`.
fn resolve_value(
    value: &mut Json,
    (scopes, comp): (&[&[Slot]], Option<&str>),
    layer: &Place,
    steps: &mut Vec<Step>,
    resolved: &mut Resolved,
) -> Result<()> {
    match value {
        Json::String(text) => {
            let (id, shift) = match written(text) {
                Written::Plain => return Ok(()),
                Written::Escaped(rest) => {
                    *value = Json::String(format!("${rest}"));
                    return Ok(());
                }
                Written::Reference(id) => (id.to_owned(), None),
                Written::Changed(id, shift) => (id.to_owned(), Some(shift)),
            };
            let place = layer.clone().join(Place::Scene(Address {
                layers: Vec::new(),
                keys: steps.clone(),
            }));
            let found = scopes.iter().enumerate().find_map(|(scope, slots)| {
                slots
                    .iter()
                    .find(|s| *s.id == *id)
                    .map(|slot| (scope, slot))
            });
            let Some((scope, slot)) = found else {
                let known = scopes
                    .iter()
                    .flat_map(|slots| slots.iter())
                    .map(|s| format!("${}", s.id));
                return Err(Diagnostic::unknown("slot", format!("${id}"), known).at(place));
            };
            resolved.followed.push(Followed {
                place: place.to_string(),
                slot: slot.id.clone(),
                comp: comp.filter(|_| scope + 1 < scopes.len()).map(str::to_owned),
            });
            *value = match shift {
                None => slot.value.clone(),
                Some(shift) => changed(slot, shift).map_err(|d| d.at(place))?,
            };
        }
        Json::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                steps.push(Step::Index(index));
                resolve_value(item, (scopes, comp), layer, steps, resolved)?;
                steps.pop();
            }
        }
        Json::Object(fields) => {
            for (key, item) in fields.iter_mut() {
                steps.push(Step::Key(key.clone()));
                resolve_value(item, (scopes, comp), layer, steps, resolved)?;
                steps.pop();
            }
        }
        _ => {}
    }
    Ok(())
}

/// `color`, RGBA from 0 to 1, as `#RRGGBB`, or `#RRGGBBAA` when it is not
/// opaque.
fn hex(color: [f64; 4]) -> String {
    let [r, g, b, a] = color.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8);
    match a {
        255 => format!("#{r:02X}{g:02X}{b:02X}"),
        _ => format!("#{r:02X}{g:02X}{b:02X}{a:02X}"),
    }
}

/// The colour of `slot` changed by `shift`, as channels.
///
/// # Errors
/// An invalid-value diagnostic when the slot holds no colour.
fn changed(slot: &Slot, shift: Shift) -> Result<Json> {
    let color = match slot.ty {
        SlotType::Color => crate::text::color(&slot.value).ok(),
        _ => None,
    };
    let Some(color) = color else {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "slot `${}` holds a {}, so it takes no change of colour",
                slot.id, slot.ty
            ),
        )
        .expected(crate::oklch::SHIFT_FORM));
    };
    Ok(Json::from(shift.apply(color).to_vec()))
}

/// A slot as `nakkash slots` lists it: where it is declared, and every value
/// that follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    /// The composition that declares it; none for the scene.
    pub comp: Option<String>,
    /// The slot.
    pub slot: Slot,
    /// The place of each value written `"$id"` that takes its value, such
    /// as `plate.fill` or `comps.card.title.text`.
    pub followers: Vec<String>,
}

/// Every slot of a valid scene's JSON, the scene's first and then each
/// composition's, with the values that follow it.
pub(crate) fn listed(raw: &Json) -> Vec<Listed> {
    // Styles hold values that follow slots as layers do.
    let Ok(raw) = crate::styles::expand(raw) else {
        return Vec::new();
    };
    let raw = &*raw;
    let Ok(declared) = declared(raw) else {
        return Vec::new();
    };
    let followed = resolve(raw, &declared).map_or_else(|_| Vec::new(), |(_, r)| r.followed);
    let owned = declared.scene.iter().map(|slot| (None, slot)).chain(
        declared
            .comps
            .iter()
            .flat_map(|(comp, slots)| slots.iter().map(move |slot| (Some(comp.clone()), slot))),
    );
    owned
        .map(|(comp, slot)| Listed {
            followers: followed
                .iter()
                .filter(|f| f.slot == slot.id && f.comp == comp)
                .map(|f| f.place.clone())
                .collect(),
            comp,
            slot: slot.clone(),
        })
        .collect()
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${} {} {}", self.id, self.ty, self.value)
    }
}

#[cfg(test)]
#[path = "../tests/unit/slots.rs"]
mod tests;
