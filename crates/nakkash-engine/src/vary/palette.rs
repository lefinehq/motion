//! The palette axis ([D78]): the scene's colours, gathered from every place
//! that writes one, and moved together by a scheme.
//!
//! Every use of one colour maps to the same new colour, whether it is a
//! slot's value, a layer's fill, an effect's colour or a key of a colour
//! track, so a slot and the values that follow it change together, and so
//! do two layers a designer gave the same colour. Each colour keeps its
//! WCAG luminance under every scheme but `brand`, so every contrast ratio
//! between two of them is the master's.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::LazyLock;

use nakkash_format::item::{ANIMATORS, EFFECTS, MASKS, SHAPES};
use nakkash_format::property::{GROUP, LAYER, Registry};
use nakkash_format::{Listed, ValueType};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use super::colour::{self, Rgba};
use super::places::{self, Held, Visit};
use super::rng::Rng;

/// How a palette moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "scheme")]
pub enum Scheme {
    /// Every hue turned by `degrees`.
    Rotate {
        /// The turn, in degrees.
        degrees: f64,
    },
    /// Every hue drawn within 60° around `hue`, in the master's order.
    Analogous {
        /// The middle hue, in degrees.
        hue: f64,
    },
    /// The master's main hue family at `hue`, the others opposite it.
    Complementary {
        /// The main hue, in degrees.
        hue: f64,
    },
    /// Light and dark swapped, hue and chroma kept: every luminance is
    /// mirrored so that every contrast ratio stays exactly the master's.
    Invert,
    /// The given colours take the master's most used hues; the other hues
    /// turn with the first.
    Brand {
        /// The brand's colours, as `#RRGGBB`.
        colours: Vec<String>,
    },
    /// The scene's own colours swapped with one another, for a scene whose
    /// palette is locked: pairs of the closest luminance trade places.
    Swap {
        /// What the generator drew the pairs from.
        seed: u64,
    },
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scheme::Rotate { degrees } => write!(f, "rotate {degrees:.0}°"),
            Scheme::Analogous { hue } => write!(f, "analogous {hue:.0}°"),
            Scheme::Complementary { hue } => write!(f, "complementary {hue:.0}°"),
            Scheme::Invert => write!(f, "invert"),
            Scheme::Brand { colours } => write!(f, "brand {}", colours.join(" ")),
            Scheme::Swap { .. } => write!(f, "swap (palette locked)"),
        }
    }
}

/// The schemes a spec may name.
pub const SCHEMES: [&str; 5] = ["rotate", "analogous", "complementary", "invert", "brand"];

/// Draws a scheme from those `names`, or every unlocked one; `brand` needs
/// `brand` colours. A locked palette only swaps.
pub(crate) fn draw(names: &[String], brand: &[String], locked: bool, rng: &mut Rng) -> Scheme {
    if locked {
        return Scheme::Swap { seed: rng.next() };
    }
    let names: Vec<&str> = match names.is_empty() {
        true if brand.is_empty() => SCHEMES[..4].to_vec(),
        true => vec!["brand"],
        false => names.iter().map(String::as_str).collect(),
    };
    let name = rng.pick(&names).copied().unwrap_or("rotate");
    let hue = rng.range(0.0, 360.0);
    match name {
        "analogous" => Scheme::Analogous { hue },
        "complementary" => Scheme::Complementary { hue },
        "invert" => Scheme::Invert,
        "brand" if !brand.is_empty() => Scheme::Brand {
            colours: brand.to_vec(),
        },
        // A turn of less than 60° reads as the same palette.
        _ => Scheme::Rotate {
            degrees: rng.range(60.0, 300.0).round(),
        },
    }
}

