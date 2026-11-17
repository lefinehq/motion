//! Controls as slots ([D44]): an expression that only reads an expression
//! control, `thisComp.layer("Controls").effect("Accent")("Color")`, becomes
//! a reference to a slot that holds the value it read, so one change to the
//! slot changes every value that read the control.
//!
//! - The value is the baked one, in the scene's units for the property, so
//!   the scene draws as it would with the value written out. A read whose
//!   value is not still, or not one a slot holds, stays baked.
//! - A slot is declared where the reference can see it: in the composition
//!   that reads its own control, or on the scene when the control is in the
//!   composition imported as the scene. A read of any other composition
//!   stays baked.
//! - A control's slot holds the value most of its reads gave. A read that
//!   gives another value, as a property in other units does, stays baked.
//!
//! [D44]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d44-slots-are-named-values-that-layers-follow

use std::collections::BTreeMap;

use nakkash_format::{Id, SlotType};
use serde_json::{Map, Value as Json, json};
use winnow::ascii::{dec_uint, multispace0};
use winnow::combinator::{alt, delimited, opt, preceded, terminated};
use winnow::prelude::*;
use winnow::token::take_till;

use super::Walk;
use super::ids::Ids;
use super::keys::Convert;
use super::report::Outcome;
use super::sources::{Marks, Writable};
use crate::model::{Composition, ItemKind, Layer, Origin, Property, Stream, Value};

/// What a reference is written as until the scene is finished, when every
/// other string that starts with `$` is escaped: a project's text never
/// holds this character.
pub(super) const REFERENCE: char = '\u{0}';

/// A name or a 1-based index, as an expression picks a layer, an effect or
/// a parameter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Pick {
    /// By name.
    Name(String),
    /// By position, from 1.
    Index(u32),
}

/// The composition a read names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CompPick {
    /// `thisComp`, or none: the one the expression is in.
    This,
    /// `comp("Main")`.
    Named(String),
}

/// An expression whose whole body reads one effect parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Read {
    /// The composition of the layer it reads.
    pub comp: CompPick,
    /// The layer, none for the expression's own.
    pub layer: Option<Pick>,
    /// The effect.
    pub effect: Pick,
    /// The parameter.
    pub param: Pick,
}

/// The read `source` is, if its whole body is one: an optional layer,
/// `thisComp.layer(…).`, `comp("…").layer(…).` or `thisLayer.`, then
/// `effect(…)(…)`, an optional `.value` and an optional `;`.
pub(super) fn read(source: &str) -> Option<Read> {
    let pick = || {
        delimited(
            ('(', multispace0),
            alt((quoted.map(Pick::Name), dec_uint.map(Pick::Index))),
            (multispace0, ')'),
        )
    };
    let comp = alt((
        "thisComp".value(CompPick::This),
        preceded(
            "comp",
            delimited(('(', multispace0), quoted, (multispace0, ')')),
        )
        .map(CompPick::Named),
    ));
    let layer = alt((
        terminated((comp, preceded(".layer", pick())), '.')
            .map(|(comp, layer)| (comp, Some(layer))),
        "thisLayer.".value((CompPick::This, None)),
    ));
    let mut body = delimited(
        multispace0,
        (
            opt(layer),
            preceded("effect", pick()),
            pick(),
            opt(".value"),
        ),
        (multispace0, opt(';'), multispace0),
    );
    let (rest, (place, effect, param, _)) = body.parse_peek(source).ok()?;
    if !rest.is_empty() {
        return None;
    }
    let (comp, layer) = place.unwrap_or((CompPick::This, None));
    Some(Read {
        comp,
        layer,
        effect,
        param,
    })
}

/// A string in double or single quotes, with no escapes.
fn quoted(input: &mut &str) -> winnow::Result<String> {
    alt((
        delimited('"', take_till(0.., ['"', '\\']), '"'),
        delimited('\'', take_till(0.., ['\'', '\\']), '\''),
    ))
    .map(str::to_owned)
    .parse_next(input)
}

