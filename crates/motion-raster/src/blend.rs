//! Blend modes: how a layer's colours combine with what is below it. The
//! names and their vello equivalents are one table, [`BLEND_MODES`].

use motion_format::{Diagnostic, Result};
use vello_cpu::peniko::{self, Compose, Mix};

/// How a layer combines with the layers below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// The layer covers what is below.
    #[default]
    Normal,
    /// Multiplies colours: never lighter.
    Multiply,
    /// Inverts, multiplies and inverts back: never darker.
    Screen,
    /// Multiply or screen, depending on the colour below.
    Overlay,
    /// The darker of the two colours.
    Darken,
    /// The lighter of the two colours.
    Lighten,
    /// Brightens the colour below to reflect the layer.
    ColorDodge,
    /// Darkens the colour below to reflect the layer.
    ColorBurn,
    /// Multiply or screen, depending on the layer's colour.
    HardLight,
    /// A softer hard light.
    SoftLight,
    /// The absolute difference of the colours.
    Difference,
    /// Like difference, with lower contrast.
    Exclusion,
    /// The layer's hue with the saturation and luminosity below.
    Hue,
    /// The layer's saturation with the hue and luminosity below.
    Saturation,
    /// The layer's hue and saturation with the luminosity below.
    Color,
    /// The layer's luminosity with the hue and saturation below.
    Luminosity,
    /// Adds the colours, clamped: a linear dodge.
    Add,
}

/// One blend mode: its name, and the vello mode that draws it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlendSpec {
    /// The name scenes use.
    pub name: &'static str,
    /// The mode.
    pub mode: BlendMode,
    vello: peniko::BlendMode,
}

const fn spec(name: &'static str, mode: BlendMode, mix: Mix, compose: Compose) -> BlendSpec {
    BlendSpec {
        name,
        mode,
        vello: peniko::BlendMode { mix, compose },
    }
}

/// Every blend mode, in the order tools usually list them.
pub const BLEND_MODES: &[BlendSpec] = &[
    spec("normal", BlendMode::Normal, Mix::Normal, Compose::SrcOver),
    spec(
        "multiply",
        BlendMode::Multiply,
        Mix::Multiply,
        Compose::SrcOver,
    ),
    spec("screen", BlendMode::Screen, Mix::Screen, Compose::SrcOver),
    spec(
        "overlay",
        BlendMode::Overlay,
        Mix::Overlay,
        Compose::SrcOver,
    ),
    spec("darken", BlendMode::Darken, Mix::Darken, Compose::SrcOver),
    spec(
        "lighten",
        BlendMode::Lighten,
        Mix::Lighten,
        Compose::SrcOver,
    ),
    spec(
        "color-dodge",
        BlendMode::ColorDodge,
        Mix::ColorDodge,
        Compose::SrcOver,
    ),
    spec(
        "color-burn",
        BlendMode::ColorBurn,
        Mix::ColorBurn,
        Compose::SrcOver,
    ),
    spec(
        "hard-light",
        BlendMode::HardLight,
        Mix::HardLight,
        Compose::SrcOver,
    ),
    spec(
        "soft-light",
        BlendMode::SoftLight,
        Mix::SoftLight,
        Compose::SrcOver,
    ),
    spec(
        "difference",
        BlendMode::Difference,
        Mix::Difference,
        Compose::SrcOver,
    ),
    spec(
        "exclusion",
        BlendMode::Exclusion,
        Mix::Exclusion,
        Compose::SrcOver,
    ),
    spec("hue", BlendMode::Hue, Mix::Hue, Compose::SrcOver),
    spec(
        "saturation",
        BlendMode::Saturation,
        Mix::Saturation,
        Compose::SrcOver,
    ),
    spec("color", BlendMode::Color, Mix::Color, Compose::SrcOver),
    spec(
        "luminosity",
        BlendMode::Luminosity,
        Mix::Luminosity,
        Compose::SrcOver,
    ),
    // Porter-Duff "plus": the sum of both, which the pipeline clamps.
    spec("add", BlendMode::Add, Mix::Normal, Compose::Plus),
];

impl BlendMode {
    /// The mode called `name` in [`BLEND_MODES`].
    ///
    /// # Errors
    /// An unknown-name diagnostic listing every mode.
    pub fn named(name: &str) -> Result<BlendMode> {
        BLEND_MODES
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.mode)
            .ok_or_else(|| {
                Diagnostic::unknown("blend mode", name, BLEND_MODES.iter().map(|s| s.name))
            })
    }

    /// The mode's row in [`BLEND_MODES`].
    pub fn spec(self) -> &'static BlendSpec {
        BLEND_MODES
            .iter()
            .find(|s| s.mode == self)
            .expect("every blend mode has a row")
    }

    /// The mode's name.
    pub fn name(self) -> &'static str {
        self.spec().name
    }

    pub(crate) fn to_vello(self) -> peniko::BlendMode {
        self.spec().vello
    }
}

#[cfg(test)]
#[path = "../tests/unit/blend.rs"]
mod tests;
