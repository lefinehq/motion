//! From a decoded [`Project`] to a scene ([D32]): one composition becomes the
//! scene, every composition it draws becomes an entry of `comps`, and every
//! layer, shape, effect and text animator becomes the scene's own, through
//! the tables in `rows`. The scene is built as JSON and read back with
//! [`parse_document`], so an import never yields a scene the format rejects;
//! the [`Mapping`] says what was mapped, defaulted, ignored or not supported.
//!
//! [D32]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d32-one-format-for-authored-and-imported-scenes

use std::collections::{BTreeMap, HashMap};

use motion_format::{Diagnostic, Document, Kind, Result, parse_document};
use serde_json::{Map, Value as Json, json};

use crate::model::{Composition, Item, ItemKind, Project};

mod ids;
mod items;
mod keys;
mod layer;
mod report;
mod rows;
mod text;

pub use report::{Counts, Finding, Mapping, Named};

use ids::Ids;
use keys::num;

/// What to import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// The composition that becomes the scene, by name; by default the one
    /// composition no other draws.
    pub comp: Option<String>,
    /// Font files and folders for the scene's `fonts`, relative to where the
    /// scene will be saved.
    pub fonts: Vec<String>,
}

/// An imported scene and what the import did.
#[derive(Debug)]
pub struct Import {
    /// The scene as JSON, as `parse_document` read it.
    pub json: Json,
    /// The scene, read.
    pub document: Document,
    /// What was mapped, defaulted, ignored or not supported.
    pub mapping: Mapping,
}

/// Imports `project` as a scene.
///
/// # Errors
/// A diagnostic when the composition to import cannot be chosen (none or
/// several draw no other, or `options.comp` names none), or, as a bug in
/// the import, when the scene it wrote is not a valid scene.
pub fn import(project: &Project, options: &Options) -> Result<Import> {
    let root = root(project, options.comp.as_deref())?;
    let mut walk = Walk::new(project);
    let json = walk.scene(root, options);
    let document = parse_document(&json.to_string()).map_err(|error| {
        Diagnostic::new(
            Kind::Invalid,
            format!("the import wrote a scene the format rejects: {error}"),
        )
    })?;
    Ok(Import {
        json,
        document,
        mapping: walk.mapping,
    })
}

/// The composition `name` names, or else the one composition no layer draws.
fn root<'p>(project: &'p Project, name: Option<&str>) -> Result<(&'p Item, &'p Composition)> {
    let compositions: Vec<(&Item, &Composition)> = project
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Composition(composition) => Some((item, composition)),
            _ => None,
        })
        .collect();
    let names = || compositions.iter().map(|(item, _)| item.name.as_str());
    if let Some(name) = name {
        return compositions
            .iter()
            .find(|(item, _)| item.name == name)
            .copied()
            .ok_or_else(|| Diagnostic::unknown("composition", name, names()));
    }
    let drawn: Vec<u32> = compositions
        .iter()
        .flat_map(|(_, composition)| &composition.layers)
        .filter_map(|layer| layer.source)
        .collect();
    let roots: Vec<(&Item, &Composition)> = compositions
        .iter()
        .filter(|(item, _)| !drawn.contains(&item.id))
        .copied()
        .collect();
    match roots.as_slice() {
        [root] => Ok(*root),
        _ => {
            let listed: Vec<String> = roots
                .iter()
                .map(|(item, _)| format!("`{}`", item.name))
                .collect();
            Err(Diagnostic::new(
                Kind::Missing,
                format!(
                    "{} compositions are drawn by no other, so none is the scene: {}",
                    roots.len(),
                    listed.join(", ")
                ),
            )
            .expected("the composition to import, by name (`--comp <name>`)"))
        }
    }
}

/// The walk over one project, with what it has written so far.
struct Walk<'p> {
    /// The project.
    project: &'p Project,
    /// Every item, by id.
    items: HashMap<u32, &'p Item>,
    /// The ids of the compositions met so far, by item id.
    comp_ids: HashMap<u32, String>,
    /// The ids given out to compositions.
    taken: Ids,
    /// The compositions met but not written yet, in the order met.
    pending: Vec<u32>,
    /// The report.
    mapping: Mapping,
}

