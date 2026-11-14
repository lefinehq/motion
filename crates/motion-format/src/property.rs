//! The registry of animatable properties. Names live only in [`PROPERTIES`];
//! code refers to a property by its row, a [`Prop`].

use crate::shorthand::{Axis, Position};
use crate::table::{self, Named};

/// One animatable property of a layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropertySpec {
    /// The name used in scene files.
    pub name: &'static str,
    /// The resting value when nothing says otherwise.
    pub default: f64,
    /// The allowed values, inclusive, or `None` when unbounded.
    pub range: Option<(f64, f64)>,
    /// The axis of the layer's `at` that gives the resting value instead of `default`.
    pub rest_from_at: Option<Axis>,
    /// What the property means, for people and agents.
    pub doc: &'static str,
}

impl PropertySpec {
    /// The resting value on a layer at `at`, on a canvas of `canvas` pixels.
    pub fn rest(&self, at: &Position, canvas: (f64, f64)) -> f64 {
        match self.rest_from_at {
            Some(axis) => at.resolve(axis, canvas),
            None => self.default,
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
        name: "x",
        default: 0.0,
        range: None,
        rest_from_at: Some(Axis::X),
        doc: "centre x in px; the resting value comes from `at`",
    },
    PropertySpec {
        name: "y",
        default: 0.0,
        range: None,
        rest_from_at: Some(Axis::Y),
        doc: "centre y in px; the resting value comes from `at`",
    },
    PropertySpec {
        name: "opacity",
        default: 1.0,
        range: Some((0.0, 1.0)),
        rest_from_at: None,
        doc: "how opaque the layer is, 0 (invisible) to 1 (solid)",
    },
    PropertySpec {
        name: "scale",
        default: 1.0,
        range: None,
        rest_from_at: None,
        doc: "uniform size factor; negative values mirror",
    },
    PropertySpec {
        name: "rotate",
        default: 0.0,
        range: None,
        rest_from_at: None,
        doc: "rotation in degrees, clockwise",
    },
];

/// A property: the index of its row in [`PROPERTIES`], so every `Prop` names a row.
///
/// The constants are the properties whose meaning code depends on, such as
/// geometry; the registry stays the source of truth for which exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Prop(usize);

impl Prop {
    /// Centre x in pixels.
    pub const X: Prop = Prop(0);
    /// Centre y in pixels.
    pub const Y: Prop = Prop(1);
    /// Opacity, 0 (invisible) to 1 (solid).
    pub const OPACITY: Prop = Prop(2);
    /// Uniform size factor.
    pub const SCALE: Prop = Prop(3);
    /// Rotation in degrees, clockwise.
    pub const ROTATE: Prop = Prop(4);

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

/// The property names in registry order.
pub fn property_names() -> impl Iterator<Item = &'static str> {
    table::names(PROPERTIES)
}

#[cfg(test)]
#[path = "../tests/unit/property.rs"]
mod tests;
