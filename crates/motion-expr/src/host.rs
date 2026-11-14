//! [`Host`]: what an expression may read, given by the importer.

/// A comp, layer, property group or property the host knows, by a number
/// it chooses.
pub type NodeId = u32;

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// The project: comps are its children.
    Project,
    /// A composition: layers are its children, from 1 at the top.
    Comp,
    /// A layer: its property groups are its children.
    Layer,
    /// A property group, such as an effect or the transform.
    Group,
    /// A property with a value.
    Property,
}

/// How an expression names a child.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Select<'a> {
    /// By its index, from 1.
    Index(i64),
    /// By its name or match name: `layer("Title")`, `effect("Slider")`.
    Name(&'a str),
    /// By the attribute the language gives it: `transform`, `position`,
    /// `sourceText`.
    Attribute(&'a str),
}

/// A value an expression reads or gives.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    /// A number.
    Number(f64),
    /// An array of numbers: a position, a colour.
    Vector(Vec<f64>),
    /// Text.
    Text(String),
}

/// Why a value cannot be read now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// The value comes from another expression, not baked yet.
    Pending(NodeId),
    /// The value is something the import cannot know, with why.
    Unknown(String),
}

/// What an expression may read: the importer's view of a project. Every
/// method is a pure read; values that depend on other expressions are
/// [`Missing::Pending`] until those are baked.
pub trait Host {
    /// The project node.
    fn project(&self) -> NodeId;
    /// What `node` is.
    fn kind(&self, node: NodeId) -> NodeKind;
    /// What holds `node`: a property's group, a group's layer, a layer's
    /// comp, a comp's project; none for the project.
    fn up(&self, node: NodeId) -> Option<NodeId>;
    /// A child of `node`.
    fn child(&self, node: NodeId, select: Select<'_>) -> Option<NodeId>;
    /// Another node `node` links to by `name`, such as a layer's `parent`.
    fn link(&self, node: NodeId, name: &str) -> Option<NodeId>;
    /// An attribute of `node`: a comp's `width`, `height`, `duration`,
    /// `frameDuration`, `displayStartTime`, `numLayers` or `name`; a layer's
    /// `index`, `inPoint`, `outPoint`, `startTime`, `width`, `height` or
    /// `name`; a property's `propertyIndex` or `name`.
    fn attribute(&self, node: NodeId, name: &str) -> Option<Val>;
    /// The value of the property `node` at `time`, in its comp's seconds.
    /// With `own`, the value under the property's expression: what `value`
    /// reads in the expression being baked.
    ///
    /// # Errors
    /// [`Missing`] when it comes from an expression not baked yet, or from
    /// something the import cannot know.
    fn value_at(&self, node: NodeId, time: f64, own: bool) -> Result<Val, Missing>;
    /// How many keys the property `node` has.
    fn key_count(&self, node: NodeId) -> usize;
    /// Key `n` of `node`, from 1: its time and value.
    fn key(&self, node: NodeId, n: usize) -> Option<(f64, Val)>;
    /// The rectangle a layer's content covers at `time`, `[left, top, width,
    /// height]` in its own space.
    ///
    /// # Errors
    /// [`Missing::Unknown`] when the host cannot lay the layer out.
    fn rect_at(&self, layer: NodeId, time: f64) -> Result<[f64; 4], Missing> {
        let _ = (layer, time);
        Err(Missing::Unknown("the size of a layer's content".into()))
    }
}
