//! Where a scene's JSON holds colours ([D77]): every value the format reads
//! as a colour, found by the kinds of its layers' keys and items, so an
//! import can bind each to a palette slot and a listing can count those
//! that no slot holds.
//!
//! A place is listed whether it holds a colour or a reference to a slot.
//! A colour that cannot show is marked: the fill of an adjustment layer,
//! the fill of a layer an effect paints over, and the scene's `bg` under a
//! bottom layer that covers the canvas.
//!
//! [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary

use serde_json::Value as Json;

use crate::item::{ANIMATORS, EFFECTS, ItemKind, Primary, SHAPES};
use crate::{Step, ValueType};

/// A keyed colour with more keys than this was written frame by frame, as
/// a bake does.
pub const BAKED_KEYS: usize = 6;

/// The effects whose drawing replaces the colours of the layer under them,
/// so the layer's own fill does not show.
const REPLACING: [&str; 3] = ["ramp", "fill", "four-color"];

/// One place in a scene that holds a colour.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorPlace {
    /// The steps from the scene to the value, keys and indices.
    pub steps: Vec<Step>,
    /// Whether it is a text's colour: its fill, stroke, runs or animators.
    pub text: bool,
    /// Whether it paints the background: the scene's `bg`, or the fill of
    /// the scene's bottom layer when that covers the canvas.
    pub background: bool,
    /// Whether it is a key of a colour keyed frame by frame.
    pub baked: bool,
    /// Whether it can show at all.
    pub shown: bool,
}

/// Every colour place of `scene`, the scene's and each composition's
/// `bg`, then each layer's, the scene's layers first.
pub fn places(scene: &Json) -> Vec<ColorPlace> {
    let mut found = Vec::new();
    let covered = bottom_covers_canvas(scene);
    let key = |k: &str| Step::Key(k.to_owned());
    push(
        scene,
        vec![key("bg")],
        Flags {
            text: false,
            background: true,
            shown: !covered,
        },
        &mut found,
    );
    if let Some(comps) = scene["comps"].as_object() {
        for id in comps.keys() {
            let steps = vec![key("comps"), key(id), key("bg")];
            push(scene, steps, Flags::default(), &mut found);
        }
    }
    let mut owners = vec![(Vec::new(), &scene["layers"])];
    if let Some(comps) = scene["comps"].as_object() {
        owners.extend(
            comps
                .iter()
                .map(|(id, comp)| (vec![key("comps"), key(id)], &comp["layers"])),
        );
    }
    for (prefix, layers) in owners {
        let Some(layers) = layers.as_array() else {
            continue;
        };
        for (index, layer) in layers.iter().enumerate() {
            let mut steps = prefix.clone();
            steps.extend([key("layers"), Step::Index(index)]);
            let background = prefix.is_empty() && index == 0 && covered;
            layer_places(scene, layer, &steps, background, &mut found);
        }
    }
    found
}

/// What a place is, besides where.
#[derive(Debug, Clone, Copy)]
struct Flags {
    text: bool,
    background: bool,
    shown: bool,
}

impl Default for Flags {
    fn default() -> Flags {
        Flags {
            text: false,
            background: false,
            shown: true,
        }
    }
}

/// Whether `value` reads as a colour, or is a reference to a slot.
fn holds_color(value: &Json) -> bool {
    match value {
        Json::String(text) if text.starts_with('$') => true,
        Json::String(text) if text.starts_with('#') => crate::is_color(value),
        Json::Array(items) if (3..=4).contains(&items.len()) => crate::is_color(value),
        _ => false,
    }
}

/// Adds the place at `steps` when it holds a colour.
fn push(scene: &Json, steps: Vec<Step>, flags: Flags, found: &mut Vec<ColorPlace>) {
    if crate::address::json_at(scene, &steps).is_some_and(holds_color) {
        found.push(ColorPlace {
            steps,
            text: flags.text,
            background: flags.background,
            baked: false,
            shown: flags.shown,
        });
    }
}

/// Whether the scene's bottom layer is a still, opaque rectangle that
/// covers its canvas for its whole length, which hides its `bg`.
fn bottom_covers_canvas(scene: &Json) -> bool {
    let Some(bottom) = scene["layers"].get(0).and_then(Json::as_object) else {
        return false;
    };
    // Only keys that keep it still and whole over the canvas: a position on
    // its anchor, and a scale of 1.
    let still = bottom.iter().all(|(key, value)| match key.as_str() {
        "id" | "rect" | "fill" | "anchor" => true,
        "position" => Some(value) == bottom.get("anchor"),
        "scale" => value
            .as_array()
            .is_some_and(|s| s.iter().all(|n| n.as_f64() == Some(1.0))),
        _ => false,
    });
    let size = scene["size"].as_str().unwrap_or_default();
    let rect = bottom
        .get("rect")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let opaque = bottom
        .get("fill")
        .and_then(crate::color)
        .is_some_and(|c| c[3] >= 1.0);
    still && opaque && !size.is_empty() && rect == size
}

