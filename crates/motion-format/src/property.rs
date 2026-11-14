//! The registry of animatable properties. Names live only in [`PROPERTIES`];
//! code refers to a property by its row, a [`Prop`], and to a property or one
//! of its components by a [`Target`].

use crate::shorthand::{Axis, Position};
use crate::table::{self, Named};
use crate::value::{Value, ValueType};

/// Where a property rests when nothing animates it and the layer sets no value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rest {
    /// A fixed value.
    Value(Value),
    /// The point the layer's `at` names on the canvas.
    At,
    /// The centre of the layer's box, in the layer's own coordinates.
    BoxCentre,
}

/// One animatable property of a layer.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// The allowed values of each component, inclusive, or `None` when unbounded.
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
    /// The resting value on a layer at `at`, on a canvas of `canvas` pixels,
    /// whose box is `size` pixels.
    pub fn rest(&self, at: &Position, canvas: (f64, f64), size: (f64, f64)) -> Value {
        match self.rest {
            Rest::Value(value) => value,
            Rest::At => Value::Point([at.resolve(Axis::X, canvas), at.resolve(Axis::Y, canvas)]),
            Rest::BoxCentre => Value::Point([size.0 / 2.0, size.1 / 2.0]),
        }
    }
}

impl Named for PropertySpec {
    fn name(&self) -> &str {
        self.name
    }
}

/// Every property, in registry order.
pub const PROPERTIES: &[PropertySpec] = &[
    PropertySpec {
        name: "position",
        ty: ValueType::Point,
        rest: Rest::At,
        components: &["x", "y"],
        range: None,
        broadcast: false,
        spatial: true,
        doc: "where the anchor sits in the parent, in px; the resting value comes from `at`",
    },
    PropertySpec {
        name: "anchor",
        ty: ValueType::Point,
        rest: Rest::BoxCentre,
        components: &["anchor-x", "anchor-y"],
        range: None,
        broadcast: false,
        spatial: true,
        doc: "the point of the layer that sits at `position` and that it scales and turns about, \
              in px of its own box; the box centre by default",
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
        doc: "how opaque the layer is, 0 (invisible) to 1 (solid)",
    },
];

/// A property: the index of its row in [`PROPERTIES`], so every `Prop` names a row.
///
/// The constants are the properties whose meaning code depends on, such as
/// geometry; the registry stays the source of truth for which exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Prop(usize);

impl Prop {
    /// Where the anchor sits in the parent.
    pub const POSITION: Prop = Prop(0);
    /// The point the layer turns and scales about.
    pub const ANCHOR: Prop = Prop(1);
    /// Size factor on each axis.
    pub const SCALE: Prop = Prop(2);
    /// Rotation in degrees, clockwise.
    pub const ROTATE: Prop = Prop(3);
    /// Opacity, 0 (invisible) to 1 (solid).
    pub const OPACITY: Prop = Prop(4);

    /// The property called `name`, if the registry has one.
    pub fn named(name: &str) -> Option<Prop> {
        table::position(PROPERTIES, name).map(Prop)
    }

    /// Every property, in registry order.
    pub fn all() -> impl Iterator<Item = Prop> {
        (0..PROPERTIES.len()).map(Prop)
    }

    /// The property's position in [`PROPERTIES`], and in every list kept parallel to it.
    pub fn index(self) -> usize {
        self.0
    }

    /// The property's row.
    pub fn spec(self) -> &'static PropertySpec {
        &PROPERTIES[self.0]
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

    /// The property or component called `name`, if the registry has one.
    pub fn named(name: &str) -> Option<Target> {
        Prop::named(name).map(Target::whole).or_else(|| {
            Prop::all().find_map(|prop| {
                let at = prop.spec().components.iter().position(|c| *c == name)?;
                Some(Target {
                    prop,
                    component: Some(at),
                })
            })
        })
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
}

/// Every name a track may animate: properties, then their components, in registry order.
pub fn target_names() -> impl Iterator<Item = &'static str> {
    table::names(PROPERTIES).chain(
        PROPERTIES
            .iter()
            .flat_map(|spec| spec.components.iter().copied()),
    )
}

/// The property names in registry order.
pub fn property_names() -> impl Iterator<Item = &'static str> + Clone {
    PROPERTIES.iter().map(|spec| spec.name)
}

#[cfg(test)]
#[path = "../tests/unit/property.rs"]
mod tests;
