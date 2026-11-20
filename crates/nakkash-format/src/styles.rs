//! Styles and eases ([D91]): a scene says a repeated set of layer keys, or a
//! repeated easing curve, once, and its layers and keys name it.
//!
//! - `styles` holds named sets of layer keys. A layer takes them with
//!   `"inherits":"glow-blur-levels"`, or a list of names, applied in order,
//!   so an effect stack and an animation that repeat apart are each one
//!   style. A key the layer writes itself wins over every style's, whole:
//!   keys are never merged inside.
//! - `eases` holds named curves, which the `curve` of a key written as an
//!   object names: `{"t":"0s","v":0,"curve":"still-to-stop-95-97"}`. A name
//!   that already reads as a curve, such as `ease-out`, cannot be taken.
//!
//! Both are expanded before anything else reads the scene, on a copy, as
//! slots are resolved ([D44]): a scene written with them is, once read, the
//! scene written without them, and draws the same bits. The document keeps
//! what was written.
//!
//! [D44]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d44-slots-are-named-values-that-layers-follow
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::borrow::Cow;

use serde_json::{Map, Value as Json};

use crate::parse::key::{COMPS, EASES, ID, INHERITS, LAYERS, STYLES, comp_path};
use crate::{Address, Curve, Diagnostic, Id, Kind, Place, Result, Step};

/// The written form of `styles`.
const STYLES_FORM: &str = r#"an object of styles by name, each an object of layer keys, e.g. {"soft-glow":{"fx":[{"glow":30}]}}"#;
/// The written form of `eases`.
const EASES_FORM: &str =
    r#"an object of curves by name, e.g. {"still-to-stop":"speed(0,0.95,0,0.97)"}"#;
/// The written form of `inherits`.
const INHERITS_FORM: &str = r#"the name of a style of the scene's `styles`, or a list of them, e.g. "soft-glow" or ["soft-glow","rise-in"]"#;

/// The styles and eases a scene declares.
struct Declared<'j> {
    styles: Vec<(&'j str, &'j Map<String, Json>)>,
    eases: Vec<(&'j str, &'j str)>,
}

/// `raw`, a scene's JSON, with every layer's `inherits` replaced by the
/// keys of its styles and every named ease replaced by its curve, and the
/// scene's `styles` and `eases` taken out; `raw` itself when it uses none.
///
/// # Errors
/// An invalid-value diagnostic at a style or an ease that does not read,
/// and an unknown-name one at an `inherits` naming no style.
pub(crate) fn expand(raw: &Json) -> Result<Cow<'_, Json>> {
    let declared = declared(raw)?;
    if declared.styles.is_empty() && declared.eases.is_empty() && !inherits_anywhere(raw) {
        return Ok(Cow::Borrowed(raw));
    }
    let mut copy = raw.clone();
    if let Some(scene) = copy.as_object_mut() {
        scene.shift_remove(STYLES);
        scene.shift_remove(EASES);
    }
    for_each_layer_list(&mut copy, |layers, owner| {
        for (index, layer) in layers.iter_mut().enumerate() {
            let Some(fields) = layer.as_object_mut() else {
                continue;
            };
            let place = || {
                let id = fields_id(fields);
                let layer = Place::layer(&id, index);
                match owner {
                    Some(comp) => comp_path(comp).join(layer),
                    None => layer,
                }
            };
            if fields.contains_key(INHERITS) {
                let styled = inherit(fields, &declared).map_err(|e| e.under(place()))?;
                *fields = styled;
            }
            if !declared.eases.is_empty() {
                fields
                    .values_mut()
                    .for_each(|value| eased(value, &declared.eases));
            }
        }
        Ok(())
    })?;
    Ok(Cow::Owned(copy))
}

/// `layer`, a layer object as written, with its styles' keys set under its
/// own, in place of its `inherits`; the layer as it is without one. For
/// tools that read a layer's keys as it draws them, such as an edit of a
/// key inside one a style gives.
///
/// # Errors
/// An invalid-value diagnostic at a style that does not read, or an
/// unknown-name one at an `inherits` naming no style of `raw`.
pub fn inherited(raw: &Json, layer: &Map<String, Json>) -> Result<Map<String, Json>> {
    if !layer.contains_key(INHERITS) {
        return Ok(layer.clone());
    }
    let declared = declared(raw)?;
    inherit(layer, &declared)
}

/// The names of the styles `layer` inherits, in order.
pub fn style_names(layer: &Map<String, Json>) -> Vec<&str> {
    match layer.get(INHERITS) {
        Some(Json::String(name)) => vec![name.as_str()],
        Some(Json::Array(names)) => names.iter().filter_map(Json::as_str).collect(),
        _ => Vec::new(),
    }
}

/// The keys of `layer` with those of its styles set under its own, written
/// where its `inherits` was, so the order of keys stays the order written.
fn inherit(layer: &Map<String, Json>, declared: &Declared<'_>) -> Result<Map<String, Json>> {
    let names: Vec<&str> = match &layer[INHERITS] {
        Json::String(name) => vec![name.as_str()],
        Json::Array(names) if !names.is_empty() => names
            .iter()
            .map(|n| n.as_str().ok_or_else(|| inherits_invalid(&layer[INHERITS])))
            .collect::<Result<_>>()?,
        other => return Err(inherits_invalid(other)),
    };
    let mut from_styles = Map::new();
    for name in &names {
        let Some((_, keys)) = declared.styles.iter().find(|(n, _)| n == name) else {
            let known = declared.styles.iter().map(|(n, _)| (*n).to_owned());
            let unknown = Diagnostic::unknown("style", *name, known);
            let unknown = match declared.styles.is_empty() {
                true => unknown.expected("a style of the scene's `styles`, which it does not have"),
                false => unknown,
            };
            return Err(unknown.at(under_key(INHERITS)));
        };
        for (key, value) in keys.iter() {
            from_styles.insert(key.clone(), value.clone());
        }
    }
    let mut out = Map::new();
    for (key, value) in layer {
        if key == INHERITS {
            for (k, v) in &from_styles {
                if !layer.contains_key(k) {
                    out.insert(k.clone(), v.clone());
                }
            }
        } else {
            out.insert(key.clone(), value.clone());
        }
    }
    Ok(out)
}

