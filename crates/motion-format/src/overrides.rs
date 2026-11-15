//! Overrides as JSON ([D43]): how a `comp` layer's `overrides` are set over
//! the layers of the composition it draws, in the one place validation and
//! edits both read it from.
//!
//! An override's keys replace the same keys of the layer its address names.
//! An address deeper than one layer is handed on as an override of the
//! `comp` layer its first id names, so each level only ever sets its own
//! layers' keys.
//!
//! [D43]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d43-one-layer-address-and-edits-per-instance

use serde_json::{Map, Value};

use crate::Address;
use crate::address::Id;
use crate::parse::key::{COMP, COMPS, ID, LAYERS, OVERRIDES};

/// The keys one override sets.
pub type Keys = Map<String, Value>;

/// Sets `keys` over the layer `target` names among `layers`, the JSON of
/// one composition's layers: on the layer itself when `target` is one id,
/// else as an override of the `comp` layer its first id names. Returns
/// `false`, changing nothing, when no layer has that first id.
pub(crate) fn set_over(layers: &mut [Value], target: &[Id], keys: &Keys) -> bool {
    let Some((first, rest)) = target.split_first() else {
        return false;
    };
    let Some(layer) = layers
        .iter_mut()
        .find(|layer| written_id(layer) == Some(first.as_ref()))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    match rest {
        [] => layer.extend(keys.clone()),
        deeper => {
            let rest = Address::layer(deeper.iter().cloned()).to_string();
            let overrides = layer
                .entry(OVERRIDES)
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(entry) = overrides
                .as_object_mut()
                .map(|o| o.entry(rest).or_insert_with(|| Value::Object(Map::new())))
                .and_then(Value::as_object_mut)
            {
                entry.extend(keys.clone());
            }
        }
    }
    true
}

/// The id a layer's JSON writes, if it writes one as a string.
pub(crate) fn written_id(layer: &Value) -> Option<&str> {
    layer.get(ID).and_then(Value::as_str)
}

/// The JSON of the layer `ids` lead to from the scene's root in `raw`, a
/// scene as written, as that layer is drawn there: the composition's own
/// layer with every override on the way set over it. `None` when an id is
/// not there, or one on the way is not a `comp` layer.
pub fn layer_json(raw: &Value, ids: &[Id]) -> Option<Keys> {
    let (last, outer) = ids.split_last()?;
    let mut layers: Vec<Value> = raw.get(LAYERS)?.as_array()?.clone();
    for id in outer {
        let layer = layers
            .iter()
            .find(|layer| written_id(layer) == Some(id.as_ref()))?;
        let comp = layer.get(COMP)?.as_str()?;
        let mut inner: Vec<Value> = raw.get(COMPS)?.get(comp)?.get(LAYERS)?.as_array()?.clone();
        if let Some(overrides) = layer.get(OVERRIDES).and_then(Value::as_object) {
            for (target, keys) in overrides {
                let (Ok(address), Some(keys)) = (target.parse::<Address>(), keys.as_object())
                else {
                    continue;
                };
                set_over(&mut inner, &address.layers, keys);
            }
        }
        layers = inner;
    }
    layers
        .into_iter()
        .find(|layer| written_id(layer) == Some(last.as_ref()))
        .and_then(|layer| layer.as_object().cloned())
}

#[cfg(test)]
#[path = "../tests/unit/overrides.rs"]
mod tests;
