//! The project as an expression reads it: [`ProjectHost`], the [`Host`] over
//! a decoded [`Project`].
//!
//! - **Nodes.** Every comp, layer, group and property is a node, numbered in
//!   project order. An effect lists its parameters as its definition does,
//!   those the project leaves at their default among them, so `(1)` and
//!   `("Slider")` find what they find in the original.
//! - **Names.** A child is found by its index, its own name, the name the
//!   original shows for it, or its match name; an attribute such as
//!   `transform` or `sourceText` by the match names it stands for.
//! - **Units.** Values are in the units an expression sees: a layer's scale
//!   and opacity in percent, which the project keeps as fractions.
//! - **Dimensions.** A 2D layer's anchor point, position and scale have two
//!   components to an expression, as the original gives them; the project
//!   keeps a third, which a 3D layer's have.
//! - **Rectangles.** A layer with a source covers its source's box; a text
//!   or shape layer covers what its content draws, which the host reads
//!   from the [`Layouts`] given and asks for when they lack it
//!   ([`Missing::Unmeasured`]).
//! - **Other expressions.** A property with an expression reads as what its
//!   expression gave, once baked; until then it is [`Missing::Pending`], and
//!   the time asked for is kept for the bake to evaluate first.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};

use motion_expr::{Host, Missing, NodeId, NodeKind, Select, Val};

use super::{Ask, AskKey, Layouts};
use crate::model::{
    Child, Color, Composition, Control, Group, Item, ItemKind, Layer, LayerKind, Parameter,
    Project, Property, Stream, Value,
};

/// A node of the project, with what an expression may read of it. Nodes own
/// their data, as the engine keeps its host for as long as it lives.
#[derive(Debug, Clone)]
enum Node {
    /// The project.
    Project,
    /// A composition.
    Comp(Comp),
    /// A layer.
    Layer(LayerInfo),
    /// A property group.
    Group,
    /// A property.
    Property(Box<Property>),
    /// An effect parameter the project leaves at its default, on a layer of
    /// the size given.
    Default(Box<Parameter>, [f64; 2]),
}

/// What an expression reads of a comp.
#[derive(Debug, Clone)]
struct Comp {
    item: u32,
    width: f64,
    height: f64,
    duration: f64,
    frame_rate: f64,
    pixel_aspect: f64,
    background: Color,
    layers: usize,
}

/// What an expression reads of a layer.
#[derive(Debug, Clone)]
struct LayerInfo {
    /// Its comp's item id and its own id.
    item: u32,
    id: u32,
    kind: LayerKind,
    in_point: f64,
    out_point: f64,
    start_time: f64,
    stretch: f64,
    parent: Option<u32>,
    size: [f64; 2],
    /// Its source's size, if it has a composition or footage as one.
    source: Option<[f64; 2]>,
}

/// One node with where it sits.
#[derive(Debug)]
struct Entry {
    node: Node,
    up: Option<NodeId>,
    children: Vec<NodeId>,
    /// Its match name; empty for the project, comps and layers.
    match_name: String,
    /// The name the original shows: the user's, else the default one.
    name: String,
    /// Its index among its siblings, from 1.
    index: usize,
    /// How many of an expression's units make one of the project's, per
    /// component.
    unit: Unit,
    /// How many components an expression sees of its value, where that is
    /// fewer than the project keeps: two of a 2D layer's anchor point,
    /// position and scale.
    components: Option<usize>,
}

/// How many of an expression's units make one of the project's, per
/// component; components past the last take 1.
pub(super) type Unit = [f64; 3];

/// What a property with an expression reads as, so far.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Baked {
    /// What its expression gave at each time asked, by the time's bits.
    Values(HashMap<u64, Val>),
    /// What its expression gives at every time: it read nothing that
    /// changes with time.
    Constant(Val),
    /// It could not be evaluated, with why.
    Failed(String),
}

/// A property whose expression is switched on, with where it is.
#[derive(Debug, Clone, Copy)]
pub(super) struct Target<'p> {
    /// Its node.
    pub node: NodeId,
    /// The property.
    pub property: &'p Property,
    /// Its layer.
    pub layer: &'p Layer,
    /// Its layer's comp.
    pub comp: &'p Composition,
    /// The comp's item id.
    pub item: u32,
    /// Whether it is part of what its layer draws: its text or its shapes,
    /// which a rectangle of the layer measures.
    pub content: bool,
}

