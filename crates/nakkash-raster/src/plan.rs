//! Effects as the steps they run, for an accelerator to run the same ones
//! ([D74]).
//!
//! A plan is what an effect's parameters come to on one layer's pixels:
//! lengths already in pixels of the grid, choices already made. The CPU
//! functions of these effects run their own plan, so an accelerator that
//! runs a plan reads the parameters exactly as the reference does, and
//! never reads them itself. Plans are a table by effect name, [`PLANS`],
//! beside the registry rather than in it: an effect gains a plan by a row
//! here, and a row in the registry never has to change for it.
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

use vello_cpu::kurbo::Vec2;

use crate::blur::{self, Edges, FindEdges, Sharpen, gaussian_radius};
use crate::channel::{self, SetMatte, ShiftChannels};
use crate::colour::{
    self, BlackWhite, Curves, Exposure, Fill, HueSaturation, Invert, Levels, Tint,
};
use crate::composite::{self, Operation, SolidComposite};
use crate::cycle::{self, ColorCycle};
use crate::distort::{self, Distortion};
use crate::effect::{Effect, ParamValue, Space};

/// The steps an effect runs on its layer's pixels. More variants come as
/// more effects gain plans, so a match on one needs a catch-all arm.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Plan {
    /// A blur: box passes along rows, then along columns.
    Blur(Blur),
    /// The bright parts blurred and combined with the layer.
    Glow(Glow),
    /// The layer's alpha, coloured, moved and blurred, behind the layer.
    Shadow(Shadow),
    /// The layer laid by an operation over a solid in its box.
    SolidComposite(SolidComposite),
    /// Each colour level through a curve's table.
    Curves(Box<Curves>),
    /// The layer's alpha from a level of a matte.
    SetMatte(SetMatte),
    /// Each colour channel's slope across its 3×3 pixels.
    FindEdges(FindEdges),
    /// Each colour's luma mapped between two colours.
    Tint(Tint),
    /// The content recoloured with one colour, or the rest of its box.
    Fill(Fill),
    /// Each channel a level of the colour as it was.
    ShiftChannels(ShiftChannels),
    /// Each pixel moved away from the mean of its neighbours.
    Sharpen(Sharpen),
    /// Channels turned into 1 minus themselves.
    Invert(Invert),
    /// Levels mapped between two ranges through a gamma.
    Levels(Levels),
    /// Hues turned, saturation and lightness moved, or colourised.
    HueSaturation(HueSaturation),
    /// Light scaled, offset and raised to a gamma.
    Exposure(Exposure),
    /// Colours turned grey by their hues' shares, and tinted.
    BlackWhite(BlackWhite),
    /// A phase read off each pixel sent round a cycle of colours.
    ColorCycle(Box<ColorCycle>),
    /// Every pixel redrawn from a point of the layer the distortion moves
    /// it to.
    Distort(Distortion),
}

/// Passes of one extended box along one way of the grid: each averages
/// the `2⌊radius⌋ + 1` pixels around a pixel, and the two beyond them by
/// what `radius` has over `⌊radius⌋`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Passes {
    /// The box's radius, in pixels of the grid.
    pub radius: f32,
    /// How many times the box runs.
    pub count: usize,
}

impl Passes {
    /// No pass at all.
    pub const NONE: Passes = Passes {
        radius: 0.0,
        count: 0,
    };

    /// Whether running them changes anything.
    pub fn any(self) -> bool {
        self.count > 0 && self.radius > 0.0
    }
}

/// A blur made of box passes: along every row, then along every column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blur {
    /// The passes along each row.
    pub rows: Passes,
    /// The passes along each column, run after the rows'.
    pub columns: Passes,
    /// What lies beyond the grid's edges.
    pub edges: Edges,
}

impl Blur {
    /// Three passes each way that approach a Gaussian of standard
    /// deviation `sigma` px along rows and along columns; none along a way
    /// whose `sigma` is not above 0.
    pub fn gaussian(sigma: [f32; 2], edges: Edges) -> Blur {
        let passes = |sigma: f32| match sigma > 0.0 {
            true => Passes {
                radius: gaussian_radius(sigma),
                count: 3,
            },
            false => Passes::NONE,
        };
        Blur {
            rows: passes(sigma[0]),
            columns: passes(sigma[1]),
            edges,
        }
    }
}

/// The colours brighter than a threshold, blurred and combined with the
/// layer by an operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glow {
    /// The luma, of the unpremultiplied colour by `luma`'s weights, below
    /// which a pixel gives no light.
    pub threshold: f32,
    /// The weights of red, green and blue in the luma: Rec. 601's.
    pub luma: [f32; 3],
    /// How the light spreads, beyond the edges transparent.
    pub blur: Blur,
    /// What the spread light is multiplied by.
    pub intensity: f32,
    /// Where the layer goes against its light.
    pub composite: GlowComposite,
    /// How the light and the layer combine, when both are drawn.
    pub operation: Operation,
}

/// Where a [`Glow`] puts its layer against its light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowComposite {
    /// The light laid over the layer by the operation.
    WithOriginal,
    /// The light alone, in place of the layer.
    GlowOnly,
    /// The layer laid over the light by the operation.
    BehindOriginal,
}

/// The layer's alpha as a shadow behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// The shadow's colour, unpremultiplied red, green and blue.
    pub color: [f32; 3],
    /// What the layer's alpha is multiplied by.
    pub opacity: f32,
    /// How far the shadow moves, in pixels of the grid.
    pub offset: Vec2,
    /// How the shadow softens.
    pub blur: Blur,
    /// Whether the shadow replaces the layer, rather than lying behind it.
    pub alone: bool,
}

