//! The mapping report: every match name the import met, with how often it
//! was mapped, left at its default, ignored or not supported, each reason
//! not supported with where it was met (fonts set in a substitute among
//! them), and the id every name became.

use std::collections::BTreeMap;
use std::fmt;

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
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Mapping {
    /// The composition that became the scene.
    pub scene: String,
    /// Every match name met, with its counts.
    pub counts: BTreeMap<String, Counts>,
    /// Everything not supported, in the order met.
    pub unsupported: Vec<Finding>,
    /// Why things were ignored, with how often each reason applied.
    pub ignored: BTreeMap<String, usize>,
    /// Every composition and layer, with its id.
    pub ids: Vec<Named>,
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
