//! Rows, items and the groups around them: each property of a group goes
//! where its row says, each item group becomes a scene item of its kind,
//! and every other group follows its policy.

use motion_format::item::{ItemKind, Primary};
use motion_format::property::{Registry, Rest, Target};
use serde_json::{Map, Value as Json, json};

use super::Walk;
use super::keys::{Clock, Written, num, written};
use super::report::Fate;
use super::rows::{self, Kind, Missing, Part, Policy, Row, To};
use crate::model::{Child, Control, EffectDefinition, Group, Property, Stream, Value};

/// Why a hidden property or group is ignored.
const NOT_APPLYING: &str =
    "it does not apply to the layer as it is (3D-only, or the unused half of a position)";

/// Where a group sits, for its defaults and its findings.
#[derive(Debug, Clone)]
pub(super) struct Place<'p> {
    /// Where it is, as `composition/layer/match name/…`.
    pub at: String,
    /// The layer's clock.
    pub clock: Clock,
    /// The centre of the layer's composition.
    pub comp_centre: [f64; 2],
    /// The centre of the layer's source, or the origin.
    pub source_centre: [f64; 2],
    /// The definition of the effect being mapped, if any.
    pub effect: Option<&'p EffectDefinition>,
}

impl Place<'_> {
    /// The place of `name` inside this one.
    pub(super) fn inside(&self, name: &str) -> Self {
        Place {
            at: format!("{}/{name}", self.at),
            ..self.clone()
        }
    }
}

/// An object being written: its keys and its `anim` tracks.
#[derive(Debug, Default)]
pub(super) struct Out {
    /// Its keys, resting values among them.
    pub keys: Map<String, Json>,
    /// Its tracks, by target name.
    pub anim: Map<String, Json>,
}

impl Out {
    /// Sets one component of a property's resting value, which a scene
    /// writes whole: the other components stay as set, or 0 until they are.
    pub(super) fn set_component(&mut self, target: Target, value: Json) {
        let (Some(index), name) = (target.component, target.prop.name()) else {
            return;
        };
        let count = target.prop.spec().components.len();
        let whole = self
            .keys
            .entry(name)
            .or_insert_with(|| Json::Array(vec![json!(0); count]));
        if let Some(slot) = whole.as_array_mut().and_then(|parts| parts.get_mut(index)) {
            *slot = value;
        }
    }

    /// The object, with its tracks under `anim` when it has any.
    pub(super) fn finish(mut self) -> Map<String, Json> {
        if !self.anim.is_empty() {
            self.keys.insert("anim".into(), Json::Object(self.anim));
        }
        self.keys
    }
}

/// Where rows write: the registry of the object and the item kind, if any,
/// whose settings and primary key the rows may fill.
#[derive(Debug, Clone, Copy)]
pub(super) struct Sink {
    /// The registry the rows' property names are in.
    pub registry: &'static Registry,
    /// The item kind, for its settings.
    pub kind: Option<&'static ItemKind>,
}

