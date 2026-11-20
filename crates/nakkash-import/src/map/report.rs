//! The mapping report: every match name the import met, with how often it
//! was mapped, left at its default, ignored or not supported, each reason
//! not supported with where it was met (fonts set in a substitute among
//! them), the fonts found under another name, and the id every name became.

use std::collections::BTreeMap;
use std::fmt;

use super::items::plural;

use serde::Serialize;

/// What became of one property or group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fate {
    /// It is in the scene.
    Mapped,
    /// The project left it out; the scene has its default.
    Defaulted,
    /// It changes nothing a frame shows.
    Ignored,
    /// The scene cannot hold it yet.
    Unsupported,
}

/// How often one match name met each fate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Counts {
    /// Mapped into the scene.
    pub mapped: usize,
    /// Left out by the project and given its default.
    pub defaulted: usize,
    /// Ignored, because it changes nothing a frame shows.
    pub ignored: usize,
    /// Not supported yet.
    pub unsupported: usize,
}

/// One thing the scene cannot hold yet, such as a feature or a font the
/// import had no face for, and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Where it is: composition, layer and the match names down to it.
    pub at: String,
    /// What is not supported, such as `expressions`, `track mattes` or
    /// ``missing fonts, substituted (`Brand-Bold → InterVariable-Bold`)``.
    pub what: String,
}

/// The id a composition or layer name became.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Named {
    /// The composition it is in, by id; `None` for a composition.
    pub comp: Option<String>,
    /// The name in the project.
    pub name: String,
    /// The id in the scene.
    pub id: String,
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Mapping {
    /// The composition that became the scene.
    pub scene: String,
    /// Every match name met, with its counts.
    pub counts: BTreeMap<String, Counts>,
    /// Everything not supported, in the order met.
    pub unsupported: Vec<Finding>,
    /// The fonts the project names by another name of a face the import
    /// has, with that face's PostScript name, which the scene names it by
    /// ([`Import::rename_fonts`](super::Import::rename_fonts)).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub renamed_fonts: BTreeMap<String, String>,
    /// Why things were ignored, with how often each reason applied.
    pub ignored: BTreeMap<String, usize>,
    /// Every composition and layer, with its id.
    pub ids: Vec<Named>,
    /// Every expression the import met that is switched on, and what it
    /// became.
    pub expressions: Vec<ExpressionReport>,
    /// Every expression control the import met, with its values: what
    /// expressions read, kept for live links.
    pub controls: Vec<ControlReport>,
    /// Values outside the range the scene takes, clamped into it as the
    /// original draws them, and where.
    pub clamped: Vec<Finding>,
    /// How many slots the controls that expressions read became.
    pub slots: usize,
    /// The roles proposed for the designer to confirm.
    pub roles: Vec<super::roles::ProposedRole>,
    /// The locks the import wrote: none since D77, as a template's logo,
    /// palette and type are what its user replaces.
    pub locks: Vec<String>,
    /// The slots made of what a designer would vary ([D77]): words,
    /// palette, media and timing.
    ///
    /// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
    pub variables: super::Variables,
    /// What became of a template's panel ([D53]), for a `.mogrt`.
    ///
    /// [D53]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d53-a-mogrt-imports-as-its-project-with-its-controls-as-slots
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<PanelReport>,
}

/// What became of a template's panel.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct PanelReport {
    /// How many controls it offers, groups left out.
    pub controls: usize,
    /// How many became slots.
    pub slots: usize,
    /// The ids of those slots, in the panel's order.
    pub ids: Vec<String>,
    /// The controls that did not, by label, and why.
    pub left_out: Vec<Finding>,
}

/// What one expression became.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExpressionReport {
    /// Where its property is, as `composition/layer/match name/…`.
    pub at: String,
    /// Its source, whole, as the project stores it, so a live link can
    /// later be made from it.
    pub source: String,
    /// The engine it was evaluated as: `modern` or `legacy`.
    pub engine: String,
    /// How many frames it was evaluated at.
    pub frames: usize,
    /// What it became.
    pub outcome: Outcome,
}

/// An expression control: a slider, colour, checkbox, angle, point or layer
/// that expressions read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ControlReport {
    /// Where it is, as `composition/layer/…`.
    pub at: String,
    /// The name the designer gave it, else its kind's.
    pub name: String,
    /// Its match name, such as `ADBE Color Control`.
    pub kind: String,
    /// Its parameters' values, by name, as the project stores them; a keyed
    /// one as its keys.
    pub values: BTreeMap<String, serde_json::Value>,
}

/// What an expression became.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Outcome {
    /// Keys, in place of those under it.
    Baked {
        /// Whether it used noise or randomness, which the bake approximates.
        approximate: bool,
        /// How many keys the property had under it.
        keys_before: usize,
        /// How many keys it became, over all the tracks it is written to.
        keys: usize,
        /// The largest distance from a sample to the keys, per component,
        /// in the project's units.
        max_error: Vec<f64>,
    },
    /// A reference to a slot, which holds the control's value it read.
    Slot {
        /// The slot's id.
        slot: String,
    },
    /// Nothing: the keys under it are kept.
    NotBaked {
        /// Why.
        why: String,
    },
}

impl Mapping {
    /// Counts one `fate` for `match_name`.
    pub(super) fn count(&mut self, match_name: &str, fate: Fate) {
        let counts = self.counts.entry(match_name.to_owned()).or_default();
        match fate {
            Fate::Mapped => counts.mapped += 1,
            Fate::Defaulted => counts.defaulted += 1,
            Fate::Ignored => counts.ignored += 1,
            Fate::Unsupported => counts.unsupported += 1,
        }
    }