/// Where the nodes being added sit: their layer, its comp and the comp's
/// item id, the layer's size, its source's size if it has a source, and
/// whether they are in its content.
#[derive(Debug, Clone, Copy)]
struct At<'p> {
    layer: &'p Layer,
    comp: &'p Composition,
    item: u32,
    size: [f64; 2],
    source: Option<[f64; 2]>,
    content: bool,
}

/// The groups that hold what a text or shape layer draws.
const CONTENT_GROUPS: [&str; 2] = ["ADBE Text Properties", "ADBE Root Vectors Group"];

/// The [`Host`] over a project.
#[derive(Debug)]
pub(super) struct ProjectHost {
    entries: Vec<Entry>,
    /// The layer node of each layer id, per comp item id.
    layers: HashMap<(u32, u32), NodeId>,
    /// The values of the properties with expressions, as baked so far.
    pub(super) baked: RefCell<HashMap<NodeId, Baked>>,
    /// The last value asked for that is not baked yet, and when.
    pub(super) demand: Cell<Option<(NodeId, f64)>>,
    /// The rectangles measured so far.
    layouts: Layouts,
    /// The rectangles read that [`ProjectHost::layouts`] lacks, in order.
    pub(super) asks: RefCell<Vec<Ask>>,
    /// What tells [`ProjectHost::asks`] apart, to keep each once.
    asked: RefCell<BTreeSet<AskKey>>,
}

/// The project node.
const PROJECT: NodeId = 0;

/// What building the host walks with.
struct Build<'p> {
    project: &'p Project,
    host: ProjectHost,
    targets: Vec<Target<'p>>,
}

impl ProjectHost {
    /// The host over `project`, reading layers' rectangles from `layouts`,
    /// and the properties whose expressions are on.
    pub(super) fn new<'p>(
        project: &'p Project,
        layouts: &Layouts,
    ) -> (ProjectHost, Vec<Target<'p>>) {
        let mut build = Build {
            project,
            host: ProjectHost {
                entries: vec![Entry {
                    node: Node::Project,
                    up: None,
                    children: Vec::new(),
                    match_name: String::new(),
                    name: String::new(),
                    index: 1,
                    unit: [1.0; 3],
                    components: None,
                }],
                layers: HashMap::new(),
                baked: RefCell::new(HashMap::new()),
                demand: Cell::new(None),
                layouts: layouts.clone(),
                asks: RefCell::new(Vec::new()),
                asked: RefCell::new(BTreeSet::new()),
            },
            targets: Vec::new(),
        };
        let comps = project.items.iter().filter_map(|item| match &item.kind {
            ItemKind::Composition(comp) => Some((item, comp)),
            _ => None,
        });
        for (n, (item, comp)) in comps.enumerate() {
            let info = Comp {
                item: item.id,
                width: f64::from(comp.width),
                height: f64::from(comp.height),
                duration: comp.duration,
                frame_rate: comp.frame_rate,
                pixel_aspect: comp.pixel_aspect,
                background: comp.background,
                layers: comp.layers.len(),
            };
            let id = build
                .host
                .add(PROJECT, Node::Comp(info), "", item.name.clone(), n + 1);
            for (i, layer) in comp.layers.iter().enumerate() {
                let source = source_size(project, layer);
                let size = source.unwrap_or([f64::from(comp.width), f64::from(comp.height)]);
                let info = LayerInfo {
                    item: item.id,
                    id: layer.id,
                    kind: layer.kind,
                    in_point: layer.in_point,
                    out_point: layer.out_point,
                    start_time: layer.start_time,
                    stretch: layer.stretch,
                    parent: layer.parent,
                    size,
                    source,
                };
                let layer_id = build
                    .host
                    .add(id, Node::Layer(info), "", layer.name.clone(), i + 1);
                build.host.layers.insert((item.id, layer.id), layer_id);
                build.children(
                    layer_id,
                    &layer.properties,
                    None,
                    At {
                        layer,
                        comp,
                        item: item.id,
                        size,
                        source,
                        content: false,
                    },
                );
            }
        }
        (build.host, build.targets)
    }

    fn add(
        &mut self,
        up: NodeId,
        node: Node,
        match_name: &str,
        name: String,
        index: usize,
    ) -> NodeId {
        let id = NodeId::try_from(self.entries.len()).expect("fewer than 2^32 nodes");
        self.entries.push(Entry {
            node,
            up: Some(up),
            children: Vec::new(),
            match_name: match_name.to_owned(),
            unit: [unit(match_name); 3],
            components: None,
            name,
            index,
        });
        self.entries[up as usize].children.push(id);
        id
    }
}