/// One row of [`PLANS`]: an effect's name and how its parameter values on
/// a layer's pixels become its plan.
#[derive(Debug, Clone, Copy)]
pub struct PlanSpec {
    /// The effect's name in the registry.
    pub name: &'static str,
    /// Its plan from its values, in the order of its parameters, and the
    /// space of the pixels it runs on.
    pub plan: fn(&[ParamValue], &Space) -> Plan,
}

/// Every effect that has a plan.
pub const PLANS: &[PlanSpec] = &[
    PlanSpec {
        name: "gaussian-blur",
        plan: |values, space| Plan::Blur(blur::gaussian_blur_plan(values, space)),
    },
    PlanSpec {
        name: "box-blur",
        plan: |values, space| Plan::Blur(blur::box_blur_plan(values, space)),
    },
    PlanSpec {
        name: "glow",
        plan: |values, space| Plan::Glow(blur::glow_plan(values, space)),
    },
    PlanSpec {
        name: "drop-shadow",
        plan: |values, space| Plan::Shadow(blur::drop_shadow_plan(values, space)),
    },
    PlanSpec {
        name: "solid-composite",
        plan: |values, space| Plan::SolidComposite(composite::solid_composite_plan(values, space)),
    },
    PlanSpec {
        name: "curves",
        plan: |values, space| Plan::Curves(Box::new(colour::curves_plan(values, space))),
    },
    PlanSpec {
        name: "set-matte",
        plan: |values, space| Plan::SetMatte(channel::set_matte_plan(values, space)),
    },
    PlanSpec {
        name: "find-edges",
        plan: |values, space| Plan::FindEdges(blur::find_edges_plan(values, space)),
    },
    PlanSpec {
        name: "tint",
        plan: |values, space| Plan::Tint(colour::tint_plan(values, space)),
    },
    PlanSpec {
        name: "fill",
        plan: |values, space| Plan::Fill(colour::fill_plan(values, space)),
    },
    PlanSpec {
        name: "shift-channels",
        plan: |values, space| Plan::ShiftChannels(channel::shift_channels_plan(values, space)),
    },
    PlanSpec {
        name: "sharpen",
        plan: |values, space| Plan::Sharpen(blur::sharpen_plan(values, space)),
    },
    PlanSpec {
        name: "invert",
        plan: |values, space| Plan::Invert(colour::invert_plan(values, space)),
    },
    PlanSpec {
        name: "levels",
        plan: |values, space| Plan::Levels(colour::levels_plan(values, space)),
    },
    PlanSpec {
        name: "hue-saturation",
        plan: |values, space| Plan::HueSaturation(colour::hue_saturation_plan(values, space)),
    },
    PlanSpec {
        name: "exposure",
        plan: |values, space| Plan::Exposure(colour::exposure_plan(values, space)),
    },
    PlanSpec {
        name: "black-white",
        plan: |values, space| Plan::BlackWhite(colour::black_white_plan(values, space)),
    },
    PlanSpec {
        name: "color-cycle",
        plan: |values, space| Plan::ColorCycle(Box::new(cycle::color_cycle_plan(values, space))),
    },
    PlanSpec {
        name: "displacement-map",
        plan: |values, _| {
            Plan::Distort(Distortion::DisplacementMap(distort::displacement_map_plan(
                values,
            )))
        },
    },
    PlanSpec {
        name: "turbulent-displace",
        plan: |values, _| {
            Plan::Distort(Distortion::TurbulentDisplace(
                distort::turbulent_displace_plan(values),
            ))
        },
    },
    PlanSpec {
        name: "polar-coordinates",
        plan: |values, _| {
            Plan::Distort(Distortion::PolarCoordinates(
                distort::polar_coordinates_plan(values),
            ))
        },
    },
    PlanSpec {
        name: "lens-distortion",
        plan: |values, _| {
            Plan::Distort(Distortion::LensDistortion(distort::lens_distortion_plan(
                values,
            )))
        },
    },
    PlanSpec {
        name: "repeat-tile",
        plan: |values, _| Plan::Distort(Distortion::RepeatTile(distort::repeat_tile_plan(values))),
    },
    PlanSpec {
        name: "motion-tile",
        plan: |values, _| Plan::Distort(Distortion::MotionTile(distort::motion_tile_plan(values))),
    },
    PlanSpec {
        name: "wrap",
        plan: |values, _| Plan::Distort(Distortion::Wrap(distort::wrap_plan(values))),
    },
    PlanSpec {
        name: "mirror",
        plan: |values, _| Plan::Distort(Distortion::Mirror(distort::mirror_plan(values))),
    },
    PlanSpec {
        name: "bezier-warp",
        plan: |values, _| Plan::Distort(Distortion::BezierWarp(distort::bezier_warp_plan(values))),
    },
];

impl Effect {
    /// The steps it runs on pixels in `space`, when it has a row in
    /// [`PLANS`].
    pub fn plan(&self, space: &Space) -> Option<Plan> {
        let name = self.spec().name;
        PLANS
            .iter()
            .find(|row| row.name == name)
            .map(|row| (row.plan)(self.values(), space))
    }

    /// Whether it has a row in [`PLANS`].
    pub fn has_plan(&self) -> bool {
        let name = self.spec().name;
        PLANS.iter().any(|row| row.name == name)
    }
}

#[cfg(test)]
#[path = "../tests/unit/plan.rs"]
mod tests;
