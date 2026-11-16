//! One layer: what it draws, its switches and timing, its transform and its
//! effects. A layer whose content the scene cannot draw yet becomes a
//! `pivot`, so its children keep their parent and its time stays its own.

use std::collections::HashMap;

use motion_format::Address;
use motion_format::property::LAYER;
use serde_json::{Value as Json, json};

use super::footage::{Media, media};
use super::items::{Out, Place, Sink};
use super::keys::{Clock, num};
use super::report::Fate;
use super::rows::{
    CAMERA_AIMED, CAMERA_FREE, CAMERA_OPTIONS, LAYER_ROOT, LIGHT_TRANSFORM, Row, TRANSFORM,
    TRANSFORM_3D,
};
use super::{StoredText, Walk};
use crate::model::{
    AutoOrient, BlendMode, Child, Composition, Footage, Group, Item, ItemKind, Layer, LayerKind,
    Property, Source, Stream, TrackMatte, Value,
};
use motion_format::shorthand::MatteMode;

/// The scene's matte mode for a project's track matte, none without one,
/// or the file's code for a kind the scene has no mode for.
fn matte_mode(matte: TrackMatte) -> Result<Option<MatteMode>, u8> {
    match matte {
        TrackMatte::None => Ok(None),
        TrackMatte::Alpha => Ok(Some(MatteMode::Alpha)),
        TrackMatte::AlphaInverted => Ok(Some(MatteMode::AlphaInverted)),
        TrackMatte::Luma => Ok(Some(MatteMode::Luma)),
        TrackMatte::LumaInverted => Ok(Some(MatteMode::LumaInverted)),
        TrackMatte::Other(code) => Err(code),
    }
}

/// Why a layer of controls is left out of the scene.
pub(super) const DROPPED: &str =
    "a layer of expression controls only, which draws nothing (left out)";

/// Whether `layer` is left out of the scene: its effects are all controls,
/// whose reads are slots or baked, it draws nothing, and no layer of
/// `composition` has it as its parent or its matte.
pub(super) fn is_dropped(layer: &Layer, composition: &Composition) -> bool {
    let switches = &layer.switches;
    let draws_nothing = layer.kind == LayerKind::Av
        && (switches.null || switches.guide || switches.adjustment || !switches.enabled);
    draws_nothing
        && super::slots::is_control_layer(layer)
        && !composition.layers.iter().any(|other| {
            other.parent == Some(layer.id)
                || (other.track_matte != TrackMatte::None
                    && matte_layer(other, composition) == Some(layer.id))
        })
}

/// The id of `layer`'s matte layer: the one its file names, or in files
/// that name none, the layer just above it.
fn matte_layer(layer: &Layer, composition: &Composition) -> Option<u32> {
    layer.matte_layer.or_else(|| {
        let index = composition.layers.iter().position(|l| l.id == layer.id)?;
        Some(composition.layers.get(index.checked_sub(1)?)?.id)
    })
}

/// The match names of the root groups a layer's content comes from.
const TRANSFORM_GROUP: &str = "ADBE Transform Group";
const VECTORS: &str = "ADBE Root Vectors Group";
const TEXT: &str = "ADBE Text Properties";
pub(super) const EFFECTS: &str = "ADBE Effect Parade";
pub(super) const MASKS: &str = "ADBE Mask Parade";
const AUDIO_GROUP: &str = "ADBE Audio Group";
const AUDIO_LEVELS: &str = "ADBE Audio Levels";
/// A layer's time remapping.
const TIME_REMAPPING: &str = "ADBE Time Remapping";
const CAMERA_OPTIONS_GROUP: &str = "ADBE Camera Options Group";

/// The scene key of an audio layer's file.
const AUDIO: &str = "audio";

/// The loudest a `volume` plays, as a gain: +12 dB.
const LOUDEST: f64 = 4.0;