impl<'p> Build<'p> {
    /// Adds `group`'s children under `up`. An effect's (`definition` given)
    /// are its definition's parameters first, from the project where it
    /// keeps them, else at their default.
    fn children(
        &mut self,
        up: NodeId,
        group: &'p Group,
        definition: Option<&'p crate::model::EffectDefinition>,
        at: At<'p>,
    ) {
        let mut index = 0;
        let mut listed: Vec<&str> = Vec::new();
        if let Some(definition) = definition {
            let parameters = definition
                .parameters
                .iter()
                .filter(|p| !p.match_name.ends_with("-0000"));
            for parameter in parameters {
                index += 1;
                listed.push(&parameter.match_name);
                let stored = group.children.iter().find_map(|c| match c {
                    Child::Property(p) if p.match_name == parameter.match_name => Some(p),
                    _ => None,
                });
                match stored {
                    Some(p) => {
                        let name = p.name.clone().unwrap_or_else(|| parameter.name.clone());
                        let id = self.property(up, p, name, index, at);
                        // A point parameter is stored in fractions of the
                        // layer's size, as its default is.
                        if let Control::Point { .. } = parameter.control {
                            self.host.entries[id as usize].unit = [at.size[0], at.size[1], 1.0];
                        }
                    }
                    None => {
                        let node = Node::Default(Box::new(parameter.clone()), at.size);
                        let name = parameter.name.clone();
                        self.host.add(up, node, &parameter.match_name, name, index);
                    }
                }
            }
        }
        let unlisted = group.children.iter().filter(|child| match child {
            Child::Property(p) => !listed.contains(&p.match_name.as_str()),
            Child::Group(_) => true,
        });
        for child in unlisted {
            index += 1;
            match child {
                Child::Group(g) => {
                    let name = g.name.clone().unwrap_or_else(|| self.default_name(g));
                    let id = self.host.add(up, Node::Group, &g.match_name, name, index);
                    let effect = (group.match_name == "ADBE Effect Parade")
                        .then(|| self.project.effects.get(&g.match_name))
                        .flatten();
                    let on_layer = matches!(self.host.entries[up as usize].node, Node::Layer(_));
                    let content =
                        at.content || (on_layer && CONTENT_GROUPS.contains(&g.match_name.as_str()));
                    self.children(id, g, effect, At { content, ..at });
                }
                Child::Property(p) => {
                    let name = p.name.clone().unwrap_or_else(|| {
                        display_name(&p.match_name)
                            .unwrap_or(&p.match_name)
                            .to_owned()
                    });
                    let id = self.property(up, p, name, index, at);
                    let transform = group.match_name == "ADBE Transform Group";
                    // A layer's anchor point is stored in fractions of its
                    // source, where it has one.
                    let anchor = transform && p.match_name == "ADBE Anchor Point";
                    if let Some([width, height]) = at.source.filter(|_| anchor) {
                        self.host.entries[id as usize].unit = [width, height, 1.0];
                    }
                    if transform && planar(at.layer, &p.match_name) {
                        self.host.entries[id as usize].components = Some(2);
                    }
                }
            }
        }
    }

    fn property(
        &mut self,
        up: NodeId,
        property: &'p Property,
        name: String,
        index: usize,
        at: At<'p>,
    ) -> NodeId {
        let node = Node::Property(Box::new(property.clone()));
        let id = self.host.add(up, node, &property.match_name, name, index);
        if property.expression.as_ref().is_some_and(|e| e.enabled) {
            self.targets.push(Target {
                node: id,
                property,
                layer: at.layer,
                comp: at.comp,
                item: at.item,
                content: at.content,
            });
        }
        id
    }

    /// The name the original shows for a group the user did not name.
    fn default_name(&self, group: &Group) -> String {
        self.project
            .effects
            .get(&group.match_name)
            .map(|d| d.name.clone())
            .or_else(|| display_name(&group.match_name).map(str::to_owned))
            .unwrap_or_else(|| group.match_name.clone())
    }
}

