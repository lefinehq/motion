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
//! - **Other expressions.** A property with an expression reads as what its
//!   expression gave, once baked; until then it is [`Missing::Pending`], and
//!   the time asked for is kept for the bake to evaluate first.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use motion_expr::{Host, Missing, NodeId, NodeKind, Select, Val};

use crate::model::{
    Child, Color, Composition, Control, Group, Item, ItemKind, Layer, Parameter, Project, Property,
    Stream, Value,
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
    in_point: f64,
    out_point: f64,
    start_time: f64,
    stretch: f64,
    parent: Option<u32>,
    size: [f64; 2],
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
}

/// What a property with an expression reads as, so far.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Baked {
    /// What its expression gave at each time asked, by the time's bits.
    Values(HashMap<u64, Val>),
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
}

/// Where the nodes being added sit: their layer, its comp and the comp's
/// item id, and the layer's size.
#[derive(Debug, Clone, Copy)]
struct At<'p> {
    layer: &'p Layer,
    comp: &'p Composition,
    item: u32,
    size: [f64; 2],
}

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
    /// The host over `project`, and the properties whose expressions are on.
    pub(super) fn new(project: &Project) -> (ProjectHost, Vec<Target<'_>>) {
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
                }],
                layers: HashMap::new(),
                baked: RefCell::new(HashMap::new()),
                demand: Cell::new(None),
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
                let size = layer_size(project, layer, comp);
                let info = LayerInfo {
                    in_point: layer.in_point,
                    out_point: layer.out_point,
                    start_time: layer.start_time,
                    stretch: layer.stretch,
                    parent: layer.parent,
                    size,
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
                        self.property(up, p, name, index, at);
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
                    self.children(id, g, effect, at);
                }
                Child::Property(p) => {
                    let name = p.name.clone().unwrap_or_else(|| {
                        display_name(&p.match_name)
                            .unwrap_or(&p.match_name)
                            .to_owned()
                    });
                    self.property(up, p, name, index, at);
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
    ) {
        let node = Node::Property(Box::new(property.clone()));
        let id = self.host.add(up, node, &property.match_name, name, index);
        if property.expression.as_ref().is_some_and(|e| e.enabled) {
            self.targets.push(Target {
                node: id,
                property,
                layer: at.layer,
                comp: at.comp,
                item: at.item,
            });
        }
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

/// A layer's size: its source's, else its comp's.
fn layer_size(project: &Project, layer: &Layer, comp: &Composition) -> [f64; 2] {
    let source = layer
        .source
        .and_then(|id| project.items.iter().find(|item: &&Item| item.id == id));
    match source.map(|item| &item.kind) {
        Some(ItemKind::Composition(c)) => [f64::from(c.width), f64::from(c.height)],
        Some(ItemKind::Footage(f)) => [f64::from(f.width), f64::from(f.height)],
        _ => [f64::from(comp.width), f64::from(comp.height)],
    }
}

impl ProjectHost {
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
        to_val(&property.value.value_at(time), unit(&property.match_name))
    }

    /// A separated position: its `_0`, `_1` and `_2` siblings together.
    fn separated(&self, node: NodeId, time: f64) -> Result<Val, Missing> {
        let up = self.entries[node as usize].up.unwrap_or(PROJECT);
        let mut parts = Vec::new();
        for suffix in ["_0", "_1", "_2"] {
            let wanted = format!("ADBE Position{suffix}");
            let Some(&sibling) = self.entries[up as usize]
                .children
                .iter()
                .find(|&&c| self.entries[c as usize].match_name == wanted)
            else {
                continue;
            };
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

/// How many of an expression's units make one of `property`'s.
pub(super) fn unit_of(property: &Property) -> f64 {
    unit(&property.match_name)
}

/// A project value as an expression reads it, scaled by `unit`.
fn to_val(value: &Value, unit: f64) -> Result<Val, Missing> {
    match value {
        Value::Number(n) => Ok(Val::Number(n * unit)),
        Value::Text(document) => Ok(Val::Text(document.text.clone())),
        Value::Path(_) => Err(Missing::Unknown("a path's shape".into())),
        Value::Opaque { reason } => Err(Missing::Unknown(format!(
            "a value the reader does not decode ({reason})"
        ))),
        other => Ok(Val::Vector(
            other
                .numbers()
                .unwrap_or_default()
                .iter()
                .map(|n| n * unit)
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
        "ADBE Vector Rect Position" | "ADBE Vector Ellipse Position" => "Position",
        "ADBE Vector Rect Roundness" => "Roundness",
        "ADBE Vector Shape" => "Path",
        "ADBE Vector Trim Start" => "Start",
        "ADBE Vector Trim End" => "End",
        "ADBE Vector Trim Offset" => "Offset",
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
        "transform" => &["ADBE Transform Group", "ADBE Vector Transform Group"],
        "effect" => &["ADBE Effect Parade"],
        "mask" => &["ADBE Mask Parade"],
        "content" => &["ADBE Root Vectors Group", "ADBE Vectors Group"],
        "text" => &["ADBE Text Properties"],
        "sourceText" => &["ADBE Text Document"],
        "timeRemap" => &["ADBE Time Remapping"],
        "marker" => &["ADBE Marker"],
        "anchorPoint" => &["ADBE Anchor Point", "ADBE Vector Anchor"],
        "position" => &[
            "ADBE Position",
            "ADBE Vector Position",
            "ADBE Vector Rect Position",
            "ADBE Vector Ellipse Position",
        ],
        "xPosition" => &["ADBE Position_0"],
        "yPosition" => &["ADBE Position_1"],
        "zPosition" => &["ADBE Position_2"],
        "scale" => &["ADBE Scale", "ADBE Vector Scale"],
        "rotation" | "zRotation" => &["ADBE Rotate Z", "ADBE Vector Rotation"],
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
        "path" => &["ADBE Vector Shape"],
        "start" => &["ADBE Vector Trim Start"],
        "end" => &["ADBE Vector Trim End"],
        "offset" => &["ADBE Vector Trim Offset"],
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
        Some((
            key.time,
            to_val(&key.value, unit(&property.match_name)).ok()?,
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bake/host.rs"]
mod tests;