/// One value read from a control: by an expression, or a property a
/// template's panel control drives.
#[derive(Debug, Clone, PartialEq)]
struct Follower {
    /// The value, in the scene's units for its property.
    value: Json,
    /// How its property converts the control's value into the scene's;
    /// none for text.
    convert: Option<Convert>,
    /// Its expression's report; none for a property a panel control
    /// drives.
    report: Option<usize>,
}

/// A control that expressions read, which may become a slot.
#[derive(Debug, Clone, PartialEq)]
struct Control {
    /// The composition item, layer and parameter.
    key: (u32, u32, Pick, Pick),
    /// The scene composition its slot would be declared in, none for the
    /// scene's own.
    scope: Option<String>,
    /// What its slot's id is made from.
    name: String,
    /// The control's name, for people.
    label: String,
    /// Where the project stores the control's value, and that value, when
    /// it stores one that an export can write.
    stored: Option<(Origin, Value)>,
    /// The values read from it, in the order written.
    followers: Vec<Follower>,
    /// What holds its value, when it is found, so two reads of one
    /// property are one control however they name it.
    property: Option<Held>,
    /// The type its slot takes, when its values do not say: text.
    ty: Option<SlotType>,
    /// Whether a template's panel offers it ([D53]), so its slot is the
    /// scene's and the report counts it.
    ///
    /// [D53]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d53-a-mogrt-imports-as-its-project-with-its-controls-as-slots
    panel: bool,
}

/// A control a template's panel offers, before anything reads it.
pub(super) struct Offered {
    /// What its slot's id is made from, and its label.
    pub name: String,
    /// Its label, with its group's name.
    pub label: String,
    /// The type its slot takes, when its values do not say.
    pub ty: Option<SlotType>,
    /// What holds each value it drives.
    pub properties: Vec<Held>,
    /// Where the project stores its value, when it stores one an export can
    /// write.
    pub stored: Option<(Origin, Value)>,
}

/// The controls read so far.
#[derive(Debug, Default)]
pub(super) struct Slots(Vec<Control>);

/// The slots the controls became: each scope's declarations, and what each
/// value read from a control is written as.
#[derive(Debug, Default)]
pub(super) struct Decided {
    /// The `slots` of each scope, by the scene composition, none for the
    /// scene's own.
    pub declared: BTreeMap<Option<String>, Map<String, Json>>,
    /// By control and follower, its reference or its own value.
    written: Vec<Vec<Json>>,
}

