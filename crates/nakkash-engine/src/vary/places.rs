//! Where values sit in a scene's JSON, and which of those places the
//! scene's locks hold ([D51]). The palette, type and content axes change
//! values in place, and never one a lock holds ([D78]).
//!
//! A composition's layers are written once and drawn by every instance,
//! so a value in one is held when any instance of it is: a lock on
//! `card-2/title` holds `title` of `card` for every card.
//!
//! [D51]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d51-locks-are-refused-in-the-edit
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::BTreeSet;

use nakkash_format::{Lock, Scene};
use serde_json::{Map, Value as Json};

/// The JSON keys this module reads.
const LAYERS: &str = "layers";
const COMPS: &str = "comps";
const COMP: &str = "comp";
const ID: &str = "id";
const OVERRIDES: &str = "overrides";

/// One layer as written: the composition whose list holds it (none for
/// the scene's root) and its id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Place {
    /// The composition, or `None` for the root.
    pub comp: Option<String>,
    /// The layer's id.
    pub layer: String,
}

/// What a scene's locks hold, as places in its JSON.
#[derive(Debug, Clone, Default)]
pub(crate) struct Held {
    /// Layers held, and through them everything inside.
    places: BTreeSet<Place>,
    /// Compositions held whole: drawn by a held layer.
    comps: BTreeSet<String>,
    /// Slots held, each as its composition (none for the scene's own) and
    /// id: those a lock holds, and those `nakkash slots --verify --mark`
    /// found change nothing drawn (`"seen": false`, [D77]), whose value a
    /// variant would move with nothing to show for it.
    ///
    /// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
    slots: BTreeSet<(Option<String>, String)>,
    /// Only the scene's own colours may be set.
    pub palette: bool,
    /// Only the scene's own font families may be set.
    pub typefaces: bool,
}

impl Held {
    /// What `scene`'s locks hold in `raw`, its JSON.
    pub(crate) fn of(scene: &Scene, raw: &Json) -> Held {
        let mut held = Held::default();
        for (lock, _) in scene.held_locks() {
            match lock {
                Lock::Layer(address) => {
                    let ids: Vec<String> = address.layers.iter().map(|id| id.to_string()).collect();
                    if let Some(place) = resolve(raw, None, &ids) {
                        if let Some(comp) = comp_of(raw, &place) {
                            held.hold_comp(raw, &comp);
                        }
                        held.places.insert(place);
                    }
                }
                Lock::Slot(id) => {
                    held.slots.insert((None, id.to_string()));
                }
                Lock::Palette => held.palette = true,
                Lock::Typefaces => held.typefaces = true,
                Lock::Role(_) => {}
            }
        }
        let comps = std::iter::once((None, &scene.root))
            .chain(scene.comps.iter().map(|comp| (Some(comp.id.clone()), comp)));
        for (owner, comp) in comps {
            for slot in comp.slots.iter().filter(|slot| !slot.seen) {
                held.slots.insert((owner.clone(), slot.id.to_string()));
            }
        }
        held
    }

    /// Whether slot `id` of composition `comp` (none for the scene's own)
    /// is held: by a lock, or as one that changes nothing drawn.
    pub(crate) fn holds_slot(&self, comp: Option<&str>, id: &str) -> bool {
        self.slots
            .contains(&(comp.map(str::to_owned), id.to_owned()))
    }

    /// Holds composition `comp` and every one it draws.
    fn hold_comp(&mut self, raw: &Json, comp: &str) {
        if !self.comps.insert(comp.to_owned()) {
            return;
        }
        let inner: Vec<String> = layers_of(raw, Some(comp))
            .iter()
            .filter_map(|layer| layer.get(COMP).and_then(Json::as_str).map(str::to_owned))
            .collect();
        for comp in inner {
            self.hold_comp(raw, &comp);
        }
    }

    /// Whether a lock holds the layer at `place`.
    pub(crate) fn holds(&self, place: &Place) -> bool {
        self.places.contains(place) || place.comp.as_ref().is_some_and(|c| self.comps.contains(c))
    }

    /// Whether a lock holds the place a slot's follower names, as
    /// [`nakkash_format::Listed`] writes it: `plate.fill` on the root,
    /// `comps.card.title.text` in a composition.
    pub(crate) fn holds_follower(&self, follower: &str) -> bool {
        let parts: Vec<&str> = follower.split('.').collect();
        let place = match parts.as_slice() {
            ["comps", comp, layer, ..] => Place {
                comp: Some((*comp).to_owned()),
                layer: (*layer).to_owned(),
            },
            [layer, ..] => Place {
                comp: None,
                layer: (*layer).to_owned(),
            },
            [] => return false,
        };
        self.holds(&place)
    }
}