/// Whether `layer`'s transform property `match_name` has two components to
/// an expression where the project keeps three: the anchor point, position
/// and scale of a layer that is not 3D. A camera or a light is always 3D.
fn planar(layer: &Layer, match_name: &str) -> bool {
    let three_d =
        layer.switches.three_d || matches!(layer.kind, LayerKind::Camera | LayerKind::Light);
    !three_d
        && matches!(
            match_name,
            "ADBE Anchor Point" | "ADBE Position" | "ADBE Scale"
        )
}

/// The size of a layer's source, if it has a composition or footage as one.
fn source_size(project: &Project, layer: &Layer) -> Option<[f64; 2]> {
    let source = layer
        .source
        .and_then(|id| project.items.iter().find(|item: &&Item| item.id == id))?;
    match &source.kind {
        ItemKind::Composition(c) => Some([f64::from(c.width), f64::from(c.height)]),
        ItemKind::Footage(f) => Some([f64::from(f.width), f64::from(f.height)]),
        _ => None,
    }
}

impl ProjectHost {
    /// How many of an expression's units make one of the project's in each
    /// component of `node`'s value.
    pub(super) fn unit(&self, node: NodeId) -> Unit {
        self.entries.get(node as usize).map_or([1.0; 3], |e| e.unit)
    }

    /// A project value of `node` as an expression reads it: in its units,
    /// with as many components as it has to an expression.
    fn read(&self, node: NodeId, value: &Value) -> Result<Val, Missing> {
        let val = to_val(value, self.unit(node))?;
        let components = self.entries.get(node as usize).and_then(|e| e.components);
        Ok(match (val, components) {
            (Val::Vector(mut v), Some(n)) => {
                v.truncate(n);
                Val::Vector(v)
            }
            (val, _) => val,
        })
    }

    /// The comp node holding `node`.
    fn comp_of(&self, node: NodeId) -> Option<NodeId> {
        let mut at = Some(node);
        while let Some(id) = at {
            if matches!(self.entries[id as usize].node, Node::Comp(_)) {
                return Some(id);
            }
            at = self.entries[id as usize].up;
        }
        None
    }

    /// `property`'s own value at `time`, without its expression, in an
    /// expression's units.
    fn own_value(&self, node: NodeId, property: &Property, time: f64) -> Result<Val, Missing> {
        if property.separated && property.match_name == "ADBE Position" {
            return self.separated(node, time);
        }
        self.read(node, &property.value.value_at(time))
    }

    /// The `_0`, `_1` and `_2` siblings of a separated position that it has.
    fn parts(&self, node: NodeId) -> Vec<NodeId> {
        let up = self.entries[node as usize].up.unwrap_or(PROJECT);
        ["_0", "_1", "_2"]
            .iter()
            .filter_map(|suffix| {
                let wanted = format!("ADBE Position{suffix}");
                self.entries[up as usize]
                    .children
                    .iter()
                    .copied()
                    .find(|&c| self.entries[c as usize].match_name == wanted)
            })
            .collect()
    }

    /// A separated position: its `_0`, `_1` and `_2` siblings together.
    fn separated(&self, node: NodeId, time: f64) -> Result<Val, Missing> {
        let mut parts = Vec::new();
        let components = self.entries[node as usize].components.unwrap_or(3);
        for sibling in self.parts(node).into_iter().take(components) {
            match self.value_at(sibling, time, false)? {
                Val::Number(n) => parts.push(n),
                _ => {
                    return Err(Missing::Unknown(
                        "a position part that is not a number".into(),
                    ));
                }
            }
        }
        Ok(Val::Vector(parts))
    }

    /// The child of `node` an attribute stands for.
    fn attribute_child(&self, node: NodeId, name: &str) -> Option<NodeId> {
        let wanted = attribute_match_names(name)?;
        let entry = &self.entries[node as usize];
        let find = |children: &[NodeId]| {
            children
                .iter()
                .copied()
                .find(|&c| wanted.contains(&self.entries[c as usize].match_name.as_str()))
        };
        match (find(&entry.children), &entry.node) {
            (Some(found), _) => Some(found),
            // A layer reaches its transform's properties directly.
            (None, Node::Layer(_)) => {
                let transform = entry
                    .children
                    .iter()
                    .copied()
                    .find(|&c| self.entries[c as usize].match_name == "ADBE Transform Group")?;
                find(&self.entries[transform as usize].children)
            }
            _ => None,
        }
    }
}

