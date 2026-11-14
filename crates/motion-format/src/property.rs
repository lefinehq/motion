//! Registries of animatable properties: each kind of object (a layer, a shape
//! group, a rectangle, a fill, an effect) has a [`Registry`] that lists its
//! properties. Names live only in the registries; code refers to a property by
//! its row, a [`Prop`], and to a property or one of its components by a [`Target`].

use kurbo::Rect;

use crate::shorthand::{Axis, Position};
use crate::table::{self, Named};
use crate::value::{Value, ValueType};

/// Where a property rests when nothing animates it and the object sets no value.
#[derive(Debug, Clone, PartialEq)]
pub enum Rest {
    /// A fixed value.
    Value(Value),
    /// The point the layer's `at` names on the canvas.
    At,
    /// The centre of the layer's box, in the layer's own coordinates (a text
    /// layer's box may start left of or above its origin).
    BoxCentre,
}

/// One animatable property of an object.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertySpec {
    /// The name used in scene files.
    pub name: &'static str,
    /// What sort of value it holds.
    pub ty: ValueType,
    /// Where it rests when nothing says otherwise.
    pub rest: Rest,
    /// The names of its components, which tracks can animate one at a time;
    /// empty for a number.
    pub components: &'static [&'static str],
    /// The allowed values of each component, inclusive, or `None` when
    /// unbounded; an infinite end leaves that side open.
    pub range: Option<(f64, f64)>,
    /// A lone number sets every component (a uniform `scale`).
    pub broadcast: bool,
    /// The value moves along a path in space: one curve times the whole move,
    /// and keys may bend the path with tangents.
    pub spatial: bool,
    /// What the property means, for people and agents.
    pub doc: &'static str,
}

impl PropertySpec {
    /// The resting value on a layer at `at`, which resolves in `extent` (its
    /// parent's box, or the canvas), whose own box is `own`.
    pub fn rest(&self, at: &Position, extent: Rect, own: Rect) -> Value {
        match &self.rest {
            Rest::Value(value) => value.clone(),
            Rest::At => {
                let size = (extent.width(), extent.height());
                Value::Point([
                    extent.x0 + at.resolve(Axis::X, size),
                    extent.y0 + at.resolve(Axis::Y, size),
                ])
            }
            Rest::BoxCentre => Value::Point([own.center().x, own.center().y]),
        }
    }
}

impl Named for PropertySpec {
    fn name(&self) -> &str {
        self.name
    }
}

/// The properties of one kind of object, in order.
#[derive(Debug)]
pub struct Registry {
    /// What the object is, for messages: `layer`, `rect`, `fill`.
    pub kind: &'static str,
    /// Its properties.
    pub rows: &'static [PropertySpec],
}

impl Registry {
    /// The property called `name`, if the registry has one.
    pub fn prop(&'static self, name: &str) -> Option<Prop> {
        table::position(self.rows, name).map(|index| Prop {
            registry: self,
            index,
        })
    }

    /// Every property, in order.
    pub fn props(&'static self) -> impl Iterator<Item = Prop> + Clone {
        (0..self.rows.len()).map(move |index| Prop {
            registry: self,
            index,
        })
    }

    /// The property or component called `name`, if the registry has one.
    pub fn target(&'static self, name: &str) -> Option<Target> {
        self.prop(name).map(Target::whole).or_else(|| {
            self.props().find_map(|prop| {
                let at = prop.spec().components.iter().position(|c| *c == name)?;
                Some(Target {
                    prop,
                    component: Some(at),
                })
            })
        })
    }

