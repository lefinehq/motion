//! From a decoded [`Project`] to a scene ([D32]): one composition becomes the
//! scene, every composition it draws becomes an entry of `comps`, and every
//! layer, shape, effect and text animator becomes the scene's own, through
//! the tables in `rows`. The scene is built as JSON and read back with
//! [`parse_document`], so an import never yields a scene the format rejects;
//! the [`Mapping`] says what was mapped, defaulted, ignored or not supported.
//!
//! [D32]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d32-one-format-for-authored-and-imported-scenes

use std::collections::{BTreeMap, HashMap};

use motion_format::{Address, Diagnostic, Document, Kind, Result, parse_document};
use serde_json::{Map, Value as Json, json};

use serde::Serialize;

use crate::bake::{Ask, Layouts};
use crate::model::{ComposedLine, Composition, Item, ItemKind, Layer, LayerKind, Project};

mod curves;
mod expressions;
mod fonts;
mod footage;
mod ids;
mod items;
pub(crate) mod keys;
mod layer;
mod masks;
mod panel;
mod parts;
mod report;
mod roles;
mod rows;
mod slots;
pub(crate) mod sources;
mod text;

pub use footage::{STILL_EXTENSIONS, file_name};
pub use report::{
    ControlReport, Counts, ExpressionReport, Finding, Mapping, Named, Outcome, PanelReport,
};
pub use sources::Sources;

use ids::Ids;
use keys::num;

/// What to import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// The composition that becomes the scene, by name, or by item id when
    /// no composition has that name; by default the one composition no
    /// other draws.
    pub comp: Option<String>,
    /// Font files and folders for the scene's `fonts`, relative to where the
    /// scene will be saved.
    pub fonts: Vec<String>,
    /// The footage files found on disk: for each path the project stores,
    /// the path an image layer names, relative to where the scene will be
    /// saved or absolute. A still or clip the map lacks is reported as not
    /// found.
    pub footage: BTreeMap<String, String>,
    /// What the caller read of each found clip, by the path the project
    /// stores: a clip in a codec with a decoder becomes a video layer, one
    /// in another is reported by its codec, and one missing here is
    /// reported as video footage.
    pub videos: BTreeMap<String, Video>,
    /// What the caller read of each found sound file, by the path the
    /// project stores: a sound in a format with a decoder becomes an audio
    /// layer, one in another is reported by its format, and one missing
    /// here is reported as a format the import cannot read.
    pub sounds: BTreeMap<String, Sound>,
    /// What the caller read of each found vector footage file, by the path
    /// the project stores ([D70]): one it could read becomes an image layer
    /// drawn from its outlines, with what it holds that cannot be drawn
    /// reported, and one it could not is reported by its format.
    ///
    /// [D70]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines
    pub vectors: BTreeMap<String, Vector>,
    /// The panel of the template the project came from, for a `.mogrt`
    /// ([`crate::template`]): its controls become slots, and the
    /// composition it was made from is the scene unless `comp` names
    /// another.
    pub panel: Vec<crate::template::Control>,
    /// The rectangles of layers' content measured in the scene an earlier
    /// import wrote, which expressions read (`sourceRectAtTime`); what they
    /// read and this lacks is in [`Import::unmeasured`].
    pub layouts: Layouts,
}

/// What a found clip is, as the caller read it from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    /// The name of its codec, as reports say it (`H.264`).
    pub codec: String,
    /// Whether a video layer can decode it.
    pub decodes: bool,
}

/// What a found vector footage file is, as the caller read it ([D70]).
///
/// [D70]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    /// The name of its format, as reports say it (`PDF`).
    pub format: String,
    /// Whether it could be read; the rest is empty when not.
    pub reads: bool,
    /// The width and height of its page, in the page's units.
    pub page: [f64; 2],
    /// The bounds of its artwork, `[x, y, width, height]` from the page's
    /// top-left corner, y down, when the file records them.
    pub art: Option<[f64; 4]>,
    /// What it holds that the scene cannot draw, as reports say it.
    pub unsupported: Vec<String>,
}

