//! The canonical form of a scene, `nakkash.Canon.1`, and the hash that
//! names a template by its content ([D91]): the scene's JSON written
//! compact, every object's keys in the order written, numbers and strings
//! as read. Writing a scene pretty or compact leaves its canonical form,
//! and so its hash, as it was; any other change moves it, the order of
//! keys included, since a layer keeps its resting values in the order
//! they are written.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use serde_json::Value as Json;

use crate::Sha256;

/// The name of the canonical form this module writes.
pub const CANON: &str = "nakkash.Canon.1";

/// The canonical text of `raw`, a scene's JSON.
pub fn canonical(raw: &Json) -> String {
    serde_json::to_string(raw).expect("JSON values always write")
}

/// The hash of `raw`'s canonical text, `sha256:<64 hex digits>`.
pub fn hash(raw: &Json) -> String {
    format!("sha256:{}", Sha256::of(canonical(raw).as_bytes()))
}

#[cfg(test)]
#[path = "../tests/unit/canon.rs"]
mod tests;