    /// The property names, in order.
    pub fn names(&'static self) -> impl Iterator<Item = &'static str> + Clone {
        self.rows.iter().map(|spec| spec.name)
    }

    /// Every name a track may animate: properties, then their components.
    pub fn target_names(&'static self) -> impl Iterator<Item = &'static str> + Clone {
        self.names().chain(
            self.rows
                .iter()
                .flat_map(|spec| spec.components.iter().copied()),
        )
    }
}

/// The transform every layer and shape group has, as a row set: position,
/// anchor, scale, rotation and opacity, with where position and anchor rest.
const fn transform(position: Rest, anchor: Rest, anchor_doc: &'static str) -> [PropertySpec; 5] {
    [
        PropertySpec {
            name: "position",
            ty: ValueType::Point,
            rest: position,
            components: &["x", "y"],
            range: None,
            broadcast: false,
            spatial: true,
            doc: "where the anchor sits in the parent, in px",
        },
        PropertySpec {
            name: "anchor",
            ty: ValueType::Point,
            rest: anchor,
            components: &["anchor-x", "anchor-y"],
            range: None,
            broadcast: false,
            spatial: true,
            doc: anchor_doc,
        },
        PropertySpec {
            name: "scale",
            ty: ValueType::Point,
            rest: Rest::Value(Value::Point([1.0, 1.0])),
            components: &["scale-x", "scale-y"],
            range: None,
            broadcast: true,
            spatial: false,
            doc: "size factor on each axis; one number scales both; negative values mirror",
        },
        PropertySpec {
            name: "rotate",
            ty: ValueType::Number,
            rest: Rest::Value(Value::Number(0.0)),
            components: &[],
            range: None,
            broadcast: false,
            spatial: false,
            doc: "rotation in degrees, clockwise",
        },
        PropertySpec {
            name: "opacity",
            ty: ValueType::Number,
            rest: Rest::Value(Value::Number(1.0)),
            components: &[],
            range: Some((0.0, 1.0)),
            broadcast: false,
            spatial: false,
            doc: "how opaque it is, 0 (invisible) to 1 (solid)",
        },
    ]
}

const LAYER_ROWS: [PropertySpec; 5] = transform(
    Rest::At,
    Rest::BoxCentre,
    "the point of the layer that sits at `position` and that it scales and turns about, \
     in px of its own box; the box centre by default",
);

const GROUP_ROWS: [PropertySpec; 5] = transform(
    Rest::Value(Value::Point([0.0, 0.0])),
    Rest::Value(Value::Point([0.0, 0.0])),
    "the point of the group that sits at `position` and that it scales and turns about, \
     in px of the shape's coordinates",
);

/// The properties of a layer: its transform and opacity. `position` rests
/// where `at` puts it.
pub static LAYER: Registry = Registry {
    kind: "layer",
    rows: &LAYER_ROWS,
};

/// The properties of a shape group: the same transform, resting at the origin.
pub static GROUP: Registry = Registry {
    kind: "group",
    rows: &GROUP_ROWS,
};

/// Every layer property, in registry order.
pub const PROPERTIES: &[PropertySpec] = &LAYER_ROWS;

/// A property: its registry and the index of its row there, so every `Prop`
/// names a row.
///
/// The constants are the layer properties whose meaning code depends on, such
/// as geometry; the registry stays the source of truth for which exist.
#[derive(Debug, Clone, Copy)]
pub struct Prop {
    registry: &'static Registry,
    index: usize,
}

impl PartialEq for Prop {
    fn eq(&self, other: &Prop) -> bool {
        std::ptr::eq(self.registry, other.registry) && self.index == other.index
    }
}

impl Eq for Prop {}

impl std::hash::Hash for Prop {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.registry.kind.hash(state);
        self.index.hash(state);
    }
}

impl Prop {
    /// Where the anchor sits in the parent.
    pub const POSITION: Prop = Prop::layer(0);
    /// The point the layer turns and scales about.
    pub const ANCHOR: Prop = Prop::layer(1);
    /// Size factor on each axis.
    pub const SCALE: Prop = Prop::layer(2);
    /// Rotation in degrees, clockwise.
    pub const ROTATE: Prop = Prop::layer(3);
    /// Opacity, 0 (invisible) to 1 (solid).
    pub const OPACITY: Prop = Prop::layer(4);

    const fn layer(index: usize) -> Prop {
        Prop::of(&LAYER, index)
    }

    /// Row `index` of `registry`, for the constants of other registries.
    pub(crate) const fn of(registry: &'static Registry, index: usize) -> Prop {
        assert!(index < registry.rows.len(), "a row of the registry");
        Prop { registry, index }
    }

    /// The layer property called `name`, if there is one.
    pub fn named(name: &str) -> Option<Prop> {
        LAYER.prop(name)
    }

    /// Every layer property, in registry order.
    pub fn all() -> impl Iterator<Item = Prop> {
        LAYER.props()
    }

    /// The property's position in its registry, and in every list kept parallel to it.
    pub fn index(self) -> usize {
        self.index
    }

    /// The registry it belongs to.
    pub fn registry(self) -> &'static Registry {
        self.registry
    }

    /// The property's row.
    pub fn spec(self) -> &'static PropertySpec {
        &self.registry.rows[self.index]
    }

    /// The property's name, as scene files write it.
    pub fn name(self) -> &'static str {
        self.spec().name
    }
}

/// What a track or a preset animates: a whole property, or one component of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    /// The property.
    pub prop: Prop,
    /// The component, an index into the row's `components`, or `None` for the whole value.
    pub component: Option<usize>,
}

impl Target {
    /// The whole of `prop`.
    pub const fn whole(prop: Prop) -> Target {
        Target {
            prop,
            component: None,
        }
    }

    /// The layer property or component called `name`, if there is one.
    pub fn named(name: &str) -> Option<Target> {
        LAYER.target(name)
    }

    /// The name scene files write: the property's, or its component's.
    pub fn name(self) -> &'static str {
        match self.component {
            None => self.prop.name(),
            Some(i) => self.prop.spec().components[i],
        }
    }

    /// The type of value the target holds: a component is a number.
    pub fn value_type(self) -> ValueType {
        match self.component {
            None => self.prop.spec().ty,
            Some(_) => ValueType::Number,
        }
    }

    /// Whether this target and `other` write the same numbers, so two tracks
    /// on them would fight.
    pub fn overlaps(self, other: Target) -> bool {
        self.prop == other.prop
            && match (self.component, other.component) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }

    /// The value it rests at, out of the property's resting value.
    pub fn rest_of(self, whole: Value) -> Value {
        match self.component {
            None => whole,
            Some(i) => Value::Number(whole.components()[i]),
        }
    }

    /// Whether a lone number sets every component of the target.
    pub fn broadcasts(self) -> bool {
        self.component.is_none() && self.prop.spec().broadcast
    }

    /// Whether the target moves along a path: a spatial property animated whole.
    pub fn moves_in_space(self) -> bool {
        self.component.is_none() && self.prop.spec().spatial
    }
}

/// Every name a layer track may animate: properties, then their components.
pub fn target_names() -> impl Iterator<Item = &'static str> + Clone {
    LAYER.target_names()
}

/// The layer property names in registry order.
pub fn property_names() -> impl Iterator<Item = &'static str> + Clone {
    LAYER.names()
}

#[cfg(test)]
#[path = "../tests/unit/property.rs"]
mod tests;
