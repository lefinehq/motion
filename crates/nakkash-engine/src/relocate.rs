//! A scene written to another folder keeps reading its files: the paths
//! it names relative to its own folder are written relative to the new
//! one.

use std::path::{Component, Path, PathBuf};

use serde_json::Value as Json;

/// The keys whose string values are files relative to the scene's folder:
/// the scene's `fonts`, and a layer's `image`, `video` and `audio`.
const FILES: [&str; 3] = ["image", "video", "audio"];

/// Rewrites every relative file `raw` names from the folder `from` to the
/// folder `to`.
pub(crate) fn files(raw: &mut Json, from: &Path, to: &Path) {
    let moved = |path: &mut String| {
        if Path::new(path.as_str()).is_relative()
            && let Some(relative) = relative(&from.join(path.as_str()), to)
        {
            *path = relative;
        }
    };
    if let Some(Json::Array(fonts)) = raw.get_mut("fonts") {
        fonts
            .iter_mut()
            .filter_map(|font| match font {
                Json::String(path) => Some(path),
                _ => None,
            })
            .for_each(moved);
    }
    fn walk(json: &mut Json, moved: &dyn Fn(&mut String)) {
        match json {
            Json::Object(fields) => {
                for (key, value) in fields.iter_mut() {
                    match (FILES.contains(&key.as_str()), value) {
                        (true, Json::String(path)) => moved(path),
                        (_, value) => walk(value, moved),
                    }
                }
            }
            Json::Array(items) => items.iter_mut().for_each(|item| walk(item, moved)),
            _ => {}
        }
    }
    for key in ["layers", "comps"] {
        if let Some(part) = raw.get_mut(key) {
            walk(part, &moved);
        }
    }
}

/// `path` relative to the folder `base`, as [`relative`] writes it: how an
/// instance's file is named from its template's folder ([D91]).
///
/// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
pub(crate) fn relative_to(path: &Path, base: &Path) -> Option<String> {
    relative(path, base)
}

/// `path` relative to the folder `base`, both made absolute, written with
/// `/`.
fn relative(path: &Path, base: &Path) -> Option<String> {
    let clean = |p: &Path| -> Option<Vec<String>> {
        let absolute = std::path::absolute(p).ok()?;
        let mut parts: Vec<String> = Vec::new();
        for component in absolute.components() {
            match component {
                Component::ParentDir => _ = parts.pop(),
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            }
        }
        Some(parts)
    };
    let (path, base) = (clean(path)?, clean(base)?);
    let shared = path.iter().zip(&base).take_while(|(a, b)| a == b).count();
    let ups = std::iter::repeat_n("..".to_owned(), base.len() - shared);
    let parts: Vec<String> = ups.chain(path[shared..].iter().cloned()).collect();
    Some(match parts.is_empty() {
        true => ".".to_owned(),
        false => PathBuf::from_iter(&parts)
            .to_string_lossy()
            .replace('\\', "/"),
    })
}

#[cfg(test)]
#[path = "../tests/unit/relocate.rs"]
mod tests;