impl Walk<'_> {
    /// Writes `properties` through `rows` into `out`, then the defaults of the
    /// rows they leave out; a property without a row is unsupported.
    pub(super) fn rows(
        &mut self,
        properties: &[&Property],
        rows: &[Row],
        sink: Sink,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        properties.iter().for_each(|property| {
            let here = place.inside(&property.match_name);
            match rows
                .iter()
                .find(|row| row.match_name == property.match_name)
            {
                Some(row) => self.put(row, Some(property), sink, out, &here),
                None => self.unknown_property(property, &here),
            }
        });
        rows.iter()
            .filter(|row| !properties.iter().any(|p| p.match_name == row.match_name))
            .for_each(|row| self.put(row, None, sink, out, &place.inside(row.match_name)));
    }

    /// A property no row names: unsupported, unless it does not apply.
    pub(super) fn unknown_property(&mut self, property: &Property, place: &Place<'_>) {
        if property.hidden {
            self.mapping.ignore(&property.match_name, NOT_APPLYING);
        } else {
            self.mapping.count(&property.match_name, Fate::Unsupported);
            self.mapping
                .unsupported(&place.at, format!("`{}`", property.match_name));
        }
    }

    /// Writes one row's property, or its default when `property` is `None`.
    fn put(
        &mut self,
        row: &Row,
        property: Option<&Property>,
        sink: Sink,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let Some(property) = property else {
            return self.put_default(row, sink, out, place);
        };
        let name = row.match_name;
        if property.hidden {
            return self.mapping.ignore(name, NOT_APPLYING);
        }
        if property.expression.as_ref().is_some_and(|e| e.enabled) {
            self.mapping
                .unsupported(&place.at, "expressions (the keys under them are kept)");
        }
        match row.to {
            To::Ignore(why) => self.mapping.ignore(name, why),
            To::OnlyDefault(why) => match self.is_default(&property.value, row, place) {
                true => self.mapping.ignore(name, "at its default"),
                false => {
                    self.mapping.count(name, Fate::Unsupported);
                    self.mapping.unsupported(&place.at, why);
                }
            },
            To::Prop(target) => {
                let spatial = sink
                    .registry
                    .target(target)
                    .is_some_and(|t| t.component.is_none() && t.prop.spec().spatial);
                let component = sink
                    .registry
                    .target(target)
                    .filter(|t| t.component.is_some());
                match written(&property.value, row.convert, spatial, place.clock) {
                    Some(Written::Value(value)) => match component {
                        Some(component) => out.set_component(component, value),
                        None => {
                            out.keys.insert(target.into(), value);
                        }
                    },
                    Some(Written::Track { keys, first }) => {
                        out.anim.insert(target.into(), keys);
                        match component {
                            Some(component) => out.set_component(component, first),
                            None if primary_property(sink.kind) == Some(target) => {
                                out.keys.insert(target.into(), first);
                            }
                            None => {}
                        }
                    }
                    None => {
                        self.mapping.count(name, Fate::Unsupported);
                        return self
                            .mapping
                            .unsupported(&place.at, "a value of a type its row does not take");
                    }
                }
                self.mapping.count(name, Fate::Mapped);
            }
            To::Setting(setting, choices) => {
                let first = match &property.value {
                    Stream::Static(value) => Some(value),
                    Stream::Animated(keys) => {
                        self.mapping.unsupported(
                            &place.at,
                            "an animated setting (its first value is kept)",
                        );
                        keys.first().map(|key| &key.value)
                    }
                };
                let code = match first {
                    Some(Value::Number(n)) => Some(n.round() as i64),
                    _ => None,
                };
                match choices.iter().find(|(c, _)| Some(*c) == code) {
                    Some((_, choice)) => {
                        out.keys.insert(setting.into(), json!(choice));
                        self.mapping.count(name, Fate::Mapped);
                    }
                    None => {
                        self.mapping.count(name, Fate::Unsupported);
                        self.mapping.unsupported(
                            &place.at,
                            format!("a `{setting}` the scene does not have"),
                        );
                    }
                }
            }
        }
    }

    /// Writes the default of a row the project leaves out, where the scene's
    /// own rest differs from it.
    fn put_default(&mut self, row: &Row, sink: Sink, out: &mut Out, place: &Place<'_>) {
        let To::Prop(target) = row.to else {
            return;
        };
        let Some(value) = default_value(row, place) else {
            return;
        };
        let Some(json) = row.convert.value(&value) else {
            return;
        };
        self.mapping.count(row.match_name, Fate::Defaulted);
        let differs = sink
            .registry
            .target(target)
            .is_none_or(|target| rest_json(target).is_none_or(|rest| rest != json));
        if differs {
            out.keys.insert(target.into(), json);
        }
    }

    /// Whether `stream` holds the row's default all the time. A row whose
    /// default is unknown holds it only where the project leaves it out.
    fn is_default(&self, stream: &Stream, row: &Row, place: &Place<'_>) -> bool {
        let (Stream::Static(value), Some(default)) = (stream, default_value(row, place)) else {
            return false;
        };
        let same = |a: f64, b: f64| (a - b).abs() < 1e-6;
        match (value, &default) {
            (Value::Number(a), Value::Number(b)) => same(*a, *b),
            (Value::Vector2(a), Value::Vector2(b)) => a.iter().zip(b).all(|(a, b)| same(*a, *b)),
            (Value::Color(a), Value::Color(b)) => [a.r, a.g, a.b, a.a]
                .iter()
                .zip([b.r, b.g, b.b, b.a])
                .all(|(a, b)| same(*a, b)),
            _ => false,
        }
    }

    /// The items among `group`'s children, each of a kind a table names; any
    /// other child group follows its policy, and the items of groups that
    /// pass their children through join the list.
    pub(super) fn items(&mut self, group: &Group, place: &Place<'_>) -> Vec<Json> {
        group
            .children
            .iter()
            .flat_map(|child| match child {
                Child::Property(property) => {
                    self.unknown_property(property, &place.inside(&property.match_name));
                    Vec::new()
                }
                Child::Group(child) => self.child_items(child, place),
            })
            .collect()
    }

    /// The items `group` stands for: itself, those it passes through, or
    /// none, as its kind or its policy says.
    fn child_items(&mut self, group: &Group, place: &Place<'_>) -> Vec<Json> {
        let here = place.inside(&group.match_name);
        if group.hidden {
            self.mapping.ignore(&group.match_name, NOT_APPLYING);
            return Vec::new();
        }
        match rows::kind(&group.match_name) {
            Some(kind) => vec![self.item(kind, group, &here)],
            None => match rows::policy(&group.match_name) {
                Some(Policy::Through) => {
                    self.mapping.count(&group.match_name, Fate::Mapped);
                    self.items(group, &here)
                }
                _ => {
                    self.group(group, &here);
                    Vec::new()
                }
            },
        }
    }

    /// One item of `kind` from `group`.
    fn item(&mut self, kind: &'static Kind, group: &Group, place: &Place<'_>) -> Json {
        self.mapping.count(kind.match_name, Fate::Mapped);
        let scene_kind = kind.scene_kind();
        let sink = Sink {
            registry: scene_kind.registry,
            kind: Some(scene_kind),
        };
        let effect = self.project.effects.get(kind.match_name);
        let place = Place {
            effect: effect.or(place.effect),
            ..place.clone()
        };
        let mut out = Out::default();
        let mut properties = Vec::new();
        let mut sub_items = Vec::new();
        group.children.iter().for_each(|child| match child {
            Child::Property(property) => properties.push(property),
            Child::Group(child) => match kind.part(&child.match_name) {
                Some(Part::Items) => {
                    sub_items.extend(self.items(child, &place.inside(&child.match_name)))
                }
                Some(Part::Rows) => {
                    let inner = place.inside(&child.match_name);
                    child
                        .children
                        .iter()
                        .for_each(|grandchild| match grandchild {
                            Child::Property(property) => properties.push(property),
                            Child::Group(nested) => {
                                self.group(nested, &inner.inside(&nested.match_name))
                            }
                        });
                }
                None => self.group(child, &place.inside(&child.match_name)),
            },
        });
        self.rows(&properties, kind.rows, sink, &mut out, &place);
        let primary = match scene_kind.primary {
            Primary::Items(_) => Json::Array(sub_items),
            Primary::Property(name) => out
                .keys
                .remove(name)
                .or_else(|| sink.registry.target(name).and_then(rest_json))
                .unwrap_or(Json::Null),
            Primary::Setting(name) => out
                .keys
                .remove(name)
                .unwrap_or_else(|| json!(scene_kind.setting(name).map(|s| s.default))),
        };
        let mut keys = Map::new();
        keys.insert(kind.item.into(), primary);
        keys.extend(out.finish());
        if !group.enabled {
            keys.insert("hidden".into(), json!(true));
        }
        Json::Object(keys)
    }

    /// A group that is not an item, by its policy; one without a policy is
    /// unsupported.
    pub(super) fn group(&mut self, group: &Group, place: &Place<'_>) {
        let name = group.match_name.as_str();
        if group.hidden {
            return self.mapping.ignore(name, NOT_APPLYING);
        }
        match rows::policy(name) {
            Some(Policy::Ignore(why)) => self.mapping.ignore(name, why),
            Some(Policy::OnlyEmpty(why)) => {
                match group.properties().any(|property| !property.hidden) {
                    false => self.mapping.ignore(name, "empty"),
                    true => {
                        self.mapping.count(name, Fate::Unsupported);
                        self.mapping.unsupported(&place.at, why);
                    }
                }
            }
            Some(Policy::Styles) => {
                self.mapping.ignore(name, "layer styles, all switched off");
                group.children.iter().for_each(|child| match child {
                    Child::Group(style)
                        if style.enabled && !style.match_name.starts_with("ADBE") =>
                    {
                        self.mapping.count(&style.match_name, Fate::Unsupported);
                        let what = style.match_name.split('/').next().unwrap_or_default();
                        self.mapping.unsupported(
                            &place.inside(&style.match_name).at,
                            format!("the layer style `{what}`"),
                        );
                    }
                    Child::Group(style) => self.mapping.count(&style.match_name, Fate::Ignored),
                    Child::Property(property) => {
                        self.mapping.count(&property.match_name, Fate::Ignored);
                    }
                });
            }
            Some(Policy::Through) | None => {
                self.mapping.count(name, Fate::Unsupported);
                self.mapping.unsupported(&place.at, format!("`{name}`"));
            }
        }
    }
}