/// What a found sound file is, as the caller read it from its first bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    /// The name of its format, as reports say it (`MP3`).
    pub format: String,
    /// Whether an audio layer can decode it.
    pub decodes: bool,
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
    /// The layout the project stored for each text layer it wrote, to hold
    /// the scene's own layout against.
    pub texts: Vec<StoredText>,
    /// Every font the scene's text is set in, by PostScript name, with where
    /// each is used: what [`Import::substitute_fonts`] may replace.
    pub fonts: BTreeMap<String, Vec<String>>,
    /// Where the scene holds values the project stores, and how each goes
    /// back: what an export writes ([D45]).
    ///
    /// [D45]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d45-export-patches-the-project-it-came-from
    pub sources: Sources,
    /// The rectangles of layers' content its expressions read that
    /// [`Options::layouts`] lacks: measured in this scene and given to the
    /// next import, they let those expressions bake.
    pub unmeasured: Vec<Unmeasured>,
}

/// A rectangle of a layer's content an expression read, not measured yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Unmeasured {
    /// What was read.
    pub ask: Ask,
    /// Where the layer is written in the scene ([`Address::written`]);
    /// none when the scene leaves it out.
    pub layer: Option<Address>,
    /// Whether every expression of the layer's content baked: what the
    /// scene draws of it will not change when other rectangles are
    /// measured.
    pub settled: bool,
}

/// The lines a project laid one text layer out in, at its first text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredText {
    /// Where the layer is written in the scene: `title`, or
    /// `scene.comps.card.layers[title]` for a composition's
    /// ([`Address::written`]).
    pub layer: Address,
    /// Its lines, in the layer's coordinates.
    pub lines: Vec<ComposedLine>,
}

/// Imports `project` as a scene.
///
/// # Errors
/// A diagnostic when the composition to import cannot be chosen (none or
/// several draw no other, or `options.comp` names none), or, as a bug in
/// the import, when the scene it wrote is not a valid scene.
pub fn import(project: &Project, options: &Options) -> Result<Import> {
    let made_from = match (&options.comp, options.panel.is_empty()) {
        (None, false) => panel_composition(project),
        _ => None,
    };
    let root = match made_from {
        Some(root) => root,
        None => root(project, options.comp.as_deref())?,
    };
    let bakes = crate::bake::bake_with(project, Some(root.0.id), &options.layouts);
    let files = footage::Files {
        found: &options.footage,
        videos: &options.videos,
        sounds: &options.sounds,
        vectors: &options.vectors,
    };
    let mut walk = Walk::new(project, files, &bakes);
    let mut json = walk.scene(root, options);
    let sources = Sources(
        sources::extract(&mut json)
            .into_iter()
            .filter_map(|(path, index)| {
                let writable = walk.sources.get(index)?.clone();
                Some(sources::Source { path, writable })
            })
            .collect(),
    );
    let mut document = read_back(&json)?;
    let offered = walk
        .mapping
        .panel
        .as_ref()
        .map(|panel| panel.ids.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|id| json["slots"][id.as_str()].get("text").is_some())
        .collect::<Vec<_>>();
    let unmeasured = bakes
        .asks
        .iter()
        .map(|ask| Unmeasured {
            ask: *ask,
            layer: walk.places.get(&(ask.comp, ask.layer)).cloned(),
            settled: !bakes.unsettled.contains(&(ask.comp, ask.layer)),
        })
        .collect();
    walk.mapping.roles = roles::propose(&document, &walk.named_roles, &offered);
    if !walk.mapping.roles.is_empty() {
        json["roles"] = roles::written(&walk.mapping.roles);
        // A master with a logo locks it, its palette and its typefaces.
        if walk.mapping.roles.iter().any(|r| r.role == "logo") {
            json["locks"] = serde_json::json!(roles::DEFAULT_LOCKS);
            walk.mapping.locks = roles::DEFAULT_LOCKS.map(str::to_owned).to_vec();
        }
        document = read_back(&json)?;
    }
    Ok(Import {
        json,
        document,
        mapping: walk.mapping,
        texts: walk.texts,
        fonts: walk.fonts,
        sources,
        unmeasured,
    })
}