impl Slots {
    /// Makes each control a slot holding the value most of its reads gave,
    /// the first of those on a tie: those reads follow it, and the others
    /// keep their own values, baked. Each following expression's report
    /// says which slot it follows, and a slot whose control the project
    /// stores, as the reads it holds give it, is marked to go back into it
    /// ([D45]) through their conversion, added to `sources`.
    ///
    /// [D45]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d45-export-patches-the-project-it-came-from
    pub(super) fn decide(
        self,
        mapping: &mut super::report::Mapping,
        sources: &mut Vec<Writable>,
    ) -> Decided {
        // Ids are unique over every scope, so a composition's slot never
        // hides one of the scene's that its layers follow.
        let mut ids = Ids::default();
        let mut decided = Decided::default();
        for control in self.0 {
            if control.followers.is_empty() {
                // A panel control nothing in the scene follows.
                if let Some(panel) = mapping.panel.as_mut() {
                    panel.left_out.push(super::report::Finding {
                        at: control.label.clone(),
                        what: "nothing the scene draws follows it".into(),
                    });
                }
                decided.written.push(Vec::new());
                continue;
            }
            let mut tally: Vec<(&Json, usize)> = Vec::new();
            for follower in &control.followers {
                match tally
                    .iter_mut()
                    .find(|(value, _)| **value == follower.value)
                {
                    Some((_, count)) => *count += 1,
                    None => tally.push((&follower.value, 1)),
                }
            }
            // The first of the largest counts: `max_by_key` keeps the last.
            let most = tally.iter().map(|(_, n)| *n).max().unwrap_or(0);
            let value = tally
                .iter()
                .find(|(_, n)| *n == most)
                .map(|(value, _)| (*value).clone())
                .expect("a control is read at least once");
            let id = ids.give(&control.name, "slot");
            let ty = control
                .ty
                .or_else(|| shorthand(&value))
                .expect("only values a slot holds are read");
            let mut written = Map::new();
            written.insert(ty.name().to_owned(), value.clone());
            written.insert("label".to_owned(), json!(control.label));
            let convert = control
                .followers
                .iter()
                .find(|f| f.value == value)
                .and_then(|f| f.convert);
            // A control may store its value otherwise than expressions read
            // it, a point in fractions of its layer or a slider in percent:
            // only one whose stored value converts to the slot's goes back.
            let stored = control.stored.filter(|(_, template)| {
                convert.and_then(|c| c.value(template)).as_ref() == Some(&value)
            });
            if let (Some((origin, template)), Some(convert)) = (stored, convert) {
                sources.push(Writable::Static {
                    origin,
                    convert,
                    template,
                });
                let mut marks = Marks::default();
                marks.add(ty.name().to_owned(), sources.len() - 1);
                marks.write(&mut written);
            }
            decided
                .declared
                .entry(control.scope)
                .or_default()
                .insert(id.clone(), Json::Object(written));
            match (control.panel, mapping.panel.as_mut()) {
                (true, Some(panel)) => {
                    panel.slots += 1;
                    panel.ids.push(id.clone());
                }
                _ => mapping.slots += 1,
            }
            let followers = control
                .followers
                .into_iter()
                .map(|follower| match follower.value == value {
                    true => {
                        if let Some(report) =
                            follower.report.and_then(|r| mapping.expressions.get_mut(r))
                        {
                            report.outcome = Outcome::Slot { slot: id.clone() };
                        }
                        Json::String(format!("${id}"))
                    }
                    false => follower.value,
                })
                .collect();
            decided.written.push(followers);
        }
        decided
    }
}

/// The type of a slot that holds `value`, written alone; none for a value
/// no slot holds.
fn shorthand(value: &Json) -> Option<SlotType> {
    match value {
        Json::Number(_) => Some(SlotType::Number),
        Json::Array(xy) if xy.len() == 2 && xy.iter().all(Json::is_number) => Some(SlotType::Point),
        Json::String(_) | Json::Array(_) if nakkash_format::is_color(value) => {
            Some(SlotType::Color)
        }
        _ => None,
    }
}