/// Replaces every named ease in `value`: the `curve` of an object with a
/// time, `t`, a key written as an object, as one name or one per component.
fn eased(value: &mut Json, eases: &[(&str, &str)]) {
    match value {
        Json::Object(fields) => {
            if fields.contains_key("t")
                && let Some(curve) = fields.get_mut("curve")
            {
                match curve {
                    Json::String(name) => named(name, eases),
                    Json::Array(names) => names.iter_mut().for_each(|n| {
                        if let Json::String(name) = n {
                            named(name, eases);
                        }
                    }),
                    _ => {}
                }
            }
            fields.values_mut().for_each(|item| eased(item, eases));
        }
        Json::Array(items) => items.iter_mut().for_each(|item| eased(item, eases)),
        _ => {}
    }
}

/// `name` replaced by its curve when it names an ease.
fn named(name: &mut String, eases: &[(&str, &str)]) {
    if let Some((_, curve)) = eases.iter().find(|(n, _)| *n == name.as_str()) {
        *name = (*curve).to_owned();
    }
}

/// The styles and eases `raw` declares, each checked.
fn declared(raw: &Json) -> Result<Declared<'_>> {
    let mut out = Declared {
        styles: Vec::new(),
        eases: Vec::new(),
    };
    if let Some(styles) = raw.get(STYLES) {
        let styles = styles.as_object().ok_or_else(|| {
            Diagnostic::new(Kind::Invalid, "styles that are not an object")
                .expected(STYLES_FORM)
                .at(under_key(STYLES))
        })?;
        for (name, keys) in styles {
            let at = || under_key(STYLES).join(under_key(name));
            name.parse::<Id>().map_err(|e| e.at(at()))?;
            let keys = keys.as_object().ok_or_else(|| {
                Diagnostic::new(Kind::Invalid, format!("the style `{keys}`"))
                    .expected(STYLES_FORM)
                    .at(at())
            })?;
            if let Some(key) = [ID, INHERITS].into_iter().find(|k| keys.contains_key(*k)) {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("a style that sets `{key}`, which belongs to the layer"),
                )
                .expected("layer keys other than `id` and `inherits`")
                .at(at().join(under_key(key))));
            }
            out.styles.push((name.as_str(), keys));
        }
    }
    if let Some(eases) = raw.get(EASES) {
        let eases = eases.as_object().ok_or_else(|| {
            Diagnostic::new(Kind::Invalid, "eases that are not an object")
                .expected(EASES_FORM)
                .at(under_key(EASES))
        })?;
        for (name, curve) in eases {
            let at = || under_key(EASES).join(under_key(name));
            name.parse::<Id>().map_err(|e| e.at(at()))?;
            if name.parse::<Curve>().is_ok() {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("an ease named `{name}`, which is already a curve"),
                )
                .expected("a name that does not read as a curve, e.g. `still-to-stop`")
                .at(at()));
            }
            let Some(text) = curve.as_str() else {
                return Err(
                    Diagnostic::new(Kind::Invalid, format!("the ease `{curve}`"))
                        .expected(EASES_FORM)
                        .at(at()),
                );
            };
            text.parse::<Curve>().map_err(|e| e.at(at()))?;
            out.eases.push((name.as_str(), text));
        }
    }
    Ok(out)
}

/// Whether any layer of `raw` names styles.
fn inherits_anywhere(raw: &Json) -> bool {
    let has = |layers: Option<&Json>| {
        layers
            .and_then(Json::as_array)
            .is_some_and(|l| l.iter().any(|layer| layer.get(INHERITS).is_some()))
    };
    has(raw.get(LAYERS))
        || raw
            .get(COMPS)
            .and_then(Json::as_object)
            .is_some_and(|comps| comps.values().any(|c| has(c.get(LAYERS))))
}

/// Calls `each` with the scene's layers and then each composition's, with
/// the composition's id.
fn for_each_layer_list(
    raw: &mut Json,
    mut each: impl FnMut(&mut Vec<Json>, Option<&str>) -> Result<()>,
) -> Result<()> {
    if let Some(Json::Array(layers)) = raw.get_mut(LAYERS) {
        each(layers, None)?;
    }
    if let Some(Json::Object(comps)) = raw.get_mut(COMPS) {
        for (id, comp) in comps.iter_mut() {
            if let Some(Json::Array(layers)) = comp.get_mut(LAYERS) {
                each(layers, Some(id))?;
            }
        }
    }
    Ok(())
}

fn fields_id(fields: &Map<String, Json>) -> String {
    fields
        .get(ID)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn under_key(key: &str) -> Place {
    Place::Scene(Address {
        layers: Vec::new(),
        keys: vec![Step::Key(key.to_owned())],
    })
}

fn inherits_invalid(written: &Json) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("`inherits` of `{written}`"))
        .expected(INHERITS_FORM)
        .at(under_key(INHERITS))
}

#[cfg(test)]
#[path = "../tests/unit/styles.rs"]
mod tests;