/// The one composition with a template panel, which the template was made
/// from.
fn panel_composition(project: &Project) -> Option<(&Item, &Composition)> {
    let mut with_panel = project.items.iter().filter_map(|item| match &item.kind {
        ItemKind::Composition(c) if !c.panel.is_empty() => Some((item, c)),
        _ => None,
    });
    let found = with_panel.next()?;
    with_panel.next().is_none().then_some(found)
}

/// The scene the import wrote, read back.
///
/// # Errors
/// An invalid diagnostic, as a bug in the import, when the format rejects it.
fn read_back(json: &Json) -> Result<Document> {
    parse_document(&json.to_string()).map_err(|error| {
        Diagnostic::new(
            Kind::Invalid,
            format!("the import wrote a scene the format rejects: {error}"),
        )
    })
}

/// How the composition to import is asked for, for its errors.
const COMP_EXPECTED: &str =
    "the composition to import, by name or item id (`--comp <name>`, `--comp <id>`)";

/// The composition `name` names, or else the one composition no layer
/// draws. A name is a composition's name or, when no composition is called
/// that, its item id; a name several compositions share is refused.
fn root<'p>(project: &'p Project, name: Option<&str>) -> Result<(&'p Item, &'p Composition)> {
    let compositions: Vec<(&Item, &Composition)> = project
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Composition(composition) => Some((item, composition)),
            _ => None,
        })
        .collect();
    if compositions.is_empty() {
        return Err(
            Diagnostic::new(Kind::Missing, "the project holds no composition to import")
                .expected("a project with at least one composition"),
        );
    }
    if let Some(name) = name {
        return named(&compositions, name);
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
        [] => Err(Diagnostic::new(
            Kind::Missing,
            "every composition is drawn by another, so none is the scene",
        )
        .expected(COMP_EXPECTED)),
        several => Err(Diagnostic::new(
            Kind::Conflict,
            format!(
                "{} compositions are drawn by no other, so none is the scene: {}",
                several.len(),
                listed(several)
            ),
        )
        .expected(COMP_EXPECTED)),
    }
}

/// The one composition of `compositions` called `name`, or else whose item
/// id `name` is.
fn named<'p>(
    compositions: &[(&'p Item, &'p Composition)],
    name: &str,
) -> Result<(&'p Item, &'p Composition)> {
    let called: Vec<(&Item, &Composition)> = compositions
        .iter()
        .filter(|(item, _)| item.name == name)
        .copied()
        .collect();
    let by_id = || {
        let id: u32 = name.parse().ok()?;
        compositions.iter().find(|(item, _)| item.id == id).copied()
    };
    match called.as_slice() {
        [one] => Ok(*one),
        [] => by_id().ok_or_else(|| {
            Diagnostic::unknown(
                "composition",
                name,
                compositions.iter().map(|(item, _)| item.name.as_str()),
            )
        }),
        several => Err(Diagnostic::new(
            Kind::Conflict,
            format!(
                "{} compositions are called `{name}`: {}",
                several.len(),
                listed(several)
            ),
        )
        .expected("one of them by its item id (`--comp <id>`)")),
    }
}