/// The keys whose values are colours in some object of the format.
static COLOUR_KEYS: LazyLock<BTreeSet<&'static str>> = LazyLock::new(|| {
    let mut keys: BTreeSet<&'static str> = ["bg", "fill", "stroke", "color", "colour"].into();
    let registries: Vec<&'static Registry> = [&LAYER, &GROUP]
        .into_iter()
        .chain(
            SHAPES
                .iter()
                .chain(&EFFECTS)
                .chain(&ANIMATORS)
                .chain(&MASKS)
                .map(|kind| kind.registry),
        )
        .collect();
    for registry in registries {
        keys.extend(
            registry
                .rows
                .iter()
                .filter(|row| row.ty == ValueType::Color)
                .map(|row| row.name),
        );
    }
    keys
});

/// Keys whose strings are words, names or files, never colours.
const WORDS: [&str; 12] = [
    "text", "id", "label", "font", "image", "video", "audio", "comp", "parent", "matte", "path",
    "svg",
];

/// A colour by its 8-bit channels, which is how two uses are the same.
type Key = [u8; 3];

fn key([r, g, b, _]: Rgba) -> Key {
    [r, g, b].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// One colour of the palette: how often it is used.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    /// The colour, opaque.
    pub colour: Rgba,
    /// How many values use it.
    pub uses: usize,
}

/// The palette of `raw`: every colour a place the locks leave free uses,
/// most used first, then by value.
pub(crate) fn gather(raw: &Json, held: &Held, slots: &[Listed]) -> Vec<Entry> {
    let mut counts: BTreeMap<Key, usize> = BTreeMap::new();
    let mut copy = raw.clone();
    rewrite(&mut copy, held, slots, &mut |c| {
        *counts.entry(key(c)).or_default() += 1;
        None
    });
    let mut entries: Vec<Entry> = counts
        .into_iter()
        .map(|(k, uses)| Entry {
            colour: [
                f64::from(k[0]) / 255.0,
                f64::from(k[1]) / 255.0,
                f64::from(k[2]) / 255.0,
                1.0,
            ],
            uses,
        })
        .collect();
    entries.sort_by(|a, b| b.uses.cmp(&a.uses).then(key(a.colour).cmp(&key(b.colour))));
    entries
}

/// What a scheme makes of each colour of a palette.
#[derive(Debug, Clone, Default)]
pub(crate) struct Mapping(BTreeMap<Key, Rgba>);

impl Mapping {
    /// The new colour for `c`, its alpha kept, when the mapping moves it.
    fn get(&self, c: Rgba) -> Option<Rgba> {
        let [r, g, b, _] = *self.0.get(&key(c))?;
        Some([r, g, b, c[3]])
    }

    /// How many colours it moves.
    pub(crate) fn len(&self) -> usize {
        self.0.iter().filter(|(k, c)| key(**c) != **k).count()
    }
}