    /// Counts `match_name` as ignored, for `why`.
    pub(super) fn ignore(&mut self, match_name: &str, why: &str) {
        self.count(match_name, Fate::Ignored);
        *self.ignored.entry(why.to_owned()).or_default() += 1;
    }

    /// Records that `what`, at `at`, is not supported.
    pub(super) fn unsupported(&mut self, at: &str, what: impl Into<String>) {
        self.unsupported.push(Finding {
            at: at.to_owned(),
            what: what.into(),
        });
    }

    /// Records that values at `at` were clamped into the scene's range.
    pub(super) fn clamped(&mut self, at: &str, what: impl Into<String>) {
        self.clamped.push(Finding {
            at: at.to_owned(),
            what: what.into(),
        });
    }

    /// The findings grouped by what is unsupported, each with where.
    pub fn by_what(&self) -> BTreeMap<&str, Vec<&str>> {
        self.unsupported
            .iter()
            .fold(BTreeMap::new(), |mut groups, finding| {
                groups
                    .entry(finding.what.as_str())
                    .or_insert_with(Vec::new)
                    .push(finding.at.as_str());
                groups
            })
    }
}

/// The scene's composition, the totals, and each unsupported feature with
/// how often and where it was met (the first three places).
impl fmt::Display for Mapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = |pick: fn(&Counts) -> usize| self.counts.values().map(pick).sum::<usize>();
        writeln!(f, "imported `{}`", self.scene)?;
        writeln!(
            f,
            "  {} mapped, {} defaulted, {} ignored, {} unsupported, over {} match names",
            total(|c| c.mapped),
            total(|c| c.defaulted),
            total(|c| c.ignored),
            total(|c| c.unsupported),
            self.counts.len()
        )?;
        if !self.ignored.is_empty() {
            writeln!(f, "  ignored, drawing nothing:")?;
            self.ignored
                .iter()
                .try_for_each(|(why, count)| writeln!(f, "    {why} ({count})"))?;
        }
        if !self.expressions.is_empty() {
            let baked: Vec<(bool, usize, usize)> = self
                .expressions
                .iter()
                .filter_map(|e| match &e.outcome {
                    Outcome::Baked {
                        approximate,
                        keys_before,
                        keys,
                        ..
                    } => Some((*approximate, *keys_before, *keys)),
                    Outcome::NotBaked { .. } | Outcome::Slot { .. } => None,
                })
                .collect();
            let followers = self
                .expressions
                .iter()
                .filter(|e| matches!(e.outcome, Outcome::Slot { .. }))
                .count();
            writeln!(
                f,
                "  expressions: {} baked ({} approximate) into {} keys, where {} were; {} not baked",
                baked.len(),
                baked.iter().filter(|b| b.0).count(),
                baked.iter().map(|b| b.2).sum::<usize>(),
                baked.iter().map(|b| b.1).sum::<usize>(),
                self.expressions.len() - baked.len() - followers
            )?;
            if self.slots > 0 {
                writeln!(
                    f,
                    "  slots: {} from the controls expressions read, which {followers} values follow",
                    self.slots
                )?;
            }
        }
        let v = &self.variables;
        if v.text + v.colors + v.media + v.screens + v.timing > 0 {
            writeln!(
                f,
                "  slots to vary: {} text, {} colour, {} media, {} placeholder, {} timing",
                v.text, v.colors, v.media, v.screens, v.timing
            )?;
        }
        if !v.edit_comps.is_empty() {
            writeln!(f, "  compositions for editing: {}", v.edit_comps.join(", "))?;
        }
        if !self.roles.is_empty() {
            let shown: Vec<String> = self
                .roles
                .iter()
                .map(|r| format!("{} {} ({})", r.role, r.address, r.from))
                .collect();
            writeln!(f, "  roles, to confirm: {}", shown.join(", "))?;
        }
        if !self.locks.is_empty() {
            writeln!(
                f,
                "  locks, as a master with a logo has: {}",
                self.locks.join(", ")
            )?;
        }
        if let Some(panel) = &self.panel {
            writeln!(
                f,
                "  template: {}, {}",
                plural(panel.controls, "control"),
                plural(panel.slots, "slot")
            )?;
            panel
                .left_out
                .iter()
                .try_for_each(|c| writeln!(f, "    left out: {} ({})", c.at, c.what))?;
        }
        if !self.clamped.is_empty() {
            let shown: Vec<String> = self
                .clamped
                .iter()
                .take(3)
                .map(|c| format!("{} ({})", c.at, c.what))
                .collect();
            let more = match self.clamped.len().saturating_sub(3) {
                0 => String::new(),
                n => format!(", and {n} more"),
            };
            writeln!(
                f,
                "  clamped into the scene's range, as the original draws them ({}): {}{more}",
                self.clamped.len(),
                shown.join("; ")
            )?;
        }
        if !self.renamed_fonts.is_empty() {
            let shown: Vec<String> = self
                .renamed_fonts
                .iter()
                .map(|(from, to)| format!("`{from}` is `{to}`"))
                .collect();
            writeln!(
                f,
                "  fonts under another name of their face: {}",
                shown.join(", ")
            )?;
        }
        let groups = self.by_what();
        if groups.is_empty() {
            return writeln!(f, "  unsupported: none");
        }
        writeln!(f, "  unsupported:")?;
        groups.iter().try_for_each(|(what, places)| {
            let shown: Vec<&str> = places.iter().take(3).copied().collect();
            let more = match places.len().saturating_sub(3) {
                0 => String::new(),
                n => format!(", and {n} more"),
            };
            writeln!(
                f,
                "    {what} ({}): {}{more}",
                places.len(),
                shown.join("; ")
            )
        })
    }
}
