//! Finding a project's footage files on this machine (step `footage`), so
//! an import can draw its still pictures as image layers ([D41]), its
//! clips as video layers and play its sounds as audio layers; each clip
//! found is read far enough to know its codec, and each other file by its
//! first bytes for the sound format it is in. A file whose first bytes are
//! a vector format's ([D70]) is read whole, for its page and its art
//! bounds and for what it holds that cannot be drawn.
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
//! [D70]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

use motion_import::map::{STILL_EXTENSIONS, Sound, Vector, Video, file_name};
use motion_import::{ItemKind, Project, Source};

/// How many folders deep under the project's folder footage is looked for,
/// so a project saved in a large folder does not walk all of it.
const DEPTH: usize = 8;

/// What was found of a project's footage, by the path the project stores.
#[derive(Debug, Default)]
pub(crate) struct Found {
    /// Each file found, as a layer names it.
    pub(crate) files: BTreeMap<String, String>,
    /// Each found file that is a clip, and its codec.
    pub(crate) videos: BTreeMap<String, Video>,
    /// Each found file that is a sound, and its format.
    pub(crate) sounds: BTreeMap<String, Sound>,
    /// Each found file that is vector footage, and what it holds.
    pub(crate) vectors: BTreeMap<String, Vector>,
}

/// The footage files of `project`, read from `project_path`, found on this
/// machine: for each stored path, the found file as a layer names it, and
/// for each found clip its codec. Paths are relative to the folder of
/// `scene`, where the scene will be saved, or absolute without one. A file
/// not found is left out, and so is a clip whose codec cannot be read.
pub(crate) fn find(project: &Project, project_path: &Path, scene: Option<&Path>) -> Found {
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
    let mut found = Found::default();
    if stored.is_empty() {
        return found;
    }
    let root = folder_of(project_path);
    let index = Index::of(&root);
    let base = scene.map(folder_of);
    for path in stored {
        let Some(file) = locate(path, &root, &index) else {
            continue;
        };
        let Some(written) = written(&file, base.as_deref()) else {
            continue;
        };
        if let Some(vector) = vector(&file) {
            found.vectors.insert(path.to_owned(), vector);
            found.files.insert(path.to_owned(), written);
            continue;
        }
        match video(&file) {
            Some(video) => {
                found.videos.insert(path.to_owned(), video);
            }
            None => {
                if let Some(sound) = sound(&file) {
                    found.sounds.insert(path.to_owned(), sound);
                }
            }
        }
        found.files.insert(path.to_owned(), written);
    }
    found
}

/// The codec of the clip `file`, or `None` for a still picture or a file
/// that is not a clip this crate's reader understands.
fn video(file: &Path) -> Option<Video> {
    let still = file
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|extension| {
            STILL_EXTENSIONS
                .iter()
                .any(|still| extension.eq_ignore_ascii_case(still))
        });
    if still {
        return None;
    }
    let track = motion_video::container::read(&mut fs::File::open(file).ok()?).ok()?;
    Some(Video {
        codec: motion_video::codec_name(track.codec),
        decodes: motion_video::decodes(track.codec),
    })
}

/// What the vector footage `file` holds, or `None` for a file whose first
/// bytes are no vector format's.
fn vector(file: &Path) -> Option<Vector> {
    let bytes = head(file, motion_vector::HEAD)?;
    let format = motion_vector::recognise(&bytes)?;
    let read = fs::read(file)
        .ok()
        .and_then(|bytes| (format.read)(&bytes).ok());
    let Some(drawing) = read else {
        return Some(Vector {
            format: format.name.to_owned(),
            reads: false,
            page: [0.0, 0.0],
            art: None,
            unsupported: Vec::new(),
        });
    };
    let size = drawing.size();
    Some(Vector {
        format: format.name.to_owned(),
        reads: true,
        page: [size.width, size.height],
        art: drawing
            .art()
            .map(|art| [art.x0, art.y0, art.width(), art.height()]),
        unsupported: drawing.unsupported().to_vec(),
    })
}

/// The first `length` bytes of `file`, or fewer when it is shorter.
fn head(file: &Path, length: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut head = vec![0_u8; length];
    let mut reader = fs::File::open(file).ok()?;
    let mut read = 0;
    // A short read is not the end of the file.
    while read < head.len() {
        match reader.read(&mut head[read..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => read += n,
        }
    }
    head.truncate(read);
    Some(head)
}

/// The sound format of `file`, by its first bytes, or `None` for a file in
/// no sound format motion knows.
fn sound(file: &Path) -> Option<Sound> {
    // Enough for an MP3 stream to show consecutive frames.
    let head = head(file, 16 * 1024)?;
    motion_audio::recognise(&head).map(|(format, decodes)| Sound {
        format: format.to_owned(),
        decodes,
    })
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