/// The mapping `scheme` makes of `palette`.
pub(crate) fn mapping(palette: &[Entry], scheme: &Scheme) -> Mapping {
    let chromatic: Vec<&Entry> = palette
        .iter()
        .filter(|e| colour::to_lch(e.colour).c >= colour::NEUTRAL)
        .collect();
    // The hue the most used chromatic colour has: the palette's own.
    let main = chromatic
        .first()
        .map_or(0.0, |e| colour::to_lch(e.colour).h);
    let mut map = BTreeMap::new();
    let mut put = |from: Rgba, to: Rgba| {
        map.insert(key(from), to);
    };
    let turned = |e: &Entry, hue: f64| {
        let lch = colour::to_lch(e.colour);
        colour::with_luminance(hue, lch.c, colour::luminance(e.colour), 1.0)
    };
    match scheme {
        Scheme::Rotate { degrees } => {
            for e in &chromatic {
                put(e.colour, turned(e, colour::to_lch(e.colour).h + degrees));
            }
        }
        Scheme::Analogous { hue } => {
            // The master's hues, measured from its main one, squeezed into
            // 60° around the new middle.
            let offsets: Vec<f64> = chromatic
                .iter()
                .map(|e| signed(colour::to_lch(e.colour).h - main))
                .collect();
            let widest = offsets.iter().fold(0.0_f64, |m, o| m.max(o.abs()));
            let squeeze = match widest > 30.0 {
                true => 30.0 / widest,
                false => 1.0,
            };
            for (e, offset) in chromatic.iter().zip(offsets) {
                put(e.colour, turned(e, hue + offset * squeeze));
            }
        }
        Scheme::Complementary { hue } => {
            for e in &chromatic {
                let offset = signed(colour::to_lch(e.colour).h - main);
                let to = match offset.abs() <= 90.0 {
                    true => hue + offset * 0.25,
                    false => hue + 180.0 + signed(offset - 180.0) * 0.25,
                };
                put(e.colour, turned(e, to));
            }
        }
        Scheme::Invert => {
            for e in palette {
                let lch = colour::to_lch(e.colour);
                let y = mirrored(colour::luminance(e.colour));
                put(e.colour, colour::with_luminance(lch.h, lch.c, y, 1.0));
            }
        }
        Scheme::Brand { colours } => {
            let brand: Vec<Rgba> = colours
                .iter()
                .filter_map(|c| nakkash_format::color(&Json::String(c.clone())))
                .collect();
            let mut free: Vec<&Entry> = chromatic.clone();
            let mut turn = None;
            for b in &brand {
                // The most used colour closest to the brand colour's place.
                let Some(i) = (0..free.len()).min_by(|&i, &j| {
                    let cost = |e: &Entry| colour::distance(*b, e.colour) / (e.uses as f64).sqrt();
                    cost(free[i]).total_cmp(&cost(free[j]))
                }) else {
                    break;
                };
                let e = free.remove(i);
                turn.get_or_insert(colour::to_lch(*b).h - colour::to_lch(e.colour).h);
                put(e.colour, *b);
            }
            for e in free {
                put(
                    e.colour,
                    turned(e, colour::to_lch(e.colour).h + turn.unwrap_or(0.0)),
                );
            }
        }
        Scheme::Swap { seed } => {
            // The scene's own colours only: pairs of neighbouring luminance
            // trade places, chosen by the seed.
            let mut order: Vec<&Entry> = palette.iter().collect();
            order.sort_by(|a, b| {
                colour::luminance(a.colour)
                    .total_cmp(&colour::luminance(b.colour))
                    .then(key(a.colour).cmp(&key(b.colour)))
            });
            let mut rng = Rng::new(*seed, 0, "swap");
            let mut i = usize::from(rng.unit() < 0.5).min(order.len().saturating_sub(2));
            while i + 1 < order.len() {
                let (a, b) = (order[i].colour, order[i + 1].colour);
                put(a, b);
                put(b, a);
                i += 2;
            }
        }
    }
    Mapping(map)
}

/// A hue difference in `-180..180`.
fn signed(degrees: f64) -> f64 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

/// The luminance that stands to every other mirrored one as `y` stands to
/// it unmirrored, in WCAG's ratio: `(y + 0.05)` becomes its reciprocal,
/// scaled so black and white trade places.
fn mirrored(y: f64) -> f64 {
    (1.05 * 0.05 / (y + 0.05) - 0.05).clamp(0.0, 1.0)
}

/// Applies `mapping` to `raw`, leaving what the locks hold; returns how
/// many values changed.
pub(crate) fn apply(raw: &mut Json, held: &Held, slots: &[Listed], mapping: &Mapping) -> usize {
    let mut changed = 0;
    rewrite(raw, held, slots, &mut |c| {
        let to = mapping.get(c)?;
        changed += usize::from(key(to) != key(c));
        Some(to)
    });
    changed
}

