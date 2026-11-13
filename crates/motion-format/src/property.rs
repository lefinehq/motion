//! The registry of animatable properties.
//!
//! Part of step 1 (Read). Property names live only in [`PROPERTIES`]: presets
//! refer to them by index, and every error that lists names reads this
//! table. Later steps use the defaults and ranges to evaluate and check
//! values.

/// The spellings of the properties whose meaning code depends on.
///
/// Geometry and visibility are computed from these few properties, so the
/// code has to name them. This module is the one place where the spellings
/// live; [`PROPERTIES`] and every other crate refer to them, so there is a
/// single spelling. It is not a list of options: the registry stays the
/// source of truth for which properties exist.
pub mod names {
    /// Centre x in pixels.
    pub const X: &str = "x";
    /// Centre y in pixels.
    pub const Y: &str = "y";
    /// Opacity, 0 (invisible) to 1 (solid).
    pub const OPACITY: &str = "opacity";
    /// Uniform size factor.
    pub const SCALE: &str = "scale";
    /// Rotation in degrees, clockwise.
    pub const ROTATE: &str = "rotate";
}

/// One animatable property of a layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropertySpec {
    /// The name used in scene files.
    pub name: &'static str,
    /// The resting value when nothing says otherwise.
    pub default: f64,
    /// The allowed values, inclusive, or `None` when unbounded.
    pub range: Option<(f64, f64)>,
    /// What the property means, for people and agents.
    pub doc: &'static str,
}

/// Every property, in registry order. A property's index here identifies it
/// everywhere else.
pub const PROPERTIES: &[PropertySpec] = &[
    PropertySpec {
        name: names::X,
        default: 0.0,
        range: None,
        doc: "centre x in px; the resting value comes from `at`",
    },
    PropertySpec {
        name: names::Y,
        default: 0.0,
        range: None,
        doc: "centre y in px; the resting value comes from `at`",
    },
    PropertySpec {
        name: names::OPACITY,
        default: 1.0,
        range: Some((0.0, 1.0)),
        doc: "how opaque the layer is, 0 (invisible) to 1 (solid)",
    },
    PropertySpec {
        name: names::SCALE,
        default: 1.0,
        range: None,
        doc: "uniform size factor; negative values mirror",
    },
    PropertySpec {
        name: names::ROTATE,
        default: 0.0,
        range: None,
        doc: "rotation in degrees, clockwise",
    },
];

/// Looks a property up by name, returning its index in [`PROPERTIES`].
pub fn property(name: &str) -> Option<(usize, &'static PropertySpec)> {
    PROPERTIES.iter().enumerate().find(|(_, p)| p.name == name)
}

/// The property names in registry order, for error messages.
pub fn property_names() -> impl Iterator<Item = &'static str> {
    PROPERTIES.iter().map(|p| p.name)
}

#[cfg(test)]
#[path = "../tests/unit/property.rs"]
mod tests;