/// The layers of composition `comp`, or of the root.
pub(crate) fn layers_of<'a>(raw: &'a Json, comp: Option<&str>) -> &'a [Json] {
    let list = match comp {
        None => raw.get(LAYERS),
        Some(comp) => raw
            .get(COMPS)
            .and_then(|c| c.get(comp))
            .and_then(|c| c.get(LAYERS)),
    };
    list.and_then(Json::as_array).map_or(&[], Vec::as_slice)
}

/// The composition the layer at `place` draws, if it is a `comp` layer.
pub(crate) fn comp_of(raw: &Json, place: &Place) -> Option<String> {
    layers_of(raw, place.comp.as_deref())
        .iter()
        .find(|layer| layer.get(ID).and_then(Json::as_str) == Some(place.layer.as_str()))?
        .get(COMP)
        .and_then(Json::as_str)
        .map(str::to_owned)
}

/// The place `ids`, an address relative to composition `from`, leads to.
pub(crate) fn resolve(raw: &Json, from: Option<&str>, ids: &[String]) -> Option<Place> {
    let (last, path) = ids.split_last()?;
    let mut comp = from.map(str::to_owned);
    for id in path {
        comp = Some(comp_of(
            raw,
            &Place {
                comp: comp.clone(),
                layer: id.clone(),
            },
        )?);
    }
    Some(Place {
        comp,
        layer: last.clone(),
    })
}

/// Calls `visit` on every layer the scene writes, root first and then each
/// composition's in the order the file has them, with its place; on each
/// override a `comp` layer sets, with the place it overrides; on each list
/// of slots, the scene's first, with the composition it belongs to; and on
/// the background colour of the scene and of each composition that has
/// one.
pub(crate) fn each_mut(raw: &mut Json, mut visit: impl FnMut(Visit<'_>)) {
    let snapshot = raw.clone();
    let comps: Vec<String> = raw
        .get(COMPS)
        .and_then(Json::as_object)
        .map(|c| c.keys().cloned().collect())
        .unwrap_or_default();
    if let Some(slots) = raw.get_mut("slots").and_then(Json::as_object_mut) {
        visit(Visit::Slots { comp: None, slots });
    }
    let lists = std::iter::once(None).chain(comps.iter().map(|c| Some(c.as_str())));
    for comp in lists {
        let target = match comp {
            None => Some(&mut *raw),
            Some(id) => raw.get_mut(COMPS).and_then(|c| c.get_mut(id)),
        };
        let Some(target) = target else { continue };
        if let Some(id) = comp
            && let Some(slots) = target.get_mut("slots").and_then(Json::as_object_mut)
        {
            visit(Visit::Slots {
                comp: Some(id),
                slots,
            });
        }
        if let Some(value) = target.get_mut("bg") {
            visit(Visit::Background { value });
        }
        let Some(layers) = target.get_mut(LAYERS).and_then(Json::as_array_mut) else {
            continue;
        };
        for layer in layers.iter_mut().filter_map(Json::as_object_mut) {
            let Some(id) = layer.get(ID).and_then(Json::as_str).map(str::to_owned) else {
                continue;
            };
            let place = Place {
                comp: comp.map(str::to_owned),
                layer: id,
            };
            let drawn = layer.get(COMP).and_then(Json::as_str).map(str::to_owned);
            let mut overrides = layer.remove(OVERRIDES);
            visit(Visit::Layer {
                place: &place,
                layer,
            });
            if let (Some(drawn), Some(Json::Object(entries))) = (&drawn, &mut overrides) {
                for (target, keys) in entries.iter_mut() {
                    if target.starts_with('$') {
                        visit(Visit::SlotValue {
                            on: &place,
                            value: keys,
                        });
                        continue;
                    }
                    let Some(keys) = keys.as_object_mut() else {
                        continue;
                    };
                    let ids: Vec<String> = target.split('/').map(str::to_owned).collect();
                    if let Some(inner) = resolve(&snapshot, Some(drawn), &ids) {
                        visit(Visit::Override {
                            on: &place,
                            place: &inner,
                            keys,
                        });
                    }
                }
            }
            if let Some(overrides) = overrides {
                layer.insert(OVERRIDES.to_owned(), overrides);
            }
        }
    }
}

/// One thing [`each_mut`] hands out.
pub(crate) enum Visit<'a> {
    /// A layer's keys, without its `overrides`.
    Layer {
        place: &'a Place,
        layer: &'a mut Map<String, Json>,
    },
    /// The keys an override of the `comp` layer at `on` sets on the layer
    /// at `place`.
    Override {
        on: &'a Place,
        place: &'a Place,
        keys: &'a mut Map<String, Json>,
    },
    /// The value the `comp` layer at `on` gives one of its composition's
    /// slots in its overrides.
    SlotValue { on: &'a Place, value: &'a mut Json },
    /// The background colour of the scene, or of a composition.
    Background { value: &'a mut Json },
    /// A list of slots: the scene's, or a composition's.
    Slots {
        comp: Option<&'a str>,
        slots: &'a mut Map<String, Json>,
    },
}
