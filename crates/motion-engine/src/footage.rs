//! Finding a project's footage files on this machine (step `footage`), so
//! an import can draw its still pictures as image layers ([D41]).
//!
//! A project stores each file's absolute path on the machine it was made
//! on, which rarely exists here. Templates ship their footage beside the
//! project instead, often in subfolders that keep the end of the stored
//! path (`(Footage)/Photos/beach.jpg`). So a stored path that exists is
//! taken as it is; otherwise every file under the project's folder with
//! the same name is a candidate, and the one whose path ends with the most
//! of the stored path's folders wins.
//!
//! [D41]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d41-image-layers-name-their-file-and-carry-their-box

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

use motion_import::map::file_name;
use motion_import::{ItemKind, Project, Source};

/// How many folders deep under the project's folder footage is looked for,
/// so a project saved in a large folder does not walk all of it.
const DEPTH: usize = 8;

/// The footage files of `project`, read from `project_path`, found on this
/// machine: for each stored path, the found file as an image layer names
/// it. Paths are relative to the folder of `scene`, where the scene will be
/// saved, or absolute without one. A file not found is left out.
pub(crate) fn find(
    project: &Project,
    project_path: &Path,
    scene: Option<&Path>,
) -> BTreeMap<String, String> {
    let stored: Vec<&str> = project
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Footage(footage) => match &footage.source {
                Source::File { path: Some(path) } => Some(path.as_str()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if stored.is_empty() {
        return BTreeMap::new();
    }
    let root = folder_of(project_path);
    let index = Index::of(&root);
    let base = scene.map(folder_of);
    stored
        .into_iter()
        .filter_map(|path| {
            let found = locate(path, &root, &index)?;
            Some((path.to_owned(), written(&found, base.as_deref())?))
        })
        .collect()
}

/// The file `stored` names: itself when it exists, else the best file of
/// the same name under `root`.
fn locate(stored: &str, root: &Path, index: &Index) -> Option<PathBuf> {
    let as_stored = Path::new(stored);
    if as_stored.is_absolute() && as_stored.is_file() {
        return fs::canonicalize(as_stored).ok();
    }
    let parts: Vec<String> = stored
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect();
    let candidates = index.files.get(&file_name(stored).to_lowercase())?;
    candidates
        .iter()
        .max_by_key(|candidate| {
            let shared = shared_tail(candidate.strip_prefix(root).unwrap_or(candidate), &parts);
            // Ties go to the shallowest file, then to the first by path.
            (
                shared,
                Reverse(candidate.components().count()),
                Reverse(*candidate),
            )
        })
        .cloned()
}

/// How many of `path`'s last parts are the last of `parts`, ignoring case.
fn shared_tail(path: &Path, parts: &[String]) -> usize {
    path.components()
        .rev()
        .zip(parts.iter().rev())
        .take_while(|(component, part)| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| name.to_lowercase() == **part)
        })
        .count()
}

/// The files under a folder, by lowercased name.
#[derive(Debug, Default)]
struct Index {
    files: HashMap<String, Vec<PathBuf>>,
}

impl Index {
    /// Every file under `root`, down to [`DEPTH`] folders; folders that
    /// cannot be read are passed over, and links to folders are not followed.
    fn of(root: &Path) -> Index {
        let mut index = Index::default();
        let mut folders = vec![(root.to_path_buf(), 0)];
        while let Some((folder, depth)) = folders.pop() {
            let Ok(entries) = fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if kind.is_dir() && depth < DEPTH {
                    folders.push((path, depth + 1));
                } else if kind.is_file()
                    && let Some(name) = path.file_name().and_then(|name| name.to_str())
                {
                    index
                        .files
                        .entry(name.to_lowercase())
                        .or_default()
                        .push(path.clone());
                }
            }
        }
        index
    }
}

/// The folder `path` is in, resolved, so relative paths from it hold.
fn folder_of(path: &Path) -> PathBuf {
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf())
}

/// `file` as an image layer names it: relative to `base`, with `/` between
/// its parts, or absolute without a base; `None` for a path that is not
/// UTF-8, which a scene cannot hold.
fn written(file: &Path, base: Option<&Path>) -> Option<String> {
    let Some(relative) = base.and_then(|base| pathdiff::diff_paths(file, base)) else {
        return file.to_str().map(str::to_owned);
    };
    let parts: Option<Vec<&str>> = relative
        .components()
        .map(|component| match component {
            Component::ParentDir => Some(".."),
            other => other.as_os_str().to_str(),
        })
        .collect();
    Some(parts?.join("/"))
}

#[cfg(test)]
#[path = "../tests/unit/footage.rs"]
mod tests;