/// Calls `map` on every colour of `raw` that the locks leave free, and
/// writes back what it returns, in the form the colour was written in.
fn rewrite(
    raw: &mut Json,
    held: &Held,
    slots: &[Listed],
    map: &mut impl FnMut(Rgba) -> Option<Rgba>,
) {
    // A slot is held by its lock or its unseen mark, or by a held value
    // that follows it.
    let frozen: BTreeSet<(Option<String>, String)> = slots
        .iter()
        .filter(|listed| {
            held.holds_slot(listed.comp.as_deref(), listed.slot.id.as_ref())
                || listed.followers.iter().any(|f| held.holds_follower(f))
        })
        .map(|listed| (listed.comp.clone(), listed.slot.id.to_string()))
        .collect();
    places::each_mut(raw, |visit| match visit {
        Visit::Layer { place, layer } => {
            if !held.holds(place) {
                object(layer, map);
            }
        }
        Visit::Override { on, place, keys } => {
            if !held.holds(on) && !held.holds(place) {
                object(keys, map);
            }
        }
        Visit::SlotValue { on, value, .. } => {
            if !held.holds(on) {
                slot_value(value, map);
            }
        }
        // Not a layer, so no lock but `palette` holds it, and that one
        // only reassigns the scene's own colours.
        Visit::Background { value, .. } => {
            whole(value, map);
        }
        Visit::Slots { comp, slots } => {
            for (id, value) in slots.iter_mut() {
                let owner = comp.map(str::to_owned);
                if !frozen.contains(&(owner, id.clone())) {
                    slot_value(value, map);
                }
            }
        }
    });
}

/// A slot's value: a colour, or `{"color": …}`.
fn slot_value(value: &mut Json, map: &mut impl FnMut(Rgba) -> Option<Rgba>) {
    match value {
        Json::Object(fields) => {
            if let Some(c) = fields.get_mut("color") {
                whole(c, map);
            }
        }
        other => {
            whole(other, map);
        }
    }
}

fn object(fields: &mut Map<String, Json>, map: &mut impl FnMut(Rgba) -> Option<Rgba>) {
    for (name, value) in fields.iter_mut() {
        walk(name, value, map);
    }
}

/// The values under key `name`.
fn walk(name: &str, value: &mut Json, map: &mut impl FnMut(Rgba) -> Option<Rgba>) {
    let colour_key = COLOUR_KEYS.contains(name);
    match value {
        Json::Object(fields) => object(fields, map),
        Json::Array(items) => {
            if colour_key && (3..=4).contains(&items.len()) && whole(value, map) {
                return;
            }
            if let Json::Array(items) = value {
                for item in items {
                    match item {
                        // A key of a colour's key list, `{"t":…,"v":…}`:
                        // its value is that colour.
                        Json::Object(key) if colour_key => {
                            for (field, v) in key.iter_mut() {
                                walk(if field == "v" { name } else { field }, v, map);
                            }
                        }
                        _ => walk(name, item, map),
                    }
                }
            }
        }
        Json::String(text) => {
            if WORDS.contains(&name) || !text.contains('#') {
                return;
            }
            if !whole(value, map) && colour_key {
                inline(value, map);
            }
        }
        _ => {}
    }
}

/// Maps `value` when it is one colour; says whether it was.
fn whole(value: &mut Json, map: &mut impl FnMut(Rgba) -> Option<Rgba>) -> bool {
    let Some(c) = colour_of(value) else {
        return false;
    };
    if let Some(to) = map(c) {
        *value = match value {
            Json::Array(_) => Json::Array(
                to.iter()
                    .map(|c| Json::from((c * 1e6).round() / 1e6))
                    .collect(),
            ),
            _ => Json::String(colour::hex(to)),
        };
    }
    true
}

/// `value` as a colour, if it is written as one.
fn colour_of(value: &Json) -> Option<Rgba> {
    match value {
        Json::String(text) if text.starts_with('#') => nakkash_format::color(value),
        Json::Array(items) if (3..=4).contains(&items.len()) => nakkash_format::color(value),
        _ => None,
    }
}

/// Maps each `#hex` word inside a string, such as a track written
/// `"#3B82F6 -> #8B5CF6 @0-5s"`.
fn inline(value: &mut Json, map: &mut impl FnMut(Rgba) -> Option<Rgba>) {
    let Json::String(text) = value else { return };
    let mut out = String::with_capacity(text.len());
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let as_json = Json::String(word.to_owned());
        match colour_of(&as_json).and_then(&mut *map) {
            Some(to) => out.push_str(&colour::hex(to)),
            None => out.push_str(word),
        }
    }
    *text = out;
}

#[cfg(test)]
#[path = "../../tests/unit/vary/palette.rs"]
mod tests;