impl<'p> Walk<'p> {
    /// What `value` is written as until the controls are decided, when
    /// `source`, the expression that gave the property being written and
    /// whose report is `report`, only reads a control that a slot in scope
    /// can hold, or that a template's panel offers.
    pub(super) fn read_control(
        &mut self,
        source: &str,
        (value, convert): (&Json, Convert),
        report: usize,
    ) -> Option<Json> {
        let read = read(source)?;
        shorthand(value)?;
        let (composition, layer) = self.writing?;
        let own = self.item_of(composition)?;
        let (item, read_in) = match &read.comp {
            CompPick::This => (own, composition),
            CompPick::Named(name) => self.comp_named(name)?,
        };
        let read_layer = match &read.layer {
            None => layer,
            Some(Pick::Name(name)) => read_in.layers.iter().find(|l| &l.name == name)?,
            Some(Pick::Index(index)) => read_in
                .layers
                .get(usize::try_from(*index).ok()?.checked_sub(1)?)?,
        };
        let (held, property) = self
            .control_property(read_layer, &read.effect, &read.param)
            .unzip();
        let property = property.flatten();
        let follower = Follower {
            value: value.clone(),
            convert: Some(convert),
            report: Some(report),
        };
        // A control the panel offers is the scene's, whichever composition
        // reads it.
        if let Some(offered) = held.as_ref().and_then(|h| self.offered.get(h).copied()) {
            return Some(self.slots.follow(offered, follower));
        }
        // A reference sees its own composition's slots, then the scene's.
        let scope = match (item == own, Some(item) == self.root) {
            (true, _) => self.scope.clone(),
            (false, true) => None,
            (false, false) => return None,
        };
        let key = (item, read_layer.id, read.effect.clone(), read.param.clone());
        let stored = property.and_then(|p| stored(p, read_layer));
        let controls = &mut self.slots.0;
        // One parameter read as `effect("Size")(1)` and as
        // `effect("Size")("Slider")` is one control: the property tells.
        let same = |c: &Control| match (&c.property, &held) {
            (Some(a), Some(b)) => a == b,
            _ => c.key == key,
        };
        let index = match controls.iter().position(same) {
            Some(index) => index,
            None => {
                let label = match &read.effect {
                    Pick::Name(name) => name.clone(),
                    Pick::Index(index) => format!("effect {index}"),
                };
                let name = match is_control_layer(read_layer) {
                    true => label.clone(),
                    false => format!("{} {label}", read_layer.name),
                };
                controls.push(Control {
                    key,
                    scope,
                    name,
                    label,
                    stored,
                    followers: Vec::new(),
                    property: held,
                    ty: None,
                    panel: false,
                });
                controls.len() - 1
            }
        };
        Some(self.slots.follow(index, follower))
    }

    /// What holds the parameter `param` of the effect `effect` of `layer`,
    /// and the parameter, when the project stores it.
    fn control_property(
        &self,
        layer: &'p Layer,
        effect: &Pick,
        param: &Pick,
    ) -> Option<(Held, Option<&'p Property>)> {
        let definitions = &self.project.effects;
        let mut effects = super::layer::effects(layer);
        let group = match effect {
            Pick::Name(name) => effects.find(|group| {
                let shown = group
                    .name
                    .as_deref()
                    .or_else(|| definitions.get(&group.match_name).map(|d| d.name.as_str()));
                shown == Some(name.as_str())
            })?,
            Pick::Index(index) => effects.nth(usize::try_from(*index).ok()?.checked_sub(1)?)?,
        };
        // A parameter's match name is its effect's and its 1-based index.
        let match_name = match param {
            Pick::Index(index) => format!("{}-{index:04}", group.match_name),
            Pick::Name(name) => definitions
                .get(&group.match_name)?
                .parameters
                .iter()
                .find(|p| &p.name == name)?
                .match_name
                .clone(),
        };
        let property = group.properties().find(|p| p.match_name == match_name);
        let held = match property {
            Some(property) => Held::Stored(address(property)),
            None => Held::Default(address(group), match_name),
        };
        Some((held, property))
    }

    /// The id of the composition item `composition` is.
    fn item_of(&self, composition: &Composition) -> Option<u32> {
        self.items.values().find_map(|item| match &item.kind {
            ItemKind::Composition(c) if std::ptr::eq(c, composition) => Some(item.id),
            _ => None,
        })
    }

    /// The one composition called `name`, with its item id.
    fn comp_named(&self, name: &str) -> Option<(u32, &'p Composition)> {
        let mut named = self
            .project
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Composition(c) if item.name == name => Some((item.id, c)),
                _ => None,
            });
        let found = named.next()?;
        named.next().is_none().then_some(found)
    }
}

/// Where the project stores `property`, a parameter of an effect of
/// `layer`, and its value, when the effect is a control and the project
/// stores a still value: a parameter at its default is not stored.
fn stored(property: &Property, layer: &Layer) -> Option<(Origin, Value)> {
    let group = super::layer::effects(layer)
        .find(|group| group.properties().any(|p| std::ptr::eq(p, property)))?;
    // An effect the scene draws writes its own parameters, which go back
    // from there.
    if !matches!(
        super::rows::policy(&group.match_name),
        Some(super::rows::Policy::Control)
    ) {
        return None;
    }
    still(property)
}