impl<'p> Walk<'p> {
    /// The scene layer for `layer`, in `composition` called `comp_name`,
    /// whose layers have the ids in `ids`.
    pub(super) fn layer(
        &mut self,
        layer: &'p Layer,
        composition: &'p Composition,
        comp_name: &str,
        ids: &HashMap<u32, String>,
    ) -> Json {
        self.writing = Some((composition, layer));
        let source = layer.source.and_then(|id| self.items.get(&id).copied());
        let half = |w: f64, h: f64| [w / 2.0, h / 2.0];
        let collapsed =
            layer.kind == LayerKind::Av && layer.switches.collapse && !layer.switches.adjustment;
        let place = Place {
            at: format!("{comp_name}/{}", layer.name),
            clock: Clock {
                start: layer.start_time,
                stretch: layer.stretch,
            },
            comp_size: [f64::from(composition.width), f64::from(composition.height)],
            frame_rate: composition.frame_rate,
            // A null's anchor rests at its top-left corner, not its centre,
            // so the children it moves keep their place; the anchor of a
            // layer with a source is kept in fractions of it.
            anchor_rest: match source.map(|item| &item.kind) {
                _ if layer.switches.null => [0.0, 0.0],
                Some(ItemKind::Composition(_) | ItemKind::Footage(_)) => [0.5, 0.5],
                _ => [0.0, 0.0],
            },
            // A collapsed composition's effects run over the composition it
            // sits in, in that composition's coordinates.
            size: match source.map(|item| &item.kind) {
                Some(ItemKind::Composition(_)) if collapsed => {
                    [f64::from(composition.width), f64::from(composition.height)]
                }
                Some(ItemKind::Composition(c)) => [f64::from(c.width), f64::from(c.height)],
                Some(ItemKind::Footage(f)) => [f64::from(f.width), f64::from(f.height)],
                _ => [f64::from(composition.width), f64::from(composition.height)],
            },
            source_unit: match source.map(|item| &item.kind) {
                Some(ItemKind::Composition(c)) => [f64::from(c.width), f64::from(c.height)],
                Some(ItemKind::Footage(f)) => [f64::from(f.width), f64::from(f.height)],
                _ => [1.0, 1.0],
            },
            origin: match (source, layer.kind) {
                (None, LayerKind::Shape | LayerKind::Text) => {
                    half(f64::from(composition.width), f64::from(composition.height))
                }
                _ => [0.0, 0.0],
            },
            effect: None,
        };
        let mut out = Out::default();
        out.keys.insert("id".into(), json!(ids[&layer.id]));
        let replaced = replacing_effect(layer);
        self.content(layer, source, replaced, &mut out, &place);
        // A layer that plays a file, and one that may play its
        // composition's sound.
        let plays = out.keys.contains_key(AUDIO);
        let hears = plays || out.keys.contains_key("comp");
        let sink = Sink {
            registry: &LAYER,
            kind: None,
        };
        // The transform's rows run even without the group, which a project
        // leaves out when every property in it is at its default.
        let transform: Vec<&Property> = layer
            .properties
            .children
            .iter()
            .filter_map(|child| match child {
                Child::Group(group) if group.match_name == TRANSFORM_GROUP => Some(group),
                _ => None,
            })
            .flat_map(Group::properties)
            .collect();
        // A sound has no picture to place.
        match plays {
            true => self.mapping.ignore(
                TRANSFORM_GROUP,
                "the transform of a layer that only plays a sound",
            ),
            false => {
                self.mapping.count(TRANSFORM_GROUP, Fate::Mapped);
                self.rows(
                    &transform,
                    transform_rows(layer),
                    sink,
                    &mut out,
                    &place.inside(TRANSFORM_GROUP),
                );
            }
        }
        let mut root = Vec::new();
        // The masks its effects follow get ids before either is written.
        let outer = std::mem::replace(&mut self.mask_ids, super::masks::followed(layer));
        layer
            .properties
            .children
            .iter()
            .for_each(|child| match child {
                Child::Property(property) => root.push(property),
                Child::Group(group) => {
                    let here = place.inside(&group.match_name);
                    match group.match_name.as_str() {
                        TRANSFORM_GROUP => {}
                        VECTORS | TEXT if replaced.is_some() => self.mapping.ignore(
                            &group.match_name,
                            "what a layer draws where an effect draws in its place",
                        ),
                        VECTORS | TEXT if adjusts_nothing(layer) => self.mapping.ignore(
                            &group.match_name,
                            "the content of an adjustment layer with no effect the scene \
                             draws, which changes nothing",
                        ),
                        VECTORS if layer.kind == LayerKind::Shape => {
                            self.mapping.count(VECTORS, Fate::Mapped);
                            let shapes = self.items(group, &here);
                            out.keys.insert("shapes".into(), Json::Array(shapes));
                        }
                        TEXT if layer.kind == LayerKind::Text => self.text(group, &mut out, &here),
                        MASKS => {
                            self.mapping.count(MASKS, Fate::Mapped);
                            let masks = self.items(group, &here);
                            if !masks.is_empty() {
                                out.keys.insert("mask".into(), Json::Array(masks));
                            }
                        }
                        EFFECTS if plays => {
                            if !group.children.is_empty() {
                                self.mapping.count(EFFECTS, Fate::Unsupported);
                                self.mapping.unsupported(
                                    &here.at,
                                    "effects on a sound (played without them)",
                                );
                            }
                        }
                        AUDIO_GROUP if hears => self.levels(group, composition, &mut out, &here),
                        CAMERA_OPTIONS_GROUP if layer.kind == LayerKind::Camera => {}
                        EFFECTS => {
                            self.mapping.count(EFFECTS, Fate::Mapped);
                            let effects = self.items(group, &here);
                            if !effects.is_empty() && replaced.is_none() {
                                out.keys.insert("fx".into(), Json::Array(effects));
                            }
                        }
                        _ => self.group(group, &here),
                    }
                }
            });
        self.mask_ids = outer;
        self.rows(&root, &LAYER_ROOT, sink, &mut out, &place);
        // Only a composition or a clip plays in time; a still, a solid or a
        // null shows the same whatever time it is remapped to.
        let timed = out.keys.contains_key("comp") || out.keys.contains_key("video");
        if !timed && (out.keys.remove("remap").is_some() | out.anim.remove("remap").is_some()) {
            self.mapping.ignore(
                TIME_REMAPPING,
                "on a layer whose content does not play in time",
            );
        }
        // A camera's options run even without the group, for its zoom.
        if layer.kind == LayerKind::Camera {
            let options: Vec<&Property> = layer
                .properties
                .children
                .iter()
                .filter_map(|child| match child {
                    Child::Group(group) if group.match_name == CAMERA_OPTIONS_GROUP => Some(group),
                    _ => None,
                })
                .flat_map(Group::properties)
                .collect();
            self.mapping.count(CAMERA_OPTIONS_GROUP, Fate::Mapped);
            let here = place.inside(CAMERA_OPTIONS_GROUP);
            self.rows(&options, &CAMERA_OPTIONS, sink, &mut out, &here);
        }
        // Any layer can adjust: its content, a solid's, a shape's or a
        // text's, says where its effects apply to what is below it.
        if layer.switches.adjustment
            && out.keys.contains_key("fx")
            && !out.keys.contains_key("pivot")
        {
            out.keys.insert("adjust".into(), json!(true));
            self.mapping.count("layer: adjustment", Fate::Mapped);
        }
        self.switches(layer, composition, ids, &mut out, &place);
        self.audio_switch(layer, source, plays, &mut out);
        self.timing(layer, composition, &mut out);
        if let Some(lines) = out.lines.take() {
            let id = ids[&layer.id].parse().expect("the import writes valid ids");
            self.texts.push(StoredText {
                layer: Address::written(self.scope.as_deref(), id),
                lines,
            });
        }
        Json::Object(out.finish())
    }