/// How many of an expression's units make one of the project's: a layer's
/// scale and opacity are percent to an expression and fractions in the file.
fn unit(match_name: &str) -> f64 {
    match match_name {
        "ADBE Scale" | "ADBE Opacity" => 100.0,
        _ => 1.0,
    }
}

/// A project value as an expression reads it, each component scaled by
/// its `unit`.
fn to_val(value: &Value, unit: Unit) -> Result<Val, Missing> {
    match value {
        Value::Number(n) => Ok(Val::Number(n * unit[0])),
        Value::Text(document) => Ok(Val::Text(document.text.clone())),
        Value::Path(_) => Err(Missing::Unknown("a path's shape".into())),
        Value::Marker(marker) => Ok(Val::Marker(Box::new(motion_expr::Marker {
            comment: marker.comment.clone(),
            duration: marker.duration,
            chapter: marker.chapter.clone(),
            url: marker.url.clone(),
            frame_target: marker.frame_target.clone(),
            cue_point_name: marker.cue_point_name.clone(),
            parameters: marker.parameters.clone(),
            event_cue_point: marker.event_cue_point,
            protected_region: marker.protected_region,
        }))),
        Value::Opaque { reason } => Err(Missing::Unknown(format!(
            "a value the reader does not decode ({reason})"
        ))),
        other => Ok(Val::Vector(
            other
                .numbers()
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(i, n)| n * unit.get(i).copied().unwrap_or(1.0))
                .collect(),
        )),
    }
}

/// A parameter's default, as an expression reads it, on a layer of `size`.
fn default_val(parameter: &Parameter, size: [f64; 2]) -> Result<Val, Missing> {
    match &parameter.control {
        Control::Slider { default, .. } | Control::Angle { default } => Ok(Val::Number(*default)),
        Control::Checkbox { default } => Ok(Val::Number(f64::from(u8::from(*default)))),
        Control::Popup { default, .. } => Ok(Val::Number(f64::from(*default))),
        Control::Color { default } => Ok(Val::Vector(vec![
            default.r, default.g, default.b, default.a,
        ])),
        Control::Point { default } => Ok(Val::Vector(
            default
                .iter()
                .zip(size.iter().chain(std::iter::repeat(&1.0)))
                .map(|(f, s)| f * s)
                .collect(),
        )),
        Control::Layer => Err(Missing::Unknown("a layer parameter's layer".into())),
        Control::Other { .. } => Err(Missing::Unknown(format!(
            "the parameter `{}`",
            parameter.name
        ))),
    }
}