impl<'p> Walk<'p> {
    /// A walk over `project`.
    fn new(project: &'p Project) -> Walk<'p> {
        Walk {
            project,
            items: project.items.iter().map(|item| (item.id, item)).collect(),
            comp_ids: HashMap::new(),
            taken: Ids::default(),
            pending: Vec::new(),
            mapping: Mapping::default(),
        }
    }

    /// The scene: the root composition's canvas and layers, the compositions
    /// they draw, and the fonts.
    fn scene(&mut self, (item, root): (&'p Item, &'p Composition), options: &Options) -> Json {
        self.mapping.scene.clone_from(&item.name);
        let scope = self.taken.give(&item.name, "scene");
        self.mapping.ids.push(Named {
            comp: None,
            name: item.name.clone(),
            id: scope.clone(),
        });
        let mut scene = Map::new();
        scene.insert("motion".into(), json!(1));
        scene.insert(
            "size".into(),
            json!(format!("{}x{}", root.width, root.height)),
        );
        scene.insert("fps".into(), num(root.frame_rate));
        scene.insert("dur".into(), json!(seconds(root.duration)));
        let [r, g, b] = [root.background.r, root.background.g, root.background.b]
            .map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8);
        scene.insert("bg".into(), json!(format!("#{r:02X}{g:02X}{b:02X}")));
        if !options.fonts.is_empty() {
            scene.insert("fonts".into(), json!(options.fonts));
        }
        let layers = self.layers(root, &item.name);
        let mut comps = BTreeMap::new();
        while let Some(id) = self.pending.pop() {
            let Some(Item {
                name,
                kind: ItemKind::Composition(composition),
                ..
            }) = self.items.get(&id).copied()
            else {
                continue;
            };
            let mut comp = Map::new();
            comp.insert(
                "size".into(),
                json!(format!("{}x{}", composition.width, composition.height)),
            );
            comp.insert("dur".into(), json!(seconds(composition.duration)));
            if composition.frame_rate != root.frame_rate {
                comp.insert("fps".into(), num(composition.frame_rate));
            }
            comp.insert("layers".into(), Json::Array(self.layers(composition, name)));
            comps.insert(self.comp_ids[&id].clone(), Json::Object(comp));
        }
        if !comps.is_empty() {
            scene.insert("comps".into(), json!(comps));
        }
        scene.insert("layers".into(), Json::Array(layers));
        Json::Object(scene)
    }

    /// The id of the composition `id`, given the first time it is met, when
    /// it is queued to be written.
    fn comp_id(&mut self, id: u32) -> Option<String> {
        if let Some(known) = self.comp_ids.get(&id) {
            return Some(known.clone());
        }
        let item = self.items.get(&id).copied()?;
        let given = self.taken.give(&item.name, "comp");
        self.mapping.ids.push(Named {
            comp: None,
            name: item.name.clone(),
            id: given.clone(),
        });
        self.comp_ids.insert(id, given.clone());
        self.pending.push(id);
        Some(given)
    }

    /// The layers of `composition`, called `name`, bottom first as a scene
    /// stacks them.
    fn layers(&mut self, composition: &'p Composition, name: &str) -> Vec<Json> {
        let mut ids = Ids::default();
        let given: HashMap<u32, String> = composition
            .layers
            .iter()
            .map(|layer| {
                let id = ids.give(&layer.name, "layer");
                self.mapping.ids.push(Named {
                    comp: Some(name.to_owned()),
                    name: layer.name.clone(),
                    id: id.clone(),
                });
                (layer.id, id)
            })
            .collect();
        composition
            .layers
            .iter()
            .rev()
            .map(|layer| self.layer(layer, composition, name, &given))
            .collect()
    }
}

/// A time in seconds as the scene writes it.
fn seconds(value: f64) -> String {
    format!("{}s", num(value))
}

#[cfg(test)]
#[path = "../tests/unit/map.rs"]
mod tests;
