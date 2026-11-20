//! The compact form of a scene ([D91]): the same scene in fewer bytes.
//!
//! [`compact`] writes three things, each read back exactly as it stood:
//!
//! - **Defaults left out.** A layer's property, or an item's, written at
//!   the fixed resting value the format's version gives it, and a setting
//!   written at its default choice. Only exact registry rows: numbers,
//!   points and angles equal to the rest, never a value that rests
//!   somewhere the layer decides (an anchor at the box's centre).
//! - **Styles.** An effect stack, or an animation, that two or more layers
//!   write the same is written once in `styles`, and each of those layers
//!   inherits it.
//! - **Eases.** A `speed(…)` or `cubic(…)` curve two or more keys write is
//!   written once in `eases`, under a name that says its shape.
//!
//! Names are long and readable (`fx-glow-levels`, `anim-position-opacity`,
//! `still-to-stop-95-97`): an agent reads them.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value as Json};

use crate::item::{EFFECTS, ItemKind, MASKS, Primary, SHAPES};
use crate::parse::key::{
    ANIM, COMPS, EASES, FX, ID, INHERITS, LAYERS, MASK, SHAPES as SHAPES_KEY, STYLES,
};
use crate::property::{PROPERTIES, PropertySpec, Rest};
use crate::{Curve, Id, Result, Value};

/// What [`compact`] does; each part can be left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts {
    /// Leave out values at their defaults.
    pub defaults: bool,
    /// Write repeated effect stacks and animations as styles.
    pub styles: bool,
    /// Write repeated curves as eases.
    pub eases: bool,
}

impl Parts {
    /// Every part.
    pub const ALL: Parts = Parts {
        defaults: true,
        styles: true,
        eases: true,
    };
}

/// What [`compact`] wrote, counted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compacted {
    /// Values left out at their defaults.
    pub defaults: usize,
    /// Styles written, and the layers that inherit them.
    pub styles: usize,
    /// Layers that inherit a style.
    pub inheriting: usize,
    /// Eases written.
    pub eases: usize,
    /// Keys that name an ease.
    pub eased: usize,
}

/// `raw`, a scene's JSON, in compact form, with what `parts` allow. A scene
/// that already has styles or eases is first read as written without them,
/// so the result names its own.
///
/// # Errors
/// The diagnostic of a style or an ease of `raw` that does not read.
pub fn compact(raw: &Json, parts: Parts) -> Result<(Json, Compacted)> {
    compact_keeping(raw, parts, &|_| false)
}

/// [`compact`], writing out every default of a row `keep` holds, named as
/// [`default_rows`] names it.
///
/// # Errors
/// As [`compact`].
pub fn compact_keeping(
    raw: &Json,
    parts: Parts,
    keep: &dyn Fn(&str) -> bool,
) -> Result<(Json, Compacted)> {
    let mut scene = crate::styles::expand(raw)?.into_owned();
    let mut done = Compacted::default();
    if parts.defaults {
        for_each_layer(&mut scene, |layer| {
            done.defaults += defaults(layer, keep, &mut |_| {})
        });
    }
    if parts.styles {
        styles(&mut scene, &mut done);
    }
    if parts.eases {
        eases(&mut scene, &mut done);
    }
    Ok((scene, done))
}

/// Calls `each` with every layer object of the scene and its compositions.
fn for_each_layer(scene: &mut Json, mut each: impl FnMut(&mut Map<String, Json>)) {
    let mut lists: Vec<&mut Vec<Json>> = Vec::new();
    // Split the borrows: the scene's layers, then each composition's.
    let Json::Object(top) = scene else {
        return;
    };
    let mut comps = None;
    for (key, value) in top.iter_mut() {
        match (key.as_str(), value) {
            (LAYERS, Json::Array(layers)) => lists.push(layers),
            (COMPS, Json::Object(c)) => comps = Some(c),
            _ => {}
        }
    }
    if let Some(comps) = comps {
        for comp in comps.values_mut() {
            if let Some(Json::Array(layers)) = comp.get_mut(LAYERS) {
                lists.push(layers);
            }
        }
    }
    for layers in lists {
        for layer in layers.iter_mut() {
            if let Some(layer) = layer.as_object_mut() {
                each(layer);
            }
        }
    }
}

// ---- Defaults ----------------------------------------------------------

