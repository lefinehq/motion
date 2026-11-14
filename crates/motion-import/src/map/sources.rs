//! Where the import wrote what the project stores ([D45]): while an object
//! is written, each value read from a property is marked with how it can
//! go back; once the scene is built, the marks become paths and leave it.
//!
//! [D45]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d45-export-patches-the-project-it-came-from

use serde_json::{Map, Value as Json};

use super::keys::Convert;
use crate::model::{Origin, Value};

/// The prefix of a mark's key in an object being written.
const MARK: &str = "$source:";

/// One step of a path into a scene: a key of an object, the element of an
/// array whose `id` it is, or else an element by index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Step {
    /// A key of an object.
    Key(String),
    /// The element of an array with this `id`.
    Id(String),
    /// The element of an array at this index, for elements with no `id`.
    Index(usize),
}

/// How a value the import wrote can go back into the project.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Writable {
    /// A static value, stored at `origin`: the scene value converts back
    /// through `convert` into `template`'s kind.
    Static {
        /// Where the project stores it.
        origin: Origin,
        /// How the import converted it.
        convert: Convert,
        /// The project's value, whose other components an edit keeps.
        template: Value,
    },
    /// Set by an expression, whose stored value would change nothing.
    Expression,
    /// Keyed: keys are not written back yet.
    Keyed,
}

/// A value the import wrote, where it is, and how it goes back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Source {
    /// Where it is in the scene.
    pub path: Vec<Step>,
    /// How it goes back.
    pub writable: Writable,
}

/// Every value the import wrote that the project stores.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sources(pub(crate) Vec<Source>);

/// The marks of an object being written.
#[derive(Debug, Default)]
pub(super) struct Marks(Vec<(String, usize)>);

impl Marks {
    /// Marks `path` (keys and indices joined by `/`) as written from the
    /// source numbered `index`.
    pub(super) fn add(&mut self, path: String, index: usize) {
        self.0.push((path, index));
    }

    /// Renames marks under the key `from` to `to`, where the writer moves a
    /// value to another key.
    pub(super) fn rename(&mut self, from: &str, to: &str) {
        for (path, _) in &mut self.0 {
            let first = path.split('/').next().unwrap_or_default();
            if first == from {
                *path = format!("{to}{}", &path[first.len()..]);
            }
        }
    }

    /// Puts the marks into `keys`, to be found by [`extract`].
    pub(super) fn write(self, keys: &mut Map<String, Json>) {
        for (path, index) in self.0 {
            keys.insert(format!("{MARK}{path}"), Json::from(index));
        }
    }
}

/// Takes every mark out of `json`, and gives, for each, the path it marks
/// and its source number.
pub(super) fn extract(json: &mut Json) -> Vec<(Vec<Step>, usize)> {
    let mut found = Vec::new();
    walk(json, &mut Vec::new(), &mut found);
    found.sort();
    found
}

fn walk(json: &mut Json, path: &mut Vec<Step>, found: &mut Vec<(Vec<Step>, usize)>) {
    match json {
        Json::Object(map) => {
            let marks: Vec<String> = map
                .keys()
                .filter(|k| k.starts_with(MARK))
                .cloned()
                .collect();
            for key in marks {
                let index = map.remove(&key).and_then(|v| v.as_u64());
                let Some(index) = index.and_then(|i| usize::try_from(i).ok()) else {
                    continue;
                };
                let mut marked = path.clone();
                marked.extend(key[MARK.len()..].split('/').map(|part| match part.parse() {
                    Ok(i) => Step::Index(i),
                    Err(_) => Step::Key(part.to_owned()),
                }));
                found.push((marked, index));
            }
            for (key, value) in map.iter_mut() {
                path.push(Step::Key(key.clone()));
                walk(value, path, found);
                path.pop();
            }
        }
        Json::Array(items) => {
            for (i, item) in items.iter_mut().enumerate() {
                path.push(step(item, i));
                walk(item, path, found);
                path.pop();
            }
        }
        _ => {}
    }
}

/// The step to the element `item` at index `i` of an array.
pub(crate) fn step(item: &Json, i: usize) -> Step {
    match item.get("id").and_then(Json::as_str) {
        Some(id) => Step::Id(id.to_owned()),
        None => Step::Index(i),
    }
}

/// The value at `path` in `json`.
pub(crate) fn at<'j>(json: &'j Json, path: &[Step]) -> Option<&'j Json> {
    path.iter().try_fold(json, |json, step| match step {
        Step::Key(key) => json.get(key),
        Step::Index(i) => json.get(i),
        Step::Id(id) => json
            .as_array()?
            .iter()
            .find(|item| item.get("id").and_then(Json::as_str) == Some(id)),
    })
}

/// A path as people read it: `layers[title].position[0]`.
pub(crate) fn show(path: &[Step]) -> String {
    let mut text = String::new();
    for step in path {
        match step {
            Step::Key(key) if text.is_empty() => text.push_str(key),
            Step::Key(key) => {
                text.push('.');
                text.push_str(key);
            }
            Step::Id(id) => text.push_str(&format!("[{id}]")),
            Step::Index(i) => text.push_str(&format!("[{i}]")),
        }
    }
    text
}
