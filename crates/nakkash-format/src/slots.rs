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

use crate::parse::key::{COMPS, ID, LAYERS, SLOTS, comp_path};
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
}

impl SlotType {
    /// Every type, by the name a declaration writes.
    pub const NAMES: [(&str, SlotType); 4] = [
        ("number", SlotType::Number),
        ("point", SlotType::Point),
        ("color", SlotType::Color),
        ("text", SlotType::Text),
    ];

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
const FORM: &str = r##"a colour, a number, [x, y], or {"text":"…"}, optionally typed and labelled, e.g. {"color":"#E63946","label":"Accent"}"##;

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
    let (ty, value, label) = match written {
        Json::Object(fields) => {
            let label = match fields.get("label") {
                None => None,
                Some(Json::String(label)) => Some(label.clone()),
                Some(_) => return Err(invalid(written)),
            };
            let mut typed = fields.iter().filter(|(name, _)| *name != "label");
            let (Some((name, value)), None) = (typed.next(), typed.next()) else {
                return Err(invalid(written));
            };
            let ty = SlotType::named(name).ok_or_else(|| {
                Diagnostic::unknown("slot type", name, SlotType::NAMES.map(|(n, _)| n))
            })?;
            (ty, value.clone(), label)
        }
        Json::Number(_) => (SlotType::Number, written.clone(), None),
        // Two numbers are a point; three or four, a colour's channels.
        Json::Array(items) if items.len() == 2 => (SlotType::Point, written.clone(), None),
        Json::Array(_) => (SlotType::Color, written.clone(), None),
        Json::String(_) => (SlotType::Color, written.clone(), None),
        _ => return Err(invalid(written)),
    };
    check_fits(ty, &value).map_err(|d| d.expected(FORM))?;
    Ok(Slot {
        id,
        ty,
        value,
        label,
    })
}

/// `Ok` when `value` is a value of type `ty`.
fn check_fits(ty: SlotType, value: &Json) -> Result<()> {
    let fits = match (ty, value) {
        (SlotType::Number, Json::Number(_)) | (SlotType::Text, Json::String(_)) => true,
        (SlotType::Point, Json::Array(xy)) => xy.len() == 2 && xy.iter().all(Json::is_number),
        (SlotType::Color, Json::String(_) | Json::Array(_)) => crate::text::color(value).is_ok(),
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
            check_fits(slot.ty, value).map_err(|d| {
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
    /// `$$…`: itself, one `$` fewer.
    Escaped(&'a str),
    /// Anything else, itself.
    Plain,
}

fn written(text: &str) -> Written<'_> {
    if let Some(rest) = text.strip_prefix("$$") {
        return Written::Escaped(rest);
    }
    match text.strip_prefix('$') {
        Some(id) if Id::is_valid(id) => Written::Reference(id),
        _ => Written::Plain,
    }
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
    if let Some(layers) = copy.get_mut(LAYERS) {
        let scopes = [declared.scene.as_slice()];
        resolve_layers(layers, &scopes, None, &mut resolved)?;
    }
    if let Some(comps) = copy.get_mut(COMPS).and_then(Json::as_object_mut) {
        for (id, comp) in comps {
            if let Some(layers) = comp.get_mut(LAYERS) {
                let scopes = [declared.of_comp(id), declared.scene.as_slice()];
                resolve_layers(layers, &scopes, Some(id), &mut resolved)?;
            }
        }
    }
    Ok((Cow::Owned(copy), resolved))
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
        Json::String(text) => match written(text) {
            Written::Plain => {}
            Written::Escaped(rest) => *value = Json::String(format!("${rest}")),
            Written::Reference(id) => {
                let place = layer.clone().join(Place::Scene(Address {
                    layers: Vec::new(),
                    keys: steps.clone(),
                }));
                let found = scopes.iter().enumerate().find_map(|(scope, slots)| {
                    slots
                        .iter()
                        .find(|s| &*s.id == id)
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
                *value = slot.value.clone();
            }
        },
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