/// The rows of `raw`'s layers and items that hold a value at its
/// default, each named `layer rotate` or `glow threshold`, in name order:
/// what [`compact_keeping`] may be told to keep.
///
/// # Errors
/// As [`compact`].
pub fn default_rows(raw: &Json) -> Result<Vec<String>> {
    let mut scene = crate::styles::expand(raw)?.into_owned();
    let mut rows = std::collections::BTreeSet::new();
    for_each_layer(&mut scene, |layer| {
        defaults(layer, &|_| true, &mut |row| {
            rows.insert(row);
        });
    });
    Ok(rows.into_iter().collect())
}

/// Leaves out the keys of `layer` at their defaults, but for the rows
/// `keep` holds, and tells `met` each row met at its default; how many.
fn defaults(
    layer: &mut Map<String, Json>,
    keep: &dyn Fn(&str) -> bool,
    met: &mut dyn FnMut(String),
) -> usize {
    let mut left = 0;
    layer.retain(|key, value| {
        let at_rest = PROPERTIES
            .iter()
            .find(|spec| spec.name == key)
            .is_some_and(|spec| rests(spec, value));
        if !at_rest {
            return true;
        }
        let row = format!("layer {key}");
        let kept = keep(&row);
        met(row);
        left += usize::from(!kept);
        kept
    });
    for (key, kinds) in [
        (FX, &EFFECTS[..]),
        (MASK, &MASKS[..]),
        (SHAPES_KEY, &SHAPES[..]),
    ] {
        if let Some(Json::Array(items)) = layer.get_mut(key) {
            left += items
                .iter_mut()
                .map(|item| item_defaults(item, kinds, keep, met))
                .sum::<usize>();
        }
    }
    left
}

/// Leaves out the keys of `item`, of one of `kinds`, at their defaults, and
/// inside the items it holds; how many.
fn item_defaults(
    item: &mut Json,
    kinds: &'static [ItemKind],
    keep: &dyn Fn(&str) -> bool,
    met: &mut dyn FnMut(String),
) -> usize {
    let Some(fields) = item.as_object_mut() else {
        return 0;
    };
    // An item's kind is its first key, as every writer puts it.
    let Some(kind) = fields
        .keys()
        .next()
        .and_then(|first| kinds.iter().find(|k| k.name == first))
    else {
        return 0;
    };
    let mut left = 0;
    fields.retain(|key, value| {
        if key == kind.name {
            return true;
        }
        let at_rest = kind
            .registry
            .rows
            .iter()
            .find(|row| row.name == key)
            .is_some_and(|row| rests(row, value))
            || kind
                .settings
                .iter()
                .find(|s| s.name == key)
                .is_some_and(|s| value.as_str() == Some(s.default));
        if !at_rest {
            return true;
        }
        let row = format!("{} {key}", kind.name);
        let kept = keep(&row);
        met(row);
        left += usize::from(!kept);
        kept
    });
    if let Primary::Items(inner) = kind.primary
        && let Some(Json::Array(items)) = fields.get_mut(kind.name)
    {
        left += items
            .iter_mut()
            .map(|item| item_defaults(item, inner_kinds(inner), keep, met))
            .sum::<usize>();
    }
    left
}

