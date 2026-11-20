//! The canonical form of a scene, `nakkash.Canon.1`, and the hash that
//! names a template by its content ([D91]): the scene's JSON written
//! compact, every object's keys in sorted order, numbers and strings as
//! read. Writing a scene pretty, compact or with its keys in another order
//! leaves its canonical form, and so its hash, as it was; any other change
//! moves it.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use serde_json::{Map, Value as Json};

use crate::Sha256;

/// The name of the canonical form this module writes.
pub const CANON: &str = "nakkash.Canon.1";

/// The canonical text of `raw`, a scene's JSON.
pub fn canonical(raw: &Json) -> String {
    serde_json::to_string(&sorted(raw)).expect("JSON values always write")
}

/// The hash of `raw`'s canonical text, `sha256:<64 hex digits>`.
pub fn hash(raw: &Json) -> String {
    format!("sha256:{}", Sha256::of(canonical(raw).as_bytes()))
}

/// `json` with the keys of every object sorted.
fn sorted(json: &Json) -> Json {
    match json {
        Json::Object(fields) => {
            let mut keys: Vec<&String> = fields.keys().collect();
            keys.sort();
            Json::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&fields[k])))
                    .collect::<Map<_, _>>(),
            )
        }
        Json::Array(items) => Json::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/canon.rs"]
mod tests;
