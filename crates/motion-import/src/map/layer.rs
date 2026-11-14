//! One layer: what it draws, its switches and timing, its transform and its
//! effects. A layer whose content the scene cannot draw yet becomes a
//! `pivot`, so its children keep their parent and its time stays its own.

use std::collections::HashMap;

use motion_format::property::LAYER;
use serde_json::{Value as Json, json};

use super::footage::{Media, file_name, media};
use super::items::{Out, Place, Sink};
use super::keys::{Clock, num};
use super::report::Fate;
use super::rows::{LAYER_ROOT, TRANSFORM};
use super::{StoredText, Walk};
use crate::model::{
    BlendMode, Child, Composition, Footage, Group, Item, ItemKind, Layer, LayerKind, Property,
    Source, TrackMatte,
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
const EFFECTS: &str = "ADBE Effect Parade";

impl<'p> Walk<'p> {
    /// The scene layer for `layer`, in `composition` called `comp_name`,
    /// whose layers have the ids in `ids`.
    pub(super) fn layer(
        &mut self,
        layer: &'p Layer,
        composition: &Composition,
        comp_name: &str,
        ids: &HashMap<u32, String>,
    ) -> Json {
        let source = layer.source.and_then(|id| self.items.get(&id).copied());
        let half = |w: f64, h: f64| [w / 2.0, h / 2.0];
        let place = Place {
            at: format!("{comp_name}/{}", layer.name),
            clock: Clock {
                start: layer.start_time,
                stretch: layer.stretch,
            },
            comp_centre: half(f64::from(composition.width), f64::from(composition.height)),
            source_centre: match source.map(|item| &item.kind) {
                Some(ItemKind::Composition(c)) => half(f64::from(c.width), f64::from(c.height)),
                Some(ItemKind::Footage(f)) => half(f64::from(f.width), f64::from(f.height)),
                _ => [0.0, 0.0],
            },
            effect: None,
        };
        let mut out = Out::default();
        out.keys.insert("id".into(), json!(ids[&layer.id]));
        self.content(layer, source, &mut out, &place);
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
        self.mapping.count(TRANSFORM_GROUP, Fate::Mapped);
        self.rows(
            &transform,
            &TRANSFORM,
            sink,
            &mut out,
            &place.inside(TRANSFORM_GROUP),
        );
        let mut root = Vec::new();
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
                        VECTORS if layer.kind == LayerKind::Shape => {
                            self.mapping.count(VECTORS, Fate::Mapped);
                            let shapes = self.items(group, &here);
                            out.keys.insert("shapes".into(), Json::Array(shapes));
                        }
                        TEXT if layer.kind == LayerKind::Text => self.text(group, &mut out, &here),
                        EFFECTS => {
                            self.mapping.count(EFFECTS, Fate::Mapped);
                            let effects = self.items(group, &here);
                            if !effects.is_empty() {
                                out.keys.insert("fx".into(), Json::Array(effects));
                            }
                        }
                        _ => self.group(group, &here),
                    }
                }
            });
        self.rows(&root, &LAYER_ROOT, sink, &mut out, &place);
        self.switches(layer, composition, ids, &mut out, &place);
        self.timing(layer, composition, &mut out);
        if let Some(lines) = out.lines.take() {
            self.texts.push(StoredText {
                comp: self.scope.clone(),
                layer: ids[&layer.id].clone(),
                lines,
            });
        }
        Json::Object(out.finish())
    }

    /// What the layer draws; a pivot, reported, for what the scene cannot
    /// draw yet.
    fn content(&mut self, layer: &Layer, source: Option<&Item>, out: &mut Out, place: &Place<'_>) {
        let pivot = |walk: &mut Walk<'_>, out: &mut Out, why: Option<&str>| {
            out.keys.insert("pivot".into(), json!(true));
            if let Some(why) = why {
                walk.mapping
                    .count(&format!("layer: {why}"), Fate::Unsupported);
                walk.mapping
                    .unsupported(&place.at, format!("{why} (drawn as nothing)"));
            }
        };
        match layer.kind {
            LayerKind::Shape => {
                out.keys.insert("shapes".into(), json!([]));
            }
            LayerKind::Text => {
                out.keys.insert("text".into(), json!(""));
            }
            LayerKind::Av if layer.switches.adjustment => {
                pivot(self, out, Some("adjustment layers"))
            }
            LayerKind::Av if layer.switches.null => pivot(self, out, None),
            LayerKind::Av => match source.map(|item| (item.id, &item.kind)) {
                Some((id, ItemKind::Composition(_))) => match self.comp_id(id) {
                    Some(comp) => {
                        out.keys.insert("comp".into(), json!(comp));
                    }
                    None => pivot(self, out, Some("compositions it cannot find")),
                },
                Some((_, ItemKind::Footage(footage))) => self.footage(footage, out, place, pivot),
                _ => pivot(self, out, None),
            },
            LayerKind::Camera => pivot(self, out, Some("cameras")),
            LayerKind::Light => pivot(self, out, Some("lights")),
            other => pivot(self, out, Some(&format!("{other} layers"))),
        }
    }

    /// A footage layer: a solid is a rectangle of its colour, and a still
    /// picture that was found an image filling the footage's size. Other
    /// footage is a pivot, reported with why and the file's name.
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
            Source::File { path: Some(stored) } => match media(footage, stored, self.found) {
                Media::Still(path) => {
                    out.keys.insert("image".into(), json!(path));
                    out.keys
                        .insert("box".into(), json!([footage.width, footage.height]));
                    self.mapping.count("footage: image", Fate::Mapped);
                    if footage.pixel_aspect != 1.0 {
                        self.mapping
                            .unsupported(&place.at, "non-square pixels (drawn square)");
                    }
                }
                Media::Unsupported(why) => {
                    pivot(self, out, None);
                    self.mapping
                        .count(&format!("layer: {why}"), Fate::Unsupported);
                    // The file's name last, in backticks, as fonts are named.
                    self.mapping.unsupported(
                        &place.at,
                        format!("{why} (drawn as nothing) (`{}`)", file_name(stored)),
                    );
                }
            },
            Source::File { path: None } => {
                pivot(self, out, Some("footage whose file path it cannot read"))
            }
            Source::Placeholder => pivot(self, out, Some("missing footage")),
        }
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
        if let Some(parent) = layer.parent.and_then(|id| ids.get(&id)) {
            out.keys.insert("parent".into(), json!(parent));
        }
        let mut unsupported = |what: String| self.mapping.unsupported(&place.at, what);
        if switches.three_d {
            unsupported("3D layers (drawn flat)".into());
        }
        if switches.motion_blur {
            unsupported("motion blur".into());
        }
        if switches.solo {
            unsupported("solo layers".into());
        }
        if layer.blend_mode != BlendMode::Normal {
            unsupported(format!("the blend mode `{}`", layer.blend_mode));
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