/// The kinds an item of items holds, as a static list.
fn inner_kinds(kinds: &'static [ItemKind]) -> &'static [ItemKind] {
    kinds
}

/// Whether `value`, written for the property `spec`, is its fixed rest,
/// exactly: a number, a point or three angles, not animated.
fn rests(spec: &PropertySpec, value: &Json) -> bool {
    let Rest::Value(rest) = &spec.rest else {
        return false;
    };
    let numbers = |json: &Json| -> Option<Vec<f64>> {
        match json {
            Json::Number(n) => Some(vec![n.as_f64()?]),
            Json::Array(items) => items.iter().map(Json::as_f64).collect(),
            _ => None,
        }
    };
    let Some(written) = numbers(value) else {
        return false;
    };
    match rest {
        Value::Number(n) => written == [*n],
        Value::Point(p) => written == p.as_slice(),
        Value::Orientation(o) => written == o.as_slice(),
        Value::Color(_) | Value::Path(_) => false,
    }
}

// ---- Styles ------------------------------------------------------------

/// Writes every effect stack and animation that two or more layers write
/// the same as a style they inherit.
fn styles(scene: &mut Json, done: &mut Compacted) {
    // The text of each key's value, and the layers writing it.
    let mut seen: BTreeMap<(&'static str, String), usize> = BTreeMap::new();
    for_each_layer(scene, |layer| {
        for key in [FX, ANIM] {
            if let Some(value) = layer.get(key) {
                *seen.entry((key, value.to_string())).or_default() += 1;
            }
        }
    });
    let mut taken: HashSet<String> = HashSet::new();
    let mut table = Map::new();
    let mut names: BTreeMap<(&'static str, String), String> = BTreeMap::new();
    // Most used first, so the shortest names go where they save most.
    let mut repeated: Vec<_> = seen.into_iter().filter(|(_, n)| *n >= 2).collect();
    repeated.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for ((key, text), uses) in repeated {
        let value: Json = serde_json::from_str(&text).expect("written by serde_json");
        let base = match key {
            FX => format!("fx-{}", effect_words(&value)),
            _ => format!("anim-{}", anim_words(&value)),
        };
        let name = free(&base, &mut taken);
        // A style saves its value at every use but one, and costs a name at each.
        let cost = name.len() * (uses + 1) + 16 * uses;
        if text.len() * (uses - 1) <= cost {
            continue;
        }
        let mut style = Map::new();
        style.insert(key.to_owned(), value);
        table.insert(name.clone(), Json::Object(style));
        names.insert((key, text), name);
    }
    if table.is_empty() {
        return;
    }
    done.styles = table.len();
    for_each_layer(scene, |layer| {
        let mut inherited: Vec<String> = Vec::new();
        let mut first: Option<String> = None;
        for key in [FX, ANIM] {
            if let Some(value) = layer.get(key)
                && let Some(name) = names.get(&(key, value.to_string()))
            {
                inherited.push(name.clone());
                first.get_or_insert_with(|| key.to_owned());
            }
        }
        let Some(first) = first else {
            return;
        };
        done.inheriting += 1;
        let inherits = match inherited.as_slice() {
            [one] => Json::String(one.clone()),
            _ => Json::Array(inherited.iter().cloned().map(Json::String).collect()),
        };
        let styled: Vec<&str> = [FX, ANIM]
            .into_iter()
            .filter(|key| {
                layer
                    .get(*key)
                    .is_some_and(|v| names.contains_key(&(*key, v.to_string())))
            })
            .collect();
        let mut out = Map::new();
        for (key, value) in std::mem::take(layer) {
            if key == first {
                out.insert(INHERITS.to_owned(), inherits.clone());
            } else if styled.contains(&key.as_str()) {
                continue;
            } else {
                out.insert(key, value);
            }
        }
        *layer = out;
    });
    insert_after(scene, STYLES, Json::Object(table));
}

/// The kinds of an effect stack's effects, in order, each once.
fn effect_words(stack: &Json) -> String {
    let mut words: Vec<&str> = Vec::new();
    for item in stack.as_array().into_iter().flatten() {
        if let Some(kind) = item.as_object().and_then(|o| o.keys().next())
            && words.last() != Some(&kind.as_str())
        {
            words.push(kind);
        }
    }
    readable(&words)
}

/// The properties an animation moves, in order.
fn anim_words(anim: &Json) -> String {
    let words: Vec<&str> = anim
        .as_object()
        .into_iter()
        .flat_map(|o| o.keys())
        .map(String::as_str)
        .collect();
    readable(&words)
}

/// At most four words joined by hyphens, then `and-more`.
fn readable(words: &[&str]) -> String {
    let mut out: Vec<&str> = words.iter().copied().take(4).collect();
    if words.len() > 4 {
        out.push("and-more");
    }
    match out.is_empty() {
        true => "none".to_owned(),
        false => out.join("-"),
    }
}

/// `base`, or `base-2`, `base-3`…, whichever is a free id.
fn free(base: &str, taken: &mut HashSet<String>) -> String {
    let base: String = base
        .chars()
        .map(|c| match c.is_ascii_alphanumeric() {
            true => c.to_ascii_lowercase(),
            false => '-',
        })
        .collect();
    let name = (1..)
        .map(|n| match n {
            1 => base.clone(),
            n => format!("{base}-{n}"),
        })
        .find(|name| !taken.contains(name) && Id::is_valid(name) && name.parse::<Curve>().is_err())
        .expect("some number is free");
    taken.insert(name.clone());
    name
}

/// Sets `key` on the scene object after its `presets`, or after `labels`,
/// `shutter`… so it stands where the format lists it.
fn insert_after(scene: &mut Json, key: &str, value: Json) {
    let Some(top) = scene.as_object_mut() else {
        return;
    };
    let order = crate::SCENE_KEYS;
    let rank = |k: &str| order.iter().position(|o| *o == k).unwrap_or(order.len());
    let mine = rank(key);
    let index = top.keys().position(|k| rank(k) > mine).unwrap_or(top.len());
    top.shift_insert(index, key.to_owned(), value);
}

// ---- Eases -------------------------------------------------------------

/// Writes every `speed(…)` or `cubic(…)` curve that two or more keys write
/// as an ease they name.
fn eases(scene: &mut Json, done: &mut Compacted) {
    let mut uses: BTreeMap<String, usize> = BTreeMap::new();
    walk_curves(scene, &mut |curve| {
        *uses.entry(curve.clone()).or_default() += 1
    });
    let mut taken: HashSet<String> = HashSet::new();
    let mut table = Map::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for (curve, n) in uses {
        let Some(base) = shape_words(&curve) else {
            continue;
        };
        if n < 2 {
            continue;
        }
        let name = free(&base, &mut taken);
        if curve.len() * n <= name.len() * (n + 1) + curve.len() + 8 {
            taken.remove(&name);
            continue;
        }
        table.insert(name.clone(), Json::String(curve.clone()));
        names.insert(curve, name);
    }
    if table.is_empty() {
        return;
    }
    done.eases = table.len();
    walk_curves(scene, &mut |curve| {
        if let Some(name) = names.get(curve.as_str()) {
            *curve = name.clone();
            done.eased += 1;
        }
    });
    insert_after(scene, EASES, Json::Object(table));
}

/// Calls `each` with every curve a key written as an object names, in the
/// scene's layers and styles.
fn walk_curves(scene: &mut Json, each: &mut impl FnMut(&mut String)) {
    fn walk(value: &mut Json, each: &mut impl FnMut(&mut String)) {
        match value {
            Json::Object(fields) => {
                if fields.contains_key("t")
                    && let Some(Json::String(curve)) = fields.get_mut("curve")
                {
                    each(curve);
                }
                for (key, item) in fields.iter_mut() {
                    if key != ID {
                        walk(item, each);
                    }
                }
            }
            Json::Array(items) => items.iter_mut().for_each(|item| walk(item, each)),
            _ => {}
        }
    }
    let Some(top) = scene.as_object_mut() else {
        return;
    };
    for key in [LAYERS, COMPS, STYLES] {
        if let Some(value) = top.get_mut(key) {
            walk(value, each);
        }
    }
}

/// A name for a curve by its shape, for `speed(…)` and `cubic(…)`: whether
/// it starts still or moving, ends at a stop or moving, and how far each
/// side reaches, in hundredths: `still-to-stop-95-97`.
fn shape_words(curve: &str) -> Option<String> {
    let numbers = |inner: &str| -> Option<Vec<f64>> {
        inner
            .split(',')
            .map(|n| n.trim().parse::<f64>().ok())
            .collect()
    };
    let hundredths = |n: f64| format!("{}", (n * 100.0).round() as i64).replace('-', "minus-");
    if let Some(inner) = curve
        .strip_prefix("speed(")
        .and_then(|r| r.strip_suffix(')'))
    {
        let n = numbers(inner)?;
        let [out, out_reach, into, in_reach] = n.as_slice() else {
            return None;
        };
        let start = if *out == 0.0 { "still" } else { "moving" };
        let end = if *into == 0.0 { "stop" } else { "moving" };
        return Some(format!(
            "{start}-to-{end}-{}-{}",
            hundredths(*out_reach),
            hundredths(*in_reach)
        ));
    }
    if let Some(inner) = curve
        .strip_prefix("cubic(")
        .and_then(|r| r.strip_suffix(')'))
    {
        let n = numbers(inner)?;
        return Some(format!(
            "cubic-{}",
            n.iter()
                .map(|v| hundredths(*v))
                .collect::<Vec<_>>()
                .join("-")
        ));
    }
    None
}

#[cfg(test)]
#[path = "../tests/unit/compact.rs"]
mod tests;