/// Where the project stores `property`'s value, and that value, when it is
/// still and stored.
pub(super) fn still(property: &Property) -> Option<(Origin, Value)> {
    match (&property.value, &property.origin) {
        (Stream::Static(value), Some(origin)) => Some((origin.clone(), value.clone())),
        _ => None,
    }
}

/// The address of `thing`, which names it while the project is read.
pub(super) fn address<T>(thing: &T) -> usize {
    std::ptr::from_ref(thing).addr()
}

/// What holds a value a control gives: a property the project stores, or
/// one it leaves out at its default, named by what owns it (a layer for
/// its transform, an effect for its parameters) and its match name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Held {
    /// A property, by its address.
    Stored(usize),
    /// A property left out, by its owner's address and its match name.
    Default(usize, String),
}

impl Slots {
    /// Adds a control the panel offers, declared on the scene, and gives
    /// its index.
    pub(super) fn offer(&mut self, offered: Offered) -> usize {
        self.0.push(Control {
            key: (0, 0, Pick::Index(0), Pick::Index(0)),
            scope: None,
            name: offered.name,
            label: offered.label,
            stored: offered.stored,
            followers: Vec::new(),
            property: offered.properties.first().cloned(),
            ty: offered.ty,
            panel: true,
        });
        self.0.len() - 1
    }

    /// Adds `follower` to the control at `index`, and gives what its value
    /// is written as until the controls are decided.
    fn follow(&mut self, index: usize, follower: Follower) -> Json {
        let followers = &mut self.0[index].followers;
        followers.push(follower);
        Json::String(format!("{REFERENCE}{index}:{}", followers.len() - 1))
    }
}

impl Walk<'_> {
    /// What the still `value` of the property `held` names is written as
    /// until the controls are decided, when a template's panel control
    /// drives it:
    /// `convert` turns the project's value into it, none for text.
    pub(super) fn follow_panel(
        &mut self,
        held: &Held,
        value: &Json,
        convert: Option<Convert>,
    ) -> Option<Json> {
        let index = *self.offered.get(held)?;
        self.slots.0[index].ty.or_else(|| shorthand(value))?;
        Some(self.slots.follow(
            index,
            Follower {
                value: value.clone(),
                convert,
                report: None,
            },
        ))
    }
}

/// A layer whose effects are all expression controls, and that has some.
pub(super) fn is_control_layer(layer: &Layer) -> bool {
    let mut effects = super::layer::effects(layer).peekable();
    effects.peek().is_some()
        && effects.all(|e| {
            matches!(
                super::rows::policy(&e.match_name),
                Some(super::rows::Policy::Control)
            )
        })
}

/// Whether the format would read `text` as other than itself: `$` and an
/// id is a reference, and `$$` stands for one `$`.
fn read_as_slot(text: &str) -> bool {
    text.starts_with("$$") || text.strip_prefix('$').is_some_and(Id::is_valid)
}

/// `json`, layers of the scene, with every value read from a control
/// written as `decided` says, and every other string the format would read
/// as a reference or an escape escaped.
pub(super) fn write_references(json: &mut Json, decided: &Decided) {
    match json {
        Json::String(text) => {
            if let Some(read) = text.strip_prefix(REFERENCE) {
                let written = read.split_once(':').and_then(|(control, follower)| {
                    let control: usize = control.parse().ok()?;
                    let follower: usize = follower.parse().ok()?;
                    decided.written.get(control)?.get(follower).cloned()
                });
                *json = written.expect("every read is decided");
            } else if read_as_slot(text) {
                text.insert(0, '$');
            }
        }
        Json::Array(items) => items
            .iter_mut()
            .for_each(|item| write_references(item, decided)),
        Json::Object(fields) => fields
            .iter_mut()
            .filter(|(key, _)| *key != "id")
            .for_each(|(_, value)| write_references(value, decided)),
        _ => {}
    }
}

#[cfg(test)]
#[path = "../../tests/unit/map/slots.rs"]
mod tests;