    /// What the layer draws; a pivot, reported, for what the scene cannot
    /// draw yet.
    fn content(
        &mut self,
        layer: &Layer,
        source: Option<&Item>,
        replaced: Option<&str>,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let pivot = |walk: &mut Walk<'_>, out: &mut Out, why: Option<&str>| {
            out.keys.insert("pivot".into(), json!(true));
            if let Some(why) = why {
                walk.mapping
                    .count(&format!("layer: {why}"), Fate::Unsupported);
                walk.mapping
                    .unsupported(&place.at, format!("{why} (drawn as nothing)"));
            }
        };
        if let Some(effect) = replaced {
            return pivot(
                self,
                out,
                Some(&format!("layers whose `{effect}` draws in their place")),
            );
        }
        match layer.kind {
            // An adjustment layer applies its effects to what is below it,
            // where its content draws; without an effect the scene draws,
            // it changes nothing.
            LayerKind::Av | LayerKind::Shape | LayerKind::Text if adjusts_nothing(layer) => {
                pivot(self, out, None)
            }
            LayerKind::Shape => {
                out.keys.insert("shapes".into(), json!([]));
            }
            LayerKind::Text => {
                out.keys.insert("text".into(), json!(""));
            }
            LayerKind::Av if layer.switches.null => pivot(self, out, None),
            LayerKind::Av => match source.map(|item| (item, &item.kind)) {
                Some((item, ItemKind::Composition(_))) => {
                    let comp = self.comp_id(item);
                    out.keys.insert("comp".into(), json!(comp));
                    // On a vector layer the switch changes nothing we draw,
                    // and an adjustment layer draws nothing to collapse.
                    if layer.switches.collapse && !layer.switches.adjustment {
                        out.keys.insert("collapse".into(), json!(true));
                    }
                }
                Some((_, ItemKind::Footage(footage))) => self.footage(footage, out, place, pivot),
                Some((_, ItemKind::Folder)) => pivot(self, out, None),
                // The layer names a source the project does not hold.
                None if layer.source.is_some() => {
                    pivot(self, out, Some("a source the project does not hold"))
                }
                None => pivot(self, out, None),
            },
            LayerKind::Camera => {
                out.keys.insert("camera".into(), json!(true));
                self.mapping.count("layer: camera", Fate::Mapped);
            }
            LayerKind::Light => pivot(self, out, Some("lights")),
            other => pivot(self, out, Some(&format!("{other} layers"))),
        }
    }

    /// A footage layer: a solid is a rectangle of its colour, a still
    /// picture that was found an image filling the footage's size, a found
    /// clip with a decoder a video filling it, and a found sound with a
    /// decoder an audio layer. Other footage is a pivot, reported with why.
    fn footage(
        &mut self,
        footage: &Footage,
        out: &mut Out,
        place: &Place<'_>,
        pivot: impl Fn(&mut Walk<'_>, &mut Out, Option<&str>),
    ) {
        match &footage.source {
            Source::Solid { color } => {
                out.keys.insert(
                    "rect".into(),
                    json!(format!("{}x{}", footage.width, footage.height)),
                );
                out.keys
                    .insert("fill".into(), super::keys::color_json(color));
                self.mapping.count("footage: solid", Fate::Mapped);
            }
            Source::File { path: Some(stored) } => {
                let (key, path) = match media(footage, stored, self.files) {
                    Media::Still(path) => ("image", path),
                    Media::Clip(path) => ("video", path),
                    // A sound draws nothing, so it has no box.
                    Media::Sound(path) => {
                        out.keys.insert(AUDIO.into(), json!(path));
                        self.mapping.count("footage: audio", Fate::Mapped);
                        return;
                    }
                    Media::Codec(codec) => {
                        return pivot(self, out, Some(&format!("video footage in {codec}")));
                    }
                    Media::SoundFormat(format) => {
                        return pivot(self, out, Some(&format!("audio footage in {format}")));
                    }
                    Media::Unsupported(why) => return pivot(self, out, Some(why)),
                };
                out.keys.insert(key.into(), json!(path));
                out.keys
                    .insert("box".into(), json!([footage.width, footage.height]));
                self.mapping.count(&format!("footage: {key}"), Fate::Mapped);
                if footage.pixel_aspect != 1.0 {
                    self.mapping
                        .unsupported(&place.at, "non-square pixels (drawn square)");
                }
            }
            Source::File { path: None } => {
                pivot(self, out, Some("footage whose file path it cannot read"))
            }
            Source::Placeholder => pivot(self, out, Some("missing footage")),
        }
    }

    /// The layer's audio levels as its `volume`, a gain of 10^(dB/20) for
    /// the levels in decibels the project keeps for left and right. The
    /// original moves between keys in decibels, which a gain does not, so
    /// moving levels are keyed every frame of the composition from the
    /// first key to the last.
    fn levels(
        &mut self,
        group: &Group,
        composition: &Composition,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let Some(levels) = group.properties().find(|p| p.match_name == AUDIO_LEVELS) else {
            return self.mapping.count(AUDIO_GROUP, Fate::Mapped);
        };
        self.mapping.count(AUDIO_LEVELS, Fate::Mapped);
        let mut uneven = false;
        let mut loud = false;
        let mut gain = |value: &Value| {
            let (left, right) = match value {
                Value::Vector2([l, r]) | Value::Vector3([l, r, _]) => (*l, *r),
                Value::Number(n) => (*n, *n),
                _ => (0.0, 0.0),
            };
            uneven |= (left - right).abs() > 1e-6;
            let gain = 10_f64.powf(f64::midpoint(left, right) / 20.0);
            loud |= gain > LOUDEST + 1e-9;
            gain.min(LOUDEST)
        };
        match &levels.value {
            Stream::Static(value) => {
                let gain = gain(value);
                if (gain - 1.0).abs() > 1e-6 {
                    out.keys.insert("volume".into(), num(gain));
                }
            }
            Stream::Animated(keys) => {
                let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
                    return;
                };
                let step = 1.0 / composition.frame_rate.max(1.0);
                let count = ((last.time - first.time) / step).ceil().max(0.0) as usize;
                let times = (0..count)
                    .map(|k| first.time + k as f64 * step)
                    .chain(std::iter::once(last.time));
                let written: Vec<Json> = times
                    .map(|t| {
                        json!({
                            "t": place.clock.time(t),
                            "v": num(gain(&levels.value.value_at(t))),
                        })
                    })
                    .collect();
                out.anim.insert("volume".into(), Json::Array(written));
            }
        }
        if levels.expression.is_some() {
            self.mapping
                .unsupported(&place.at, "an expression on audio levels (its keys play)");
        }
        if uneven {
            self.mapping.unsupported(
                &place.at,
                "different levels for left and right (both play at their mean)",
            );
        }
        if loud {
            self.mapping
                .unsupported(&place.at, "levels above +12 dB (played at +12 dB)");
        }
    }

    /// The layer's audio switch: an audio layer plays when it is on,
    /// whatever its video switch says, and a composition layer that would
    /// play a sound is muted when it is off.
    fn audio_switch(&mut self, layer: &Layer, source: Option<&Item>, plays: bool, out: &mut Out) {
        let on = layer.switches.audio;
        if plays {
            match on {
                true => out.keys.remove("hidden"),
                false => out.keys.insert("hidden".into(), json!(true)),
            };
            return;
        }
        let composition = source.and_then(|item| match &item.kind {
            ItemKind::Composition(c) => Some(c),
            _ => None,
        });
        if !on && composition.is_some_and(|c| self.sounds_in(c, 0)) {
            out.anim.remove("volume");
            out.keys.insert("volume".into(), json!(0));
        }
    }

    /// Whether `composition` plays a sound: a layer of audio footage, or of
    /// a composition that plays one, with its audio switch on.
    fn sounds_in(&self, composition: &Composition, depth: usize) -> bool {
        // Projects do not nest compositions in a loop, but a broken one
        // must not overflow the stack.
        if depth > 64 {
            return false;
        }
        composition.layers.iter().any(|layer| {
            let source = layer.source.and_then(|id| self.items.get(&id));
            layer.switches.audio
                && match source.map(|item| &item.kind) {
                    Some(ItemKind::Footage(footage)) => footage.width == 0 || footage.height == 0,
                    Some(ItemKind::Composition(c)) => self.sounds_in(c, depth + 1),
                    _ => false,
                }
        })
    }

    /// The layer's switches, parent, blending and matte.
    fn switches(
        &mut self,
        layer: &Layer,
        composition: &Composition,
        ids: &HashMap<u32, String>,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let switches = layer.switches;
        if !switches.enabled || switches.guide {
            out.keys.insert("hidden".into(), json!(true));
        }
        let mut unsupported = |what: String| self.mapping.unsupported(&place.at, what);
        match layer.parent.map(|id| ids.get(&id)) {
            Some(Some(parent)) => {
                out.keys.insert("parent".into(), json!(parent));
            }
            Some(None) => unsupported(
                "a parent that is not in its composition (drawn without a parent)".into(),
            ),
            None => {}
        }
        // A camera is always 3D, and a light is drawn as nothing in space.
        match layer.kind {
            LayerKind::Camera => {}
            LayerKind::Light => {
                out.keys.insert("3d".into(), json!(true));
            }
            _ if switches.three_d => {
                out.keys.insert("3d".into(), json!(true));
            }
            _ => {}
        }
        match (switches.orient, layer.kind) {
            (AutoOrient::Off, _)
            | (AutoOrient::TowardsPoint, LayerKind::Camera | LayerKind::Light) => {}
            (AutoOrient::AlongPath, _) => {
                unsupported("turning along the motion path (drawn as it is turned)".into());
            }
            (AutoOrient::TowardsCamera, _) if switches.three_d => {
                unsupported("turning to face the camera (drawn as it is turned)".into());
            }
            (AutoOrient::CharactersTowardCamera, _) if switches.three_d => {
                unsupported(
                    "characters turning to face the camera (drawn as they are turned)".into(),
                );
            }
            // The original ignores the setting where the layer cannot take it.
            _ => {}
        }
        // A layer's switch blurs it only where its composition's is on.
        if switches.motion_blur && composition.motion_blur.on {
            out.keys.insert("mblur".into(), json!(true));
        }
        if switches.solo {
            unsupported("solo layers".into());
        }
        match blend(layer.blend_mode) {
            Some("normal") => {}
            Some(mode) => {
                out.keys.insert("blend".into(), json!(mode));
            }
            None => unsupported(format!("the blend mode `{}`", layer.blend_mode)),
        }
        let mode = match matte_mode(layer.track_matte) {
            Ok(mode) => mode,
            Err(code) => {
                unsupported(format!("a track matte of an unknown kind ({code})"));
                None
            }
        };
        if let Some(mode) = mode {
            match matte_layer(layer, composition).and_then(|id| ids.get(&id)) {
                // The matte layer keeps its own visibility: projects switch
                // off the layers they use as mattes, which still matte.
                Some(id) => {
                    out.keys
                        .insert("matte".into(), json!(format!("{id} {}", mode.name())));
                }
                None => unsupported("a track matte without its matte layer".into()),
            }
        }
    }

    /// When the layer shows, where its own time starts and how fast it runs.
    fn timing(&mut self, layer: &Layer, composition: &Composition, out: &mut Out) {
        let start = layer.in_point.max(0.0);
        let end = layer.out_point;
        if end <= start {
            out.keys.insert("hidden".into(), json!(true));
        } else if start > 0.0 || end < composition.duration {
            out.keys.insert(
                "span".into(),
                json!(format!("{}s-{}s", num(start), num(end))),
            );
        }
        if layer.start_time != 0.0 {
            out.keys.insert(
                "offset".into(),
                json!(format!("{}s", num(layer.start_time))),
            );
        }
        if layer.stretch != 1.0 {
            out.keys.insert("stretch".into(), num(layer.stretch));
        }
    }
}