/// `compositions` by name and item id, for an error.
fn listed(compositions: &[(&Item, &Composition)]) -> String {
    compositions
        .iter()
        .map(|(item, _)| format!("`{}` (id {})", item.name, item.id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The walk over one project, with what it has written so far.
struct Walk<'p> {
    /// The project.
    project: &'p Project,
    /// Every item, by id.
    items: HashMap<u32, &'p Item>,
    /// The ids of the compositions met so far, by item id.
    comp_ids: HashMap<u32, String>,
    /// The scene ids of the layers of each composition being mapped, the
    /// innermost last, by project layer id, with the layer being mapped.
    layer_scopes: Vec<(HashMap<u32, String>, u32)>,
    /// The ids given out to compositions.
    taken: Ids,
    /// The compositions met but not written yet, in the order met.
    pending: Vec<u32>,
    /// The report.
    mapping: Mapping,
    /// The scene composition whose layers are being written, or none for
    /// the scene's own.
    scope: Option<String>,
    /// The stored layout of each text layer written so far.
    texts: Vec<StoredText>,
    /// What each expression selector of the text layers met counts, by its
    /// amount's address, or why that cannot be known.
    part_counts: HashMap<usize, std::result::Result<crate::parts::Counted, String>>,
    /// Where each layer written so far is, by comp item id and layer id.
    places: HashMap<(u32, u32), Address>,
    /// The fonts the text written so far is set in, with where.
    fonts: BTreeMap<String, Vec<String>>,
    /// The footage files found, and what the clips and sounds among them
    /// are.
    files: footage::Files<'p>,
    /// The project's expressions, evaluated.
    bakes: &'p crate::bake::Bakes,
    /// How each value marked in the scene goes back into the project.
    sources: Vec<sources::Writable>,
    /// The item id of the composition imported as the scene.
    root: Option<u32>,
    /// The composition and layer being written.
    writing: Option<(&'p Composition, &'p Layer)>,
    /// The slots the controls read so far became, by scope.
    slots: slots::Slots,
    /// The layers named for a role so far.
    named_roles: Vec<roles::Named>,
    /// The slot of the panel control that drives each property, by what
    /// holds it.
    offered: HashMap<slots::Held, usize>,
    /// The scene ids of the masks of the layer being written that its
    /// effects follow, by the id the project gives each mask.
    mask_ids: HashMap<u32, String>,
}

impl<'p> Walk<'p> {
    /// A walk over `project`, with the footage `files` found for it and
    /// its expressions evaluated.
    fn new(
        project: &'p Project,
        files: footage::Files<'p>,
        bakes: &'p crate::bake::Bakes,
    ) -> Walk<'p> {
        Walk {
            project,
            items: project.items.iter().map(|item| (item.id, item)).collect(),
            comp_ids: HashMap::new(),
            layer_scopes: Vec::new(),
            taken: Ids::default(),
            pending: Vec::new(),
            mapping: Mapping::default(),
            scope: None,
            texts: Vec::new(),
            part_counts: HashMap::new(),
            places: HashMap::new(),
            fonts: BTreeMap::new(),
            files,
            bakes,
            sources: Vec::new(),
            root: None,
            writing: None,
            slots: slots::Slots::default(),
            named_roles: Vec::new(),
            offered: HashMap::new(),
            mask_ids: HashMap::new(),
        }
    }

    /// Numbers `writable`, for a mark in the scene.
    fn source(&mut self, writable: sources::Writable) -> usize {
        self.sources.push(writable);
        self.sources.len() - 1
    }

    /// The scene: the root composition's canvas and layers, the compositions
    /// they draw, and the fonts.
    fn scene(&mut self, (item, root): (&'p Item, &'p Composition), options: &Options) -> Json {
        self.mapping.scene.clone_from(&item.name);
        self.root = Some(item.id);
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
        if let Some(written) = shutter(root) {
            scene.insert("shutter".into(), written);
        }
        if !options.fonts.is_empty() {
            scene.insert("fonts".into(), json!(options.fonts));
        }
        if !options.panel.is_empty() {
            self.offer_panel(&options.panel);
        }
        let layers = self.layers(item.id, root, &item.name);
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
            if let Some(written) = shutter(composition) {
                comp.insert("shutter".into(), written);
            }
            self.scope = Some(self.comp_ids[&id].clone());
            let layers = Json::Array(self.layers(id, composition, name));
            comps.insert(self.comp_ids[&id].clone(), (comp, layers));
        }
        // A control's slot holds what most of its reads gave, so references
        // are written once every layer is.
        let mut decided =
            std::mem::take(&mut self.slots).decide(&mut self.mapping, &mut self.sources);
        if let Some(declared) = decided.declared.remove(&None) {
            scene.insert("slots".into(), Json::Object(declared));
        }
        if !comps.is_empty() {
            let comps: Map<String, Json> = comps
                .into_iter()
                .map(|(id, (mut comp, mut layers))| {
                    slots::write_references(&mut layers, &decided);
                    if let Some(declared) = decided.declared.remove(&Some(id.clone())) {
                        comp.insert("slots".into(), Json::Object(declared));
                    }
                    comp.insert("layers".into(), layers);
                    (id, Json::Object(comp))
                })
                .collect();
            scene.insert("comps".into(), Json::Object(comps));
        }
        let mut layers = Json::Array(layers);
        slots::write_references(&mut layers, &decided);
        scene.insert("layers".into(), layers);
        Json::Object(scene)
    }

    /// The id of the composition `id`, given the first time it is met, when
    /// it is queued to be written.
    fn comp_id(&mut self, item: &Item) -> String {
        if let Some(known) = self.comp_ids.get(&item.id) {
            return known.clone();
        }
        let given = self.taken.give(&item.name, "comp");
        self.mapping.ids.push(Named {
            comp: None,
            name: item.name.clone(),
            id: given.clone(),
        });
        self.comp_ids.insert(item.id, given.clone());
        self.pending.push(item.id);
        given
    }

    /// The layers of `composition`, the item `item` called `name`, bottom
    /// first as a scene stacks them.
    fn layers(&mut self, item: u32, composition: &'p Composition, name: &str) -> Vec<Json> {
        let mut ids = Ids::default();
        let given: HashMap<u32, String> = composition
            .layers
            .iter()
            .map(|layer| {
                let (role, named) = roles::named(&layer.name);
                let id = ids.give(named, "layer");
                if let Some(role) = role {
                    self.named_roles
                        .push((role, self.scope.clone(), id.clone()));
                }
                self.mapping.ids.push(Named {
                    comp: Some(name.to_owned()),
                    name: layer.name.clone(),
                    id: id.clone(),
                });
                (layer.id, id)
            })
            .collect();
        // An effect may read only a layer the scene keeps.
        let kept = composition
            .layers
            .iter()
            .filter(|layer| !layer::is_dropped(layer, composition))
            .filter_map(|layer| Some((layer.id, given.get(&layer.id)?.clone())))
            .collect();
        self.layer_scopes.push((kept, 0));
        let layers = composition
            .layers
            .iter()
            .rev()
            .filter_map(|layer| {
                if let Some(scope) = self.layer_scopes.last_mut() {
                    scope.1 = layer.id;
                }
                // A control layer is still walked, so its controls are
                // reported and the reads of them decided.
                let written = self.layer(layer, composition, name, &given);
                match layer::is_dropped(layer, composition) {
                    true => {
                        self.mapping.ignore("layer", layer::DROPPED);
                        None
                    }
                    false => {
                        let id = given[&layer.id]
                            .parse()
                            .expect("the import writes valid ids");
                        let at = Address::written(self.scope.as_deref(), id);
                        self.places.insert((item, layer.id), at);
                        Some(written)
                    }
                }
            })
            .collect();
        self.layer_scopes.pop();
        self.renderer(composition, name);
        layers
    }

    /// Reports the renderer of `composition`, called `name`, where it has
    /// 3D layers and draws them with another renderer than the classic one,
    /// which is how the scene draws them.
    fn renderer(&mut self, composition: &Composition, name: &str) {
        let spatial = composition
            .layers
            .iter()
            .any(|layer| layer.switches.three_d || layer.kind == LayerKind::Camera);
        if let Some(renderer) = composition.renderer.as_ref().filter(|r| !r.is_classic())
            && spatial
        {
            self.mapping.unsupported(
                name,
                format!(
                    "the `{}` renderer (drawn as the classic renderer draws)",
                    renderer.name
                ),
            );
        }
    }
}

/// A time in seconds as the scene writes it.
fn seconds(value: f64) -> String {
    format!("{}s", num(value))
}

/// A composition's `shutter`, where its motion blur is on and its shutter
/// is not the scene's default ([D57]); the samples are clamped into the
/// scene's range.
///
/// [D57]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter
fn shutter(composition: &Composition) -> Option<Json> {
    let blur = composition.motion_blur;
    let default = motion_format::shorthand::Shutter::DEFAULT;
    let samples = blur.samples.clamp(2, 64);
    let same =
        blur.angle == default.angle && blur.phase == default.phase && samples == default.samples;
    (blur.on && !same).then(|| json!(format!("{} {} {samples}", blur.angle, blur.phase)))
}

#[cfg(test)]
#[path = "../tests/unit/map.rs"]
mod tests;