/// The name the original shows for a match name the project gives none.
fn display_name(match_name: &str) -> Option<&'static str> {
    Some(match match_name {
        "ADBE Transform Group" | "ADBE Vector Transform Group" => "Transform",
        "ADBE Effect Parade" => "Effects",
        "ADBE Mask Parade" => "Masks",
        "ADBE Root Vectors Group" | "ADBE Vectors Group" => "Contents",
        "ADBE Text Properties" => "Text",
        "ADBE Text Document" => "Source Text",
        "ADBE Time Remapping" => "Time Remap",
        "ADBE Marker" => "Marker",
        "ADBE Anchor Point" | "ADBE Vector Anchor" => "Anchor Point",
        "ADBE Position" | "ADBE Vector Position" => "Position",
        "ADBE Position_0" => "X Position",
        "ADBE Position_1" => "Y Position",
        "ADBE Position_2" => "Z Position",
        "ADBE Scale" | "ADBE Vector Scale" => "Scale",
        "ADBE Rotate Z" | "ADBE Vector Rotation" => "Rotation",
        "ADBE Rotate X" => "X Rotation",
        "ADBE Rotate Y" => "Y Rotation",
        "ADBE Orientation" => "Orientation",
        "ADBE Opacity"
        | "ADBE Vector Group Opacity"
        | "ADBE Vector Fill Opacity"
        | "ADBE Vector Stroke Opacity" => "Opacity",
        "ADBE Vector Skew" => "Skew",
        "ADBE Vector Skew Axis" => "Skew Axis",
        "ADBE Vector Fill Color" | "ADBE Vector Stroke Color" => "Color",
        "ADBE Vector Stroke Width" => "Stroke Width",
        "ADBE Vector Rect Size" | "ADBE Vector Ellipse Size" => "Size",
        "ADBE Vector Rect Position"
        | "ADBE Vector Ellipse Position"
        | "ADBE Vector Star Position" => "Position",
        "ADBE Vector Rect Roundness" => "Roundness",
        "ADBE Vector Star Type" => "Type",
        "ADBE Vector Star Points" => "Points",
        "ADBE Vector Star Rotation" => "Rotation",
        "ADBE Vector Star Inner Radius" => "Inner Radius",
        "ADBE Vector Star Outer Radius" => "Outer Radius",
        "ADBE Vector Star Inner Roundess" => "Inner Roundness",
        "ADBE Vector Star Outer Roundess" => "Outer Roundness",
        "ADBE Vector Shape" => "Path",
        "ADBE Vector Trim Start" => "Start",
        "ADBE Vector Trim End" => "End",
        "ADBE Vector Trim Offset" | "ADBE Vector Repeater Offset" => "Offset",
        "ADBE Vector Offset Amount" => "Amount",
        "ADBE Vector Offset Line Join" => "Line Join",
        "ADBE Vector Offset Miter Limit" => "Miter Limit",
        "ADBE Vector RoundCorner Radius" => "Radius",
        "ADBE Vector Repeater Copies" => "Copies",
        "ADBE Vector Repeater Order" => "Composite",
        "ADBE Vector Repeater Transform" => "Transform",
        "ADBE Vector Repeater Anchor" => "Anchor Point",
        "ADBE Vector Repeater Position" => "Position",
        "ADBE Vector Repeater Scale" => "Scale",
        "ADBE Vector Repeater Rotation" => "Rotation",
        "ADBE Vector Repeater Opacity 1" => "Start Opacity",
        "ADBE Vector Repeater Opacity 2" => "End Opacity",
        "ADBE Mask Shape" => "Mask Path",
        "ADBE Mask Feather" => "Mask Feather",
        "ADBE Mask Opacity" => "Mask Opacity",
        "ADBE Mask Offset" => "Mask Expansion",
        _ => return None,
    })
}

/// The match names an attribute of the language stands for.
fn attribute_match_names(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "transform" => &[
            "ADBE Transform Group",
            "ADBE Vector Transform Group",
            "ADBE Vector Repeater Transform",
        ],
        "effect" => &["ADBE Effect Parade"],
        "mask" => &["ADBE Mask Parade"],
        "content" => &["ADBE Root Vectors Group", "ADBE Vectors Group"],
        "text" => &["ADBE Text Properties"],
        "sourceText" => &["ADBE Text Document"],
        "timeRemap" => &["ADBE Time Remapping"],
        "marker" => &["ADBE Marker"],
        "anchorPoint" => &[
            "ADBE Anchor Point",
            "ADBE Vector Anchor",
            "ADBE Vector Repeater Anchor",
        ],
        "position" => &[
            "ADBE Position",
            "ADBE Vector Position",
            "ADBE Vector Rect Position",
            "ADBE Vector Ellipse Position",
            "ADBE Vector Star Position",
            "ADBE Vector Repeater Position",
        ],
        "xPosition" => &["ADBE Position_0"],
        "yPosition" => &["ADBE Position_1"],
        "zPosition" => &["ADBE Position_2"],
        "scale" => &[
            "ADBE Scale",
            "ADBE Vector Scale",
            "ADBE Vector Repeater Scale",
        ],
        "rotation" | "zRotation" => &[
            "ADBE Rotate Z",
            "ADBE Vector Rotation",
            "ADBE Vector Star Rotation",
            "ADBE Vector Repeater Rotation",
        ],
        "xRotation" => &["ADBE Rotate X"],
        "yRotation" => &["ADBE Rotate Y"],
        "orientation" => &["ADBE Orientation"],
        "opacity" => &[
            "ADBE Opacity",
            "ADBE Vector Group Opacity",
            "ADBE Vector Fill Opacity",
            "ADBE Vector Stroke Opacity",
        ],
        "skew" => &["ADBE Vector Skew"],
        "skewAxis" => &["ADBE Vector Skew Axis"],
        "color" => &["ADBE Vector Fill Color", "ADBE Vector Stroke Color"],
        "strokeWidth" => &["ADBE Vector Stroke Width"],
        "size" => &["ADBE Vector Rect Size", "ADBE Vector Ellipse Size"],
        "roundness" => &["ADBE Vector Rect Roundness"],
        "type" => &["ADBE Vector Star Type"],
        "points" => &["ADBE Vector Star Points"],
        "innerRadius" => &["ADBE Vector Star Inner Radius"],
        "outerRadius" => &["ADBE Vector Star Outer Radius"],
        "innerRoundness" => &["ADBE Vector Star Inner Roundess"],
        "outerRoundness" => &["ADBE Vector Star Outer Roundess"],
        "path" => &["ADBE Vector Shape"],
        "start" => &["ADBE Vector Trim Start"],
        "end" => &["ADBE Vector Trim End"],
        "offset" => &["ADBE Vector Trim Offset", "ADBE Vector Repeater Offset"],
        "amount" => &["ADBE Vector Offset Amount"],
        "lineJoin" => &["ADBE Vector Offset Line Join"],
        "miterLimit" => &["ADBE Vector Offset Miter Limit"],
        "radius" => &["ADBE Vector RoundCorner Radius"],
        "copies" => &["ADBE Vector Repeater Copies"],
        "startOpacity" => &["ADBE Vector Repeater Opacity 1"],
        "endOpacity" => &["ADBE Vector Repeater Opacity 2"],
        "maskPath" => &["ADBE Mask Shape"],
        "maskFeather" => &["ADBE Mask Feather"],
        "maskOpacity" => &["ADBE Mask Opacity"],
        "maskExpansion" => &["ADBE Mask Offset"],
        _ => return None,
    })
}

