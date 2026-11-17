//! Rows, items and the groups around them: each property of a group goes
//! where its row says, each item group becomes a scene item of its kind,
//! and every other group follows its policy.

use nakkash_format::item::{ItemKind, Primary};
use nakkash_format::property::{Registry, Rest, Target, default_zoom};
use serde_json::{Map, Value as Json, json};

use super::keys::{Clock, Convert, Written, num, split, written};
use super::report::{ControlReport, Fate};
use super::rows::{self, Kind, Missing, Part, Policy, Row, To};
use super::slots::{Held, address};
use super::sources::{Marks, Writable};
use super::{Walk, curves, levels};
use crate::bake::Bake;
use crate::model::{
    Child, ComposedLine, Control, EffectDefinition, Group, Keyframe, Property, Stream, Value,
};

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
    /// The size of the layer's composition.
    pub comp_size: [f64; 2],
    /// The frame rate of the layer's composition, at which what the scene
    /// cannot key exactly is keyed at every frame.
    pub frame_rate: f64,
    /// Where the layer's anchor point rests when the project leaves it out,
    /// in the project's units ([`Place::source_unit`]): the middle of its
    /// source, the top-left corner of a null's, or the origin.
    pub anchor_rest: [f64; 2],
    /// The layer's size: its source's, else its composition's, which an
    /// effect's points are stored in fractions of.
    pub size: [f64; 2],
    /// Where the layer's own origin sits in the box of `size` its effects'
    /// points are measured in: the top left for a layer with a source, the
    /// middle for a shape or text layer, whose content is drawn about its
    /// origin in a box the size of its composition.
    pub origin: [f64; 2],
    /// What one unit of the geometry the project stores in the layer's own
    /// space spans, in px: its source's size, as the project keeps a
    /// layer's mask paths and anchor point in fractions of its source, or
    /// 1 on a layer without a source, whose masks and anchor are in px.
    pub source_unit: [f64; 2],
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
    /// The lines the project laid its text out in, for a text layer.
    pub lines: Option<Vec<ComposedLine>>,
    /// Where it holds values the project stores, for an export.
    pub marks: Marks,
    /// The keys of items of the same kind that go before it, each a step of
    /// one effect the scene writes as several, such as a levels effect's
    /// channels before its RGB levels.
    pub before: Vec<Map<String, Json>>,
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

    /// The object, with its tracks under `anim` when it has any, and the
    /// marks of where it holds values the project stores.
    pub(super) fn finish(mut self) -> Map<String, Json> {
        if !self.anim.is_empty() {
            self.keys.insert("anim".into(), Json::Object(self.anim));
        }
        self.marks.write(&mut self.keys);
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
                // How a property applies, while that property is 0, changes nothing.
                Some(Row {
                    to: To::OnlyDefaultWith(_, other),
                    ..
                }) if properties
                    .iter()
                    .find(|p| p.match_name == *other)
                    .is_none_or(|p| always_zero(&p.value)) =>
                {
                    self.mapping.ignore(
                        &property.match_name,
                        "it says how a value applies that is 0 or left out",
                    );
                }
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
        let baked = self.evaluated(property, place);
        if baked.is_some() && !matches!(row.to, To::Prop(_) | To::Split(..)) {
            self.mapping.unsupported(
                &place.at,
                "an expression on a property the scene does not animate (its value is kept)",
            );
        }
        match row.to {
            To::Ignore(why) => self.mapping.ignore(name, why),
            To::OnlyDefault(why) | To::OnlyDefaultWith(why, _) => {
                match self.is_default(&property.value, row, place) {
                    true => self.mapping.ignore(name, "at its default"),
                    false => {
                        self.mapping.count(name, Fate::Unsupported);
                        self.mapping.unsupported(&place.at, why);
                    }
                }
            }
            To::Prop(target) => {
                self.put_value(row, (property, baked), (target, None), sink, out, place);
            }
            To::Split(point, depth) => {
                self.put_value(
                    row,
                    (property, baked),
                    (point, Some(depth)),
                    sink,
                    out,
                    place,
                );
            }
            To::Input => {
                let named = match &property.value {
                    Stream::Static(Value::Layer(id)) => Some(*id),
                    _ => None,
                };
                let key = sink.kind.and_then(|kind| kind.input).map(|input| input.key);
                let scope = self.layer_scopes.last();
                // The layer itself, or none, is what the effect reads anyway.
                let other = named.filter(|id| scope.is_none_or(|(_, own)| own != id));
                match (other, key) {
                    (None, _) => self.mapping.count(name, Fate::Defaulted),
                    (Some(id), Some(key)) => match scope.and_then(|(ids, _)| ids.get(&id)) {
                        Some(layer) => {
                            out.keys.insert(key.into(), json!(layer));
                            self.mapping.count(name, Fate::Mapped);
                        }
                        None => {
                            self.mapping.count(name, Fate::Unsupported);
                            self.mapping.unsupported(
                                &place.at,
                                "a layer parameter naming a layer outside its composition",
                            );
                        }
                    },
                    (Some(_), None) => self.mapping.ignore(name, "a layer the scene does not read"),
                }
            }
            To::Outline => {
                let key = sink.kind.and_then(|kind| kind.outline);
                match (&property.value, key) {
                    (Stream::Static(Value::Mask(mask)), Some(key)) => {
                        match self.mask_ids.get(mask) {
                            Some(id) => {
                                out.keys.insert(key.into(), json!(id));
                                self.mapping.count(name, Fate::Mapped);
                            }
                            None => {
                                self.mapping.count(name, Fate::Unsupported);
                                self.mapping.unsupported(
                                    &place.at,
                                    "a path parameter naming a mask its layer does not have",
                                );
                            }
                        }
                    }
                    (Stream::Static(Value::Mask(_)), None) => {
                        self.mapping
                            .ignore(name, "a mask the scene does not follow");
                    }
                    // Its value without a mask is 0: the line or circle of
                    // the effect's points.
                    (Stream::Static(_), _) => self.mapping.count(name, Fate::Defaulted),
                    (Stream::Animated(_), _) => {
                        self.mapping.count(name, Fate::Unsupported);
                        self.mapping
                            .unsupported(&place.at, "a path parameter keyed to other masks");
                    }
                }
            }
            To::Curves => {
                let first = match &property.value {
                    Stream::Static(value) => Some(value),
                    Stream::Animated(keys) => {
                        self.mapping.unsupported(
                            &place.at,
                            "animated curves (the first keyframe's are kept)",
                        );
                        keys.first().map(|key| &key.value)
                    }
                };
                let curves = match first {
                    Some(Value::Data(data)) => curves::read(data),
                    _ => None,
                };
                let Some(curves) = curves else {
                    // The kind's key holds `rgb`: straight, the effect
                    // changes nothing.
                    out.keys.insert("rgb".into(), json!(curves::path(&[])));
                    self.mapping.count(name, Fate::Unsupported);
                    return self.mapping.unsupported(
                        &place.at,
                        "curves kept in a layout the import does not read",
                    );
                };
                // The kind's key holds `rgb`, so it is written even straight.
                for (curve, points) in curves::NAMES.iter().zip(&curves) {
                    if *curve == "rgb" || !curves::is_straight(points) {
                        out.keys
                            .insert((*curve).into(), json!(curves::path(points)));
                    }
                }
                self.mapping.count(name, Fate::Mapped);
            }
            To::Levels => {
                let first = match &property.value {
                    Stream::Static(value) => Some(value),
                    Stream::Animated(keys) => {
                        self.mapping.unsupported(
                            &place.at,
                            "animated levels (the first keyframe's are kept)",
                        );
                        keys.first().map(|key| &key.value)
                    }
                };
                let sets = match first {
                    Some(Value::Data(data)) => levels::read(data),
                    _ => None,
                };
                let Some([rgb, channels @ ..]) = sets else {
                    self.mapping.count(name, Fate::Unsupported);
                    return self.mapping.unsupported(
                        &place.at,
                        "levels kept in a layout the import does not read",
                    );
                };
                let set_keys = |set: &levels::Set| -> Map<String, Json> {
                    levels::NAMES
                        .iter()
                        .zip(set.iter().zip(levels::IDENTITY))
                        .filter(|(_, (value, rest))| **value != *rest)
                        .map(|(name, (value, _))| ((*name).to_owned(), json!(value)))
                        .collect()
                };
                // Each channel first, then RGB, as the effect applies them.
                for (channel, set) in levels::CHANNELS[1..].iter().zip(&channels) {
                    if *set != levels::IDENTITY {
                        let mut keys = set_keys(set);
                        keys.insert("channel".into(), json!(channel));
                        out.before.push(keys);
                    }
                }
                out.keys.extend(set_keys(&rgb));
                self.mapping.count(name, Fate::Mapped);
            }
            To::Setting(setting, choices) => {
                let first = match &property.value {
                    Stream::Static(value) => Some(value),
                    // Keys that all hold one value hold it all the time.
                    Stream::Animated(keys) if one_value(keys).is_some() => one_value(keys),
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

    /// Writes `property` to `target`, and its depth to `depth` when the row
    /// splits a 3D vector: what its expression gave where it was `baked`,
    /// else its value or keys.
    fn put_value(
        &mut self,
        row: &Row,
        (property, baked): (&Property, Option<&Bake>),
        (target, depth): (&str, Option<&str>),
        sink: Sink,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let name = row.match_name;
        let convert = convert(row, place);
        let found = sink.registry.target(target);
        let spatial = found.is_some_and(Target::moves_in_space);
        if let Some(bake) = baked {
            let depth = depth.and_then(|d| Some((d, sink.registry.target(d)?)));
            if let Some(mut tracks) =
                self.baked_tracks(bake, property, (target, found), depth, convert, place)
            {
                self.follow_slot(&bake.source, convert, &mut tracks);
                let source = self.source(Writable::Expression);
                for track in tracks {
                    let (name, component) = (&track.name, track.component);
                    insert(out, sink, name, component, track.written, Some(source));
                }
                return self.mapping.count(name, Fate::Mapped);
            }
        }
        let parts = match depth {
            None => written(&property.value, convert, spatial, place.clock).map(|w| (w, None)),
            Some(_) => split(
                &property.value,
                convert,
                spatial,
                place.clock,
                place.frame_rate,
            ),
        };
        let Some((point, in_depth)) = parts else {
            self.mapping.count(name, Fate::Unsupported);
            return self
                .mapping
                .unsupported(&place.at, "a value of a type its row does not take");
        };
        self.place_written(point, (target, convert), property, sink, out, place);
        if let (Some(depth), Some(written)) = (depth, in_depth) {
            self.place_written(written, (depth, Convert::Depth), property, sink, out, place);
        }
        self.mapping.count(name, Fate::Mapped);
    }

    /// Puts `written`, read from `property` through `convert`, at `target`:
    /// clamped into the property's range, following the slot of a panel
    /// control that drives it, and marked with how it goes back.
    fn place_written(
        &mut self,
        mut written: Written,
        (target, convert): (&str, Convert),
        property: &Property,
        sink: Sink,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let found = sink.registry.target(target);
        let component = found.filter(|t| t.component.is_some());
        let expression = property.expression.as_ref().is_some_and(|e| e.enabled);
        if let Some(range) = found.and_then(|t| t.prop.spec().range) {
            let clamped = written.clamp(range);
            if clamped.count > 0 {
                let between = match clamped.between_keys {
                    true => {
                        "; the motion between those keys can differ, as the original clamps after it interpolates"
                    }
                    false => "",
                };
                self.mapping.clamped(
                    &place.at,
                    format!(
                        "{} outside {}{between}",
                        plural(clamped.count, "value"),
                        range_text(range)
                    ),
                );
            }
        }
        // A still value a panel control drives follows its slot, and goes
        // back through the slot. A control drives a point, not its depth.
        let followed = match &mut written {
            Written::Value(value) if !expression && convert != Convert::Depth => self
                .follow_panel(&Held::Stored(address(property)), value, Some(convert))
                .map(|marked| *value = marked)
                .is_some(),
            _ => false,
        };
        let writable = match (&written, &property.value, &property.origin) {
            _ if followed => None,
            _ if expression => Some(Writable::Expression),
            (Written::Track { .. }, ..) => Some(Writable::Keyed),
            (Written::Value(_), Stream::Static(value), Some(origin)) => Some(Writable::Static {
                origin: origin.clone(),
                convert,
                template: value.clone(),
            }),
            _ => None,
        };
        let source = writable.map(|w| self.source(w));
        insert(out, sink, target, component, written, source);
    }

    /// Writes the default of a row the project leaves out, where the scene's
    /// own rest differs from it.
    fn put_default(&mut self, row: &Row, sink: Sink, out: &mut Out, place: &Place<'_>) {
        let (target, depth) = match row.to {
            To::Prop(target) => (target, None),
            To::Split(point, depth) => (point, Some(depth)),
            To::Setting(setting, choices) => {
                return self.put_default_setting(row, (setting, choices), sink, out, place);
            }
            _ => return,
        };
        let Some(value) = default_value(row, place) else {
            return;
        };
        let Some(json) = convert(row, place).value(&value) else {
            return;
        };
        self.mapping.count(row.match_name, Fate::Defaulted);
        // A default with depth writes it where it is not the scene's rest.
        if let Some((depth, at)) = depth.zip(Convert::Depth.value(&value)) {
            let differs = sink
                .registry
                .target(depth)
                .is_none_or(|target| rest_json(target).is_none_or(|rest| rest != at));
            if differs {
                out.keys.insert(depth.into(), at);
            }
        }
        // A transform value the project leaves out at its default can still
        // be driven by a panel control: it follows the slot, written even
        // at rest.
        let held = self
            .writing
            .map(|(_, layer)| Held::Default(address(layer), row.match_name.to_owned()));
        if let Some(marked) =
            held.and_then(|held| self.follow_panel(&held, &json, Some(convert(row, place))))
        {
            out.keys.insert(target.into(), marked);
            return;
        }
        let differs = sink
            .registry
            .target(target)
            .is_none_or(|target| rest_json(target).is_none_or(|rest| rest != json));
        if differs {
            out.keys.insert(target.into(), json);
        }
    }

    /// Writes the default of a setting the project leaves out, the choice
    /// its effect's definition gives, where the scene's own default differs.
    fn put_default_setting(
        &mut self,
        row: &Row,
        (setting, choices): (&str, &[(i64, &'static str)]),
        sink: Sink,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let Some(Value::Number(code)) = default_value(row, place) else {
            return;
        };
        let Some((_, choice)) = choices.iter().find(|(c, _)| *c == code.round() as i64) else {
            self.mapping.count(row.match_name, Fate::Unsupported);
            return self
                .mapping
                .unsupported(&place.at, format!("a `{setting}` the scene does not have"));
        };
        self.mapping.count(row.match_name, Fate::Defaulted);
        let rest = sink
            .kind
            .and_then(|kind| kind.setting(setting))
            .map(|spec| spec.default);
        if rest != Some(*choice) {
            out.keys.insert(setting.into(), json!(choice));
        }
    }

    /// Whether `stream` holds the row's default all the time. A row whose
    /// default is unknown holds it only where the project leaves it out.
    fn is_default(&self, stream: &Stream, row: &Row, place: &Place<'_>) -> bool {
        let value = match stream {
            Stream::Static(value) => Some(value),
            Stream::Animated(keys) => one_value(keys),
        };
        let (Some(value), Some(default)) = (value, default_value(row, place)) else {
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
            Some(kind) => self.item(kind, group, &here),
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

    /// The item of `kind` from `group`, after the items of the steps its
    /// effect takes first, if the scene writes it as several.
    fn item(&mut self, kind: &'static Kind, group: &Group, place: &Place<'_>) -> Vec<Json> {
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
        // An expression selector's amount becomes its parts.
        if let Some(at) = properties
            .iter()
            .position(|p| p.match_name == crate::parts::AMOUNT)
        {
            let amount = properties.remove(at);
            self.mapping.count(&amount.match_name, Fate::Mapped);
            let here = place.inside(&amount.match_name);
            // One that cannot be baked keeps no parts, and chooses nothing.
            if let Some((by, parts)) = self.part_items(amount, &here) {
                out.keys.insert("by".into(), json!(by));
                sub_items = parts;
            }
        }
        self.rows(
            &properties,
            kind.rows_for(&properties),
            sink,
            &mut out,
            &place,
        );
        let primary = match scene_kind.primary {
            Primary::Items(_) => Json::Array(sub_items),
            Primary::Property(name) => out
                .keys
                .remove(name)
                .inspect(|_| out.marks.rename(name, kind.item))
                .or_else(|| sink.registry.target(name).and_then(rest_json))
                .unwrap_or(Json::Null),
            Primary::Setting(name) => out
                .keys
                .remove(name)
                .unwrap_or_else(|| json!(scene_kind.setting(name).map(|s| s.default))),
        };
        let before = std::mem::take(&mut out.before);
        let mut keys = Map::new();
        keys.insert(kind.item.into(), primary);
        keys.extend(out.finish());
        if let Some(settings) = group.mask {
            self.mask(settings, &mut keys, &place);
        }
        // A step before shares the item's switch, which turns the whole
        // effect off, and writes its primary key as the item does.
        let steps = before.into_iter().map(|mut step| {
            let primary = match scene_kind.primary {
                Primary::Property(name) => step
                    .remove(name)
                    .or_else(|| sink.registry.target(name).and_then(rest_json))
                    .unwrap_or(Json::Null),
                _ => Json::Null,
            };
            let mut keys = Map::new();
            keys.insert(kind.item.into(), primary);
            keys.extend(step);
            keys
        });
        let mut items: Vec<_> = steps.chain([keys]).collect();
        if !group.enabled {
            for keys in &mut items {
                keys.insert("hidden".into(), json!(true));
            }
        }
        items.into_iter().map(Json::Object).collect()
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
            Some(Policy::Control) => {
                self.mapping
                    .ignore(name, "an expression control, which draws nothing");
                self.control(group, place);
            }
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
            // Its layer reports drawing nothing in its place.
            Some(Policy::Through | Policy::InPlace) | None => {
                self.mapping.count(name, Fate::Unsupported);
                self.mapping.unsupported(&place.at, format!("`{name}`"));
            }
        }
    }
}

impl Walk<'_> {
    /// Records an expression control with its values.
    fn control(&mut self, group: &Group, place: &Place<'_>) {
        let definition = self.project.effects.get(&group.match_name);
        let name = group
            .name
            .clone()
            .or_else(|| definition.map(|d| d.name.clone()))
            .unwrap_or_else(|| group.match_name.clone());
        let stored = |p: &Property| match &p.value {
            Stream::Static(value) => serde_json::to_value(value),
            Stream::Animated(keys) => serde_json::to_value(keys),
        };
        let values = match definition {
            // Every parameter, those the project leaves at their default too.
            Some(definition) => definition
                .parameters
                .iter()
                .filter(|d| !d.match_name.ends_with("-0000"))
                .map(|d| {
                    let value = match group.properties().find(|p| p.match_name == d.match_name) {
                        Some(p) => stored(p),
                        None => serde_json::to_value(&d.control).map(|c| json!({ "default": c })),
                    };
                    (d.name.clone(), value.unwrap_or(Json::Null))
                })
                .collect(),
            None => group
                .properties()
                .map(|p| (p.match_name.clone(), stored(p).unwrap_or(Json::Null)))
                .collect(),
        };
        self.mapping.controls.push(ControlReport {
            at: place.at.clone(),
            name,
            kind: group.match_name.clone(),
            values,
        });
    }
}

/// Puts a track for `name` into `out`, with the resting value its first key
/// gives where the scene needs one: a component's, or a kind's primary
/// property's.
fn insert(
    out: &mut Out,
    sink: Sink,
    name: &str,
    component: Option<Target>,
    written: Written,
    source: Option<usize>,
) {
    // Where the resting value is: a component of a whole value, or the key.
    let rest = match component {
        Some(c) => format!("{}/{}", c.prop.name(), c.component.unwrap_or(0)),
        None => name.to_owned(),
    };
    let (marked, rest_written) = match written {
        Written::Value(value) => {
            match component {
                Some(component) => out.set_component(component, value),
                None => {
                    out.keys.insert(name.into(), value);
                }
            }
            (vec![rest], true)
        }
        Written::Track { keys, first } => {
            out.anim.insert(name.into(), keys);
            let rest_written = match component {
                Some(component) => {
                    out.set_component(component, first);
                    true
                }
                None if primary_property(sink.kind) == Some(name) => {
                    out.keys.insert(name.into(), first);
                    true
                }
                None => false,
            };
            (vec![format!("anim/{name}"), rest], rest_written)
        }
    };
    let Some(source) = source else { return };
    for (i, path) in marked.into_iter().enumerate() {
        // A track's resting value is marked only where it was written.
        if i == 0 || rest_written {
            out.marks.add(path, source);
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
        nakkash_format::Value::Number(n) => num(*n),
        nakkash_format::Value::Point([x, y]) => json!([num(*x), num(*y)]),
        nakkash_format::Value::Color([r, g, b, a]) => json!([num(*r), num(*g), num(*b), num(*a)]),
        _ => return None,
    };
    match (target.component, whole) {
        (Some(i), Json::Array(parts)) => parts.get(i).cloned(),
        (_, whole) => Some(whole),
    }
}

/// How `row`'s values become the scene's at `place`: an effect's point
/// parameter is stored in fractions of the layer's effect space, so it
/// becomes pixels of that space, measured from the layer's origin; a value
/// the row keeps in fractions of the layer's source (a mask's path, a
/// layer's anchor point) becomes pixels of the source.
fn convert(row: &Row, place: &Place<'_>) -> Convert {
    let point = place
        .effect
        .and_then(|effect| effect.parameter(row.match_name))
        .is_some_and(|parameter| matches!(parameter.control, Control::Point { .. }));
    match (point, row.convert) {
        (true, _) => Convert::Sized {
            size: place.size,
            origin: place.origin,
        },
        (false, Convert::PathIn(_)) => Convert::PathIn(place.source_unit),
        (false, Convert::Sized { origin, .. }) => Convert::Sized {
            size: place.source_unit,
            origin,
        },
        (false, convert) => convert,
    }
}

/// A row's default as a project value, if it has one here.
fn default_value(row: &Row, place: &Place<'_>) -> Option<Value> {
    Some(match row.default {
        Missing::Absent => return None,
        Missing::Number(n) => Value::Number(n),
        Missing::Vector(v) => Value::Vector2(v),
        Missing::Color([r, g, b, a]) => Value::Color(crate::model::Color { r, g, b, a }),
        Missing::CompCentre => Value::Vector2(place.comp_size.map(|n| n / 2.0)),
        Missing::CameraZoom => Value::Number(default_zoom(place.comp_size[0])),
        Missing::CameraPosition => {
            let [width, height] = place.comp_size;
            Value::Vector3([width / 2.0, height / 2.0, -default_zoom(width)])
        }
        Missing::SourceCentre => Value::Vector2(place.anchor_rest),
        Missing::Parameter => {
            let parameter = place.effect?.parameter(row.match_name)?;
            match &parameter.control {
                Control::Slider { default, .. } | Control::Angle { default } => {
                    Value::Number(*default)
                }
                Control::Checkbox { default } => Value::Number(f64::from(u8::from(*default))),
                Control::Popup { default, .. } => Value::Number(f64::from(*default)),
                Control::Color { default } => Value::Color(*default),
                Control::Point { default } => Value::Vector2([
                    default.first().copied().unwrap_or(0.5),
                    default.get(1).copied().unwrap_or(0.5),
                ]),
                Control::Layer | Control::Other { .. } => return None,
            }
        }
    })
}

/// `n` and `noun`, in the plural when `n` is not one.
pub(super) fn plural(n: usize, noun: &str) -> String {
    match n {
        1 => format!("1 {noun}"),
        _ => format!("{n} {noun}s"),
    }
}

/// A property's range as the format writes it: `0..=1`, or `0..` with no top.
pub(super) fn range_text((lo, hi): (f64, f64)) -> String {
    match hi {
        f64::INFINITY => format!("{}..", nakkash_format::fmt_num(lo)),
        _ => format!(
            "{}..={}",
            nakkash_format::fmt_num(lo),
            nakkash_format::fmt_num(hi)
        ),
    }
}

/// The one value every key of `keys` holds, if they all hold the same.
fn one_value(keys: &[Keyframe]) -> Option<&Value> {
    let first = &keys.first()?.value;
    keys.iter().all(|key| key.value == *first).then_some(first)
}

/// Whether `stream` holds the number 0 all the time.
fn always_zero(stream: &Stream) -> bool {
    let value = match stream {
        Stream::Static(value) => Some(value),
        Stream::Animated(keys) => one_value(keys),
    };
    matches!(value, Some(Value::Number(n)) if *n == 0.0)
}