/// The rows of `layer`'s transform: a flat layer's, a 3D layer's, a
/// camera's, with its point of interest as its target where it turns
/// towards it, or a light's.
fn transform_rows(layer: &Layer) -> &'static [Row] {
    match layer.kind {
        LayerKind::Camera if layer.switches.orient == AutoOrient::TowardsPoint => &CAMERA_AIMED,
        LayerKind::Camera => &CAMERA_FREE,
        LayerKind::Light => &LIGHT_TRANSFORM,
        _ if layer.switches.three_d => &TRANSFORM_3D,
        _ => &TRANSFORM,
    }
}

/// The first effect on `layer` that draws in place of its content and that
/// the scene cannot draw yet, if any.
fn replacing_effect(layer: &Layer) -> Option<&str> {
    layer
        .properties
        .children
        .iter()
        .find_map(|child| match child {
            Child::Group(parade) if parade.match_name == EFFECTS => {
                parade.children.iter().find_map(|effect| match effect {
                    Child::Group(effect)
                        if effect.enabled
                            && !effect.hidden
                            && super::rows::kind(&effect.match_name).is_none()
                            && super::rows::REPLACING.contains(&effect.match_name.as_str()) =>
                    {
                        Some(effect.match_name.as_str())
                    }
                    _ => None,
                })
            }
            _ => None,
        })
}