impl Host for ProjectHost {
    fn project(&self) -> NodeId {
        PROJECT
    }

    fn kind(&self, node: NodeId) -> NodeKind {
        match self.entries.get(node as usize).map(|e| &e.node) {
            Some(Node::Project) | None => NodeKind::Project,
            Some(Node::Comp(_)) => NodeKind::Comp,
            Some(Node::Layer(_)) => NodeKind::Layer,
            Some(Node::Group) => NodeKind::Group,
            Some(Node::Property(_) | Node::Default(..)) => NodeKind::Property,
        }
    }

    fn up(&self, node: NodeId) -> Option<NodeId> {
        self.entries.get(node as usize)?.up
    }

    fn child(&self, node: NodeId, select: Select<'_>) -> Option<NodeId> {
        let entry = self.entries.get(node as usize)?;
        match select {
            Select::Index(i) => entry
                .children
                .iter()
                .copied()
                .find(|&c| i64::try_from(self.entries[c as usize].index).is_ok_and(|n| n == i)),
            Select::Name(name) => entry.children.iter().copied().find(|&c| {
                let child = &self.entries[c as usize];
                child.name == name || child.match_name == name
            }),
            Select::Attribute(name) => self.attribute_child(node, name),
        }
    }

    fn link(&self, node: NodeId, name: &str) -> Option<NodeId> {
        let Node::Layer(layer) = &self.entries.get(node as usize)?.node else {
            return None;
        };
        let Node::Comp(comp) = &self.entries[self.comp_of(node)? as usize].node else {
            return None;
        };
        match name {
            "parent" => self.layers.get(&(comp.item, layer.parent?)).copied(),
            _ => None,
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "indices and counts far below 2^52"
    )]
    fn attribute(&self, node: NodeId, name: &str) -> Option<Val> {
        let entry = self.entries.get(node as usize)?;
        let number = |n: f64| Some(Val::Number(n));
        match (&entry.node, name) {
            (Node::Project, _) => None,
            (Node::Comp(_) | Node::Layer(_), "name") => Some(Val::Text(entry.name.clone())),
            (Node::Comp(comp), _) => match name {
                "width" => number(comp.width),
                "height" => number(comp.height),
                "duration" => number(comp.duration),
                "frameDuration" => number(1.0 / comp.frame_rate),
                "frameRate" => number(comp.frame_rate),
                "pixelAspect" => number(comp.pixel_aspect),
                "displayStartTime" => number(0.0),
                "numLayers" => number(comp.layers as f64),
                "bgColor" => Some(Val::Vector(vec![
                    comp.background.r,
                    comp.background.g,
                    comp.background.b,
                    comp.background.a,
                ])),
                _ => None,
            },
            (Node::Layer(layer), _) => match name {
                "index" => number(entry.index as f64),
                "inPoint" => number(layer.in_point),
                "outPoint" => number(layer.out_point),
                "startTime" => number(layer.start_time),
                "stretch" => number(layer.stretch * 100.0),
                "width" => number(layer.size[0]),
                "height" => number(layer.size[1]),
                _ => None,
            },
            (_, "name") => Some(Val::Text(entry.name.clone())),
            (_, "propertyIndex") => number(entry.index as f64),
            (_, "matchName") => Some(Val::Text(entry.match_name.clone())),
            (Node::Group, "numProperties") => number(entry.children.len() as f64),
            _ => None,
        }
    }

