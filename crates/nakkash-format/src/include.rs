//! Compositions in files of their own ([D91]): an entry of a scene's
//! `comps` may be `{"include":"comps/card.json"}`, the composition written
//! in that file, so an agent can open one section of a long scene.
//!
//! A scene is flattened before anything else reads it: each included file
//! takes its entry's place, and the scene is then the one file it would
//! have been. The include's path is relative to the scene file; so is every
//! file the included composition names, as for every composition of the
//! scene, so a composition moves into a file of its own and back unchanged.
//! An included composition is all in its file: it holds no other include,
//! and the entry holds nothing beside its `include`.
//!
//! The format reads no files: [`flatten`] takes what reads one.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use serde_json::{Map, Value as Json};

use crate::parse::key::{COMPS, INCLUDE, comp_path};
use crate::{Address, Diagnostic, Kind, Place, Result, Step};

/// The written form of an included composition.
const FORM: &str =
    r#"a composition, or {"include":"comps/<id>.json"} for one in a file of its own"#;

/// Whether any composition of `raw` is in a file of its own.
pub fn has_includes(raw: &Json) -> bool {
    raw.get(COMPS)
        .and_then(Json::as_object)
        .is_some_and(|comps| comps.values().any(|comp| comp.get(INCLUDE).is_some()))
}

/// `raw`, a scene's JSON, with every included composition read by `read`
/// from the path its entry gives, as written, in its entry's place.
///
/// # Errors
/// The diagnostic `read` gives for a file, or an invalid-value one at an
/// entry that is not an include alone, or a file that is not a composition
/// or includes another, each at `comps.<id>.include`.
pub fn flatten(raw: &Json, mut read: impl FnMut(&str) -> Result<String>) -> Result<Json> {
    let mut flat = raw.clone();
    let Some(comps) = flat.get_mut(COMPS).and_then(Json::as_object_mut) else {
        return Ok(flat);
    };
    for (id, comp) in comps.iter_mut() {
        let Some(include) = comp.get(INCLUDE) else {
            continue;
        };
        let at = || {
            comp_path(id).join(Place::Scene(Address {
                layers: Vec::new(),
                keys: vec![Step::Key(INCLUDE.to_owned())],
            }))
        };
        let (Some(path), 1) = (include.as_str(), comp.as_object().map_or(0, Map::len)) else {
            return Err(
                Diagnostic::new(Kind::Invalid, format!("the composition `{comp}`"))
                    .expected(FORM)
                    .at(comp_path(id)),
            );
        };
        let text = read(path).map_err(|e| e.at(at()))?;
        let included: Json = serde_json::from_str(&text).map_err(|e| {
            Diagnostic::syntax(format!("invalid JSON in `{path}`: {e}"))
                .expected(FORM)
                .at(at())
        })?;
        if !included.is_object() {
            return Err(
                Diagnostic::new(Kind::Invalid, format!("`{path}` holds no composition"))
                    .expected("a composition object, e.g. {\"size\":\"600x300\",\"layers\":[]}")
                    .at(at()),
            );
        }
        if included.get(INCLUDE).is_some() {
            return Err(
                Diagnostic::new(Kind::Invalid, format!("`{path}` includes another file"))
                    .expected("the composition itself: an included composition is all in its file")
                    .at(at()),
            );
        }
        *comp = included;
    }
    Ok(flat)
}

/// `raw`, a scene's JSON, split: the scene with each composition's entry an
/// include of `comps/<id>.json`, and each composition with the path its
/// file takes, in the scene's order.
pub fn split(raw: &Json) -> (Json, Vec<(String, Json)>) {
    let mut top = raw.clone();
    let mut files = Vec::new();
    if let Some(comps) = top.get_mut(COMPS).and_then(Json::as_object_mut) {
        for (id, comp) in comps.iter_mut() {
            let path = format!("comps/{id}.json");
            let mut entry = Map::new();
            entry.insert(INCLUDE.to_owned(), Json::String(path.clone()));
            files.push((path, std::mem::replace(comp, Json::Object(entry))));
        }
    }
    (top, files)
}

/// An error for a scene whose compositions are in files, read without them.
pub(crate) fn unread() -> Diagnostic {
    Diagnostic::new(
        Kind::Missing,
        "a scene whose compositions are in files of their own, read without them",
    )
    .expected(
        "the scene opened from its file, which reads each `include`, or `nakkash flatten` first",
    )
}

#[cfg(test)]
#[path = "../tests/unit/include.rs"]
mod tests;