/// The property a kind's key holds, if it holds one.
fn primary_property(kind: Option<&ItemKind>) -> Option<&'static str> {
    match kind?.primary {
        Primary::Property(name) => Some(name),
        _ => None,
    }
}

/// The scene value a target rests at when the object sets none, where that
/// is a fixed value.
fn rest_json(target: Target) -> Option<Json> {
    let Rest::Value(value) = &target.prop.spec().rest else {
        return None;
    };
    let whole = match value {
        motion_format::Value::Number(n) => num(*n),
        motion_format::Value::Point([x, y]) => json!([num(*x), num(*y)]),
        motion_format::Value::Color([r, g, b, a]) => json!([num(*r), num(*g), num(*b), num(*a)]),
        _ => return None,
    };
    match (target.component, whole) {
        (Some(i), Json::Array(parts)) => parts.get(i).cloned(),
        (_, whole) => Some(whole),
    }
}

/// A row's default as a project value, if it has one here.
fn default_value(row: &Row, place: &Place<'_>) -> Option<Value> {
    Some(match row.default {
        Missing::Absent => return None,
        Missing::Number(n) => Value::Number(n),
        Missing::Vector(v) => Value::Vector2(v),
        Missing::Color([r, g, b, a]) => Value::Color(crate::model::Color { r, g, b, a }),
        Missing::CompCentre => Value::Vector2(place.comp_centre),
        Missing::SourceCentre => Value::Vector2(place.source_centre),
        Missing::Parameter => {
            let parameter = place.effect?.parameter(row.match_name)?;
            match &parameter.control {
                Control::Slider { default, .. } | Control::Angle { default } => {
                    Value::Number(*default)
                }
                Control::Checkbox { default } => Value::Number(f64::from(u8::from(*default))),
                Control::Popup { default, .. } => Value::Number(f64::from(*default)),
                Control::Color { default } => Value::Color(*default),
                Control::Layer | Control::Point { .. } | Control::Other { .. } => return None,
            }
        }
    })
}
