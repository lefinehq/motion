//! A mask's settings, as its `mkif` stores them, onto its scene item
//! ([D55]): its mode and the side it keeps, and what the scene cannot draw
//! of it yet, reported.
//!
//! [D55]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d55-masks-are-items-that-cut-a-layers-content-before-its-effects

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value as Json, json};

use super::Walk;
use super::ids::Ids;
use super::items::Place;
use super::keys::svg;
use super::layer::{EFFECTS, MASKS};
use super::rows::MASK_MODES;
use crate::model::{Child, Group, Layer, MaskSettings, Path, Stream, Value, Vertex};

/// The masks of `layer` its effects follow, each with its scene id, made
/// from its name, by the id the project gives it ([`MaskSettings::id`]).
/// A path parameter that names a mask the layer does not have is reported
/// where its effect is mapped.
pub(super) fn followed(layer: &Layer) -> HashMap<u32, String> {
    let followed: HashSet<u32> = layer
        .properties
        .children
        .iter()
        .filter_map(|child| match child {
            Child::Group(group) if group.match_name == EFFECTS => Some(group),
            _ => None,
        })
        .flat_map(Group::properties)
        .filter_map(|property| match property.value {
            Stream::Static(Value::Mask(id)) => Some(id),
            _ => None,
        })
        .collect();
    let mut ids = Ids::default();
    layer
        .properties
        .children
        .iter()
        .filter_map(|child| match child {
            Child::Group(group) if group.match_name == MASKS => Some(group),
            _ => None,
        })
        .flat_map(|parade| &parade.children)
        .filter_map(|child| match child {
            Child::Group(mask) => Some((mask.mask?.id, mask.name.as_deref())),
            Child::Property(_) => None,
        })
        .filter(|(id, _)| followed.contains(id))
        .map(|(id, name)| (id, ids.give(name.unwrap_or_default(), "mask")))
        .collect()
}

impl Walk<'_> {
    /// Writes `settings` onto the mask item `keys`, and an outline over the
    /// whole mask space where the project stores none.
    pub(super) fn mask(
        &mut self,
        settings: MaskSettings,
        keys: &mut Map<String, Json>,
        place: &Place<'_>,
    ) {
        if keys.get("mask").is_none_or(Json::is_null) {
            keys.insert("mask".into(), json!(svg(&whole(place.source_unit))));
        }
        // An effect that follows the mask names it by this id.
        if let Some(id) = self.mask_ids.get(&settings.id) {
            keys.insert("id".into(), json!(id));
        }
        match MASK_MODES.iter().find(|(mode, _)| *mode == settings.mode) {
            Some((_, "add")) => {}
            Some((_, mode)) => {
                keys.insert("mode".into(), json!(mode));
            }
            None => self
                .mapping
                .unsupported(&place.at, format!("mask mode {}", settings.mode)),
        }
        if settings.inverted {
            keys.insert("keep".into(), json!("outside"));
        }
        if settings.motion_blur == 2 {
            self.mapping.unsupported(&place.at, "mask motion blur");
        }
        let feathered = keys.contains_key("feather")
            || keys
                .get("anim")
                .and_then(Json::as_object)
                .is_some_and(|anim| anim.contains_key("feather"));
        if settings.linear_falloff && feathered {
            self.mapping
                .unsupported(&place.at, "a mask feather that falls off linearly");
        }
    }
}

/// The closed rectangle over a mask space of `size`, as the project reads a
/// mask that stores no path.
fn whole([w, h]: [f64; 2]) -> Path {
    let corner = |x, y| Vertex {
        point: [x, y],
        in_tangent: [0.0, 0.0],
        out_tangent: [0.0, 0.0],
    };
    Path {
        closed: true,
        vertices: vec![
            corner(0.0, 0.0),
            corner(0.0, h),
            corner(w, h),
            corner(w, 0.0),
        ],
    }
}

#[cfg(test)]
#[path = "../../tests/unit/map/masks.rs"]
mod tests;