    fn value_at(&self, node: NodeId, time: f64, own: bool) -> Result<Val, Missing> {
        let entry = self
            .entries
            .get(node as usize)
            .ok_or_else(|| Missing::Unknown("a node the project does not have".into()))?;
        match &entry.node {
            Node::Property(property) => {
                let expression = property.expression.as_ref().is_some_and(|e| e.enabled);
                if !own && expression {
                    match self.baked.borrow().get(&node) {
                        Some(Baked::Values(values)) => {
                            if let Some(value) = values.get(&time.to_bits()) {
                                return Ok(value.clone());
                            }
                        }
                        Some(Baked::Constant(value)) => return Ok(value.clone()),
                        // The bake says why it failed.
                        Some(Baked::Failed(_)) | None => {}
                    }
                    self.demand.set(Some((node, time)));
                    return Err(Missing::Pending(node));
                }
                self.own_value(node, property, time)
            }
            Node::Default(parameter, size) => default_val(parameter, *size),
            _ => Err(Missing::Unknown(
                "the value of something that is not a property".into(),
            )),
        }
    }

    fn constant(&self, node: NodeId, own: bool) -> bool {
        match self.entries.get(node as usize).map(|e| &e.node) {
            Some(Node::Property(property)) => {
                let expression = property.expression.as_ref().is_some_and(|e| e.enabled);
                if !own && expression {
                    matches!(self.baked.borrow().get(&node), Some(Baked::Constant(_)))
                } else if property.separated && property.match_name == "ADBE Position" {
                    self.parts(node)
                        .iter()
                        .all(|&part| self.constant(part, false))
                } else {
                    matches!(property.value, Stream::Static(_))
                }
            }
            Some(Node::Default(..)) => true,
            _ => false,
        }
    }

    fn rect_at(&self, layer: NodeId, time: f64, extents: bool) -> Result<[f64; 4], Missing> {
        let Some(Node::Layer(info)) = self.entries.get(layer as usize).map(|e| &e.node) else {
            return Err(Missing::Unknown(
                "the content of something that is not a layer".into(),
            ));
        };
        // A layer with a source covers its source, whatever its time.
        if let Some([width, height]) = info.source {
            return Ok([0.0, 0.0, width, height]);
        }
        if !matches!(info.kind, LayerKind::Text | LayerKind::Shape) {
            return Err(Missing::Unknown(format!(
                "the content of a {} layer, which draws none",
                match info.kind {
                    LayerKind::Camera => "camera",
                    LayerKind::Light => "light",
                    _ => "sourceless",
                }
            )));
        }
        let ask = Ask {
            comp: info.item,
            layer: info.id,
            time,
            extents,
        };
        match self.layouts.get(&ask) {
            Some(Ok(rect)) => Ok(*rect),
            Some(Err(why)) => Err(Missing::Unknown(format!(
                "the size of a layer's content, which could not be measured ({why})"
            ))),
            None => {
                if self.asked.borrow_mut().insert(ask.key()) {
                    self.asks.borrow_mut().push(ask);
                }
                Err(Missing::Unmeasured)
            }
        }
    }

    fn key_count(&self, node: NodeId) -> usize {
        match self.entries.get(node as usize).map(|e| &e.node) {
            Some(Node::Property(property)) => match &property.value {
                Stream::Animated(keys) => keys.len(),
                Stream::Static(_) => 0,
            },
            _ => 0,
        }
    }

    fn key(&self, node: NodeId, n: usize) -> Option<(f64, Val)> {
        let Some(Node::Property(property)) = self.entries.get(node as usize).map(|e| &e.node)
        else {
            return None;
        };
        let Stream::Animated(keys) = &property.value else {
            return None;
        };
        let key = keys.get(n.checked_sub(1)?)?;
        Some((key.time, self.read(node, &key.value).ok()?))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bake/host.rs"]
mod tests;
