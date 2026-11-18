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

use crate::blur::{self, Edges, gaussian_radius};
use crate::composite::{self, Operation, SolidComposite};
use crate::effect::{Effect, ParamValue, Space};
use crate::generate::{
    self, Beam, FourColor, FractalNoise, GridLines, IrisWipe, LinearWipe, Ramp, VenetianBlinds,
};

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
    /// A gradient between two colours.
    Ramp(Ramp),
    /// Four colours at four points, blended.
    FourColor(FourColor),
    /// A grid of lines, or its cells.
    Grid(GridLines),
    /// The layer hidden in stripes.
    VenetianBlinds(VenetianBlinds),
    /// The layer hidden behind a straight edge.
    LinearWipe(LinearWipe),
    /// A polygon hole in the layer.
    IrisWipe(IrisWipe),
    /// Fractal value noise.
    FractalNoise(FractalNoise),
    /// A straight beam along a line.
    Beam(Beam),
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
        name: "ramp",
        plan: |values, space| Plan::Ramp(generate::ramp_plan(values, space)),
    },
    PlanSpec {
        name: "four-color",
        plan: |values, space| Plan::FourColor(generate::four_color_plan(values, space)),
    },
    PlanSpec {
        name: "grid",
        plan: |values, space| Plan::Grid(generate::grid_plan(values, space)),
    },
    PlanSpec {
        name: "venetian-blinds",
        plan: |values, space| Plan::VenetianBlinds(generate::venetian_blinds_plan(values, space)),
    },
    PlanSpec {
        name: "linear-wipe",
        plan: |values, space| Plan::LinearWipe(generate::linear_wipe_plan(values, space)),
    },
    PlanSpec {
        name: "iris-wipe",
        plan: |values, space| Plan::IrisWipe(generate::iris_wipe_plan(values, space)),
    },
    PlanSpec {
        name: "fractal-noise",
        plan: |values, space| Plan::FractalNoise(generate::fractal_noise_plan(values, space)),
    },
    PlanSpec {
        name: "beam",
        plan: |values, space| Plan::Beam(generate::beam_plan(values, space)),
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