/// The colour places of one layer at `steps`: its fill and keyed fill, its
/// text style and runs, and the colour properties of its effects, shapes
/// and animators, key by key, and of what its `overrides` set.
fn layer_places(
    scene: &Json,
    layer: &Json,
    steps: &[Step],
    background: bool,
    found: &mut Vec<ColorPlace>,
) {
    let Some(keys) = layer.as_object() else {
        return;
    };
    let text = keys.contains_key("text");
    let flag = |key: &str| keys.get(key).and_then(Json::as_bool) == Some(true);
    // A layer left out, or one that draws nothing of its own, shows none
    // of its colours.
    let drawn = !flag("hidden")
        && !["pivot", "camera", "audio"]
            .iter()
            .any(|k| keys.contains_key(*k));
    let adjust = flag("adjust");
    // An effect that paints the layer over hides the colours of what the
    // layer draws, not those of its effects.
    let painted_over = keys.get("fx").and_then(Json::as_array).is_some_and(|fx| {
        fx.iter().any(|e| {
            REPLACING.iter().any(|kind| e.get(*kind).is_some())
                && e.get("hidden").and_then(Json::as_bool) != Some(true)
        })
    });
    let content = drawn && !painted_over;
    let under = |path: &[&str]| {
        let mut s = steps.to_vec();
        s.extend(path.iter().map(|k| Step::Key((*k).to_owned())));
        s
    };
    let own = Flags {
        text,
        background,
        shown: content && !adjust,
    };
    push(scene, under(&["fill"]), own, found);
    if let Some(track) = keys.get("anim").and_then(|a| a.get("fill")) {
        keyed(track, &under(&["anim", "fill"]), own, found);
    }
    let texts = Flags {
        text,
        background: false,
        shown: content,
    };
    for key in ["fill", "stroke"] {
        push(scene, under(&["style", key]), texts, found);
    }
    if let Some(runs) = keys.get("runs").and_then(Json::as_array) {
        for i in 0..runs.len() {
            for key in ["fill", "stroke"] {
                let mut s = under(&["runs"]);
                s.extend([Step::Index(i), Step::Key(key.into())]);
                push(scene, s, texts, found);
            }
        }
    }
    for (key, kinds) in [
        ("fx", &EFFECTS[..]),
        ("shapes", &SHAPES[..]),
        ("animate", &ANIMATORS[..]),
    ] {
        if let Some(items) = keys.get(key).and_then(Json::as_array) {
            let flags = Flags {
                text: text && key == "animate",
                background: false,
                shown: match key {
                    "fx" => drawn,
                    _ => content,
                },
            };
            for (i, item) in items.iter().enumerate() {
                let mut s = under(&[key]);
                s.push(Step::Index(i));
                item_places(item, kinds, &s, flags, found);
            }
        }
    }
    if let Some(overrides) = keys.get("overrides").and_then(Json::as_object) {
        for (target, inner) in overrides.iter().filter(|(t, _)| !t.starts_with('$')) {
            let mut s = under(&["overrides"]);
            s.push(Step::Key(target.clone()));
            layer_places(scene, inner, &s, false, found);
        }
    }
}

/// The colour places of one item of `kinds` at `steps`, and of the items
/// inside it.
fn item_places(
    item: &Json,
    kinds: &'static [ItemKind],
    steps: &[Step],
    flags: Flags,
    found: &mut Vec<ColorPlace>,
) {
    let Some(keys) = item.as_object() else {
        return;
    };
    let Some(kind) = kinds.iter().find(|kind| keys.contains_key(kind.name)) else {
        return;
    };
    // A hidden item draws nothing, nor do the items inside it.
    let flags = Flags {
        shown: flags.shown && keys.get("hidden").and_then(Json::as_bool) != Some(true),
        ..flags
    };
    let is_color = |name: &str| {
        let name = match (name == kind.name, kind.primary) {
            (true, Primary::Property(primary)) => primary,
            _ => name,
        };
        kind.registry
            .prop(name)
            .is_some_and(|p| p.spec().ty == ValueType::Color)
    };
    for (key, value) in keys {
        let mut s = steps.to_vec();
        s.push(Step::Key(key.clone()));
        match (key.as_str(), kind.primary) {
            (name, Primary::Items(inner)) if name == kind.name => {
                for (i, nested) in value.as_array().into_iter().flatten().enumerate() {
                    let mut n = s.clone();
                    n.push(Step::Index(i));
                    item_places(nested, inner, &n, flags, found);
                }
            }
            ("anim", _) => {
                for (track, keys) in value.as_object().into_iter().flatten() {
                    if is_color(track) {
                        let mut t = s.clone();
                        t.push(Step::Key(track.clone()));
                        keyed(keys, &t, flags, found);
                    }
                }
            }
            (name, _) if is_color(name) && holds_color(value) => found.push(ColorPlace {
                steps: s,
                text: flags.text,
                background: false,
                baked: false,
                shown: flags.shown,
            }),
            _ => {}
        }
    }
}

/// The keys of a keyed colour at `steps`: each key's value, `v` of an
/// object or the second of a pair.
fn keyed(track: &Json, steps: &[Step], flags: Flags, found: &mut Vec<ColorPlace>) {
    let Some(list) = track.as_array() else {
        return;
    };
    let baked = list.len() > BAKED_KEYS;
    for (i, key) in list.iter().enumerate() {
        let (value, inner) = match key {
            Json::Object(fields) => (fields.get("v"), Step::Key("v".into())),
            Json::Array(pair) if pair.len() == 2 => (pair.get(1), Step::Index(1)),
            _ => continue,
        };
        if value.is_some_and(holds_color) {
            let mut s = steps.to_vec();
            s.extend([Step::Index(i), inner]);
            found.push(ColorPlace {
                steps: s,
                text: flags.text,
                background: false,
                baked,
                shown: flags.shown,
            });
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/colors.rs"]
mod tests;
