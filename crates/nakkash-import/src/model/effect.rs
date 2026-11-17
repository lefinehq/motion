//! Effect definitions: the parameters of each effect type a project uses,
//! with their controls, defaults and bounds
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listpart)).
//!
//! A project stores an effect parameter only when it differs from its
//! default, so the default of an omitted parameter comes from here, not from
//! a table kept by the reader.

use serde::Serialize;

use super::Color;

/// One effect type's definition.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectDefinition {
    /// The effect's display name, such as `Fill`.
    pub name: String,
    /// Its parameters, in the order the effect lists them.
    pub parameters: Vec<Parameter>,
}

impl EffectDefinition {
    /// The parameter whose match name is `match_name`, if the effect has one.
    pub fn parameter(&self, match_name: &str) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.match_name == match_name)
    }
}

/// One parameter of an effect.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parameter {
    /// The name the file matches the parameter by, such as `ADBE Fill-0002`.
    pub match_name: String,
    /// The name the effect shows for it.
    pub name: String,
    /// Whether it is hidden from the panels and the timeline.
    pub invisible: bool,
    /// Whether it can be animated.
    pub animatable: bool,
    /// Its control, with its default.
    pub control: Control,
}

/// An effect parameter's control and its default, in the units an instance
/// of the effect stores its value in.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "control", rename_all = "snake_case")]
pub enum Control {
    /// A layer of the composition.
    Layer,
    /// A number between bounds.
    Slider {
        /// The default.
        default: f64,
        /// The smallest valid value, if it has one.
        min: Option<f64>,
        /// The largest valid value, if it has one.
        max: Option<f64>,
        /// Whether the effect shows the value in percent.
        percent: bool,
    },
    /// An angle in degrees.
    Angle {
        /// The default.
        default: f64,
    },
    /// A checkbox.
    Checkbox {
        /// The default.
        default: bool,
    },
    /// A colour.
    Color {
        /// The default.
        default: Color,
    },
    /// A point in the layer.
    Point {
        /// The default, as fractions of the layer's width and height.
        default: Vec<f64>,
    },
    /// One choice out of a list.
    Popup {
        /// The default, a 1-based choice, as instances store it.
        default: u16,
        /// The choices, in order.
        choices: Vec<String>,
    },
    /// A control whose value the reader does not decode: no data, a group's
    /// start or end, a button, a path or arbitrary data, by its SDK code.
    Other {
        /// The control type's code.
        code: u32,
    },
}