/// The effects of `layer`, in order.
pub(super) fn effects(layer: &Layer) -> impl Iterator<Item = &Group> {
    layer
        .properties
        .children
        .iter()
        .filter_map(|child| match child {
            Child::Group(parade) if parade.match_name == EFFECTS => Some(parade),
            _ => None,
        })
        .flat_map(|parade| parade.children.iter())
        .filter_map(|effect| match effect {
            Child::Group(effect) => Some(effect),
            Child::Property(_) => None,
        })
}

/// Whether `layer` has an effect, switched on, that the scene draws.
/// Whether `layer` is an adjustment layer none of whose effects the scene
/// draws, so it changes nothing and draws nothing of its own.
fn adjusts_nothing(layer: &Layer) -> bool {
    layer.switches.adjustment && !draws_an_effect(layer)
}

fn draws_an_effect(layer: &Layer) -> bool {
    layer.properties.children.iter().any(|child| match child {
        Child::Group(parade) if parade.match_name == EFFECTS => {
            parade.children.iter().any(|effect| match effect {
                Child::Group(effect) => {
                    effect.enabled
                        && !effect.hidden
                        && super::rows::kind(&effect.match_name).is_some()
                }
                Child::Property(_) => false,
            })
        }
        _ => false,
    })
}

/// The scene's blend mode for the project's `mode`, if the scene has one
/// that blends the same way. The classic modes, kept from older versions
/// for their 8-bit arithmetic, blend as their current ones on the levels
/// the scene draws.
fn blend(mode: BlendMode) -> Option<&'static str> {
    Some(match mode {
        BlendMode::Normal => "normal",
        BlendMode::Add | BlendMode::LinearDodge => "add",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::SoftLight => "soft-light",
        BlendMode::HardLight => "hard-light",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::Difference | BlendMode::ClassicDifference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/map/layer.rs"]
mod tests;
