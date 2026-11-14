//! The import report: what a [`Project`] holds, counted, and what the reader
//! could not decode yet, so support can grow by how often a feature appears.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::model::{Child, Group, ItemKind, Project, Property, Stream, Value, label};

/// The match name of a layer's effects group.
const EFFECTS: &str = "ADBE Effect Parade";

/// Counts of what a project holds, each by name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Report {
    /// Items by kind: `folder`, `composition`, `footage`.
    pub items: BTreeMap<String, usize>,
    /// Layers by kind, with nulls and adjustment layers apart.
    pub layers: BTreeMap<String, usize>,
    /// Effects by match name.
    pub effects: BTreeMap<String, usize>,
    /// Properties whose values are opaque, by reason.
    pub opaque: BTreeMap<String, usize>,
    /// Chunks the reader passed over, by name.
    pub skipped: BTreeMap<String, usize>,
}

impl Project {
    /// What the project holds, counted.
    pub fn report(&self) -> Report {
        let compositions = self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::Composition(composition) => Some(composition),
            _ => None,
        });
        let layers: Vec<_> = compositions.flat_map(|c| &c.layers).collect();
        let effects = layers
            .iter()
            .flat_map(|layer| groups(&layer.properties))
            .filter(|group| group.match_name == EFFECTS)
            .flat_map(groups)
            .map(|effect| effect.match_name.clone());
        let opaque = layers
            .iter()
            .flat_map(|layer| layer.properties.properties())
            .filter_map(opaque_reason);
        Report {
            items: counts(self.items.iter().map(|item| item.kind.name().to_owned())),
            layers: counts(layers.iter().map(|layer| {
                match (layer.switches.null, layer.switches.adjustment) {
                    (true, _) => "null".to_owned(),
                    (_, true) => "adjustment".to_owned(),
                    _ => label(&layer.kind),
                }
            })),
            effects: counts(effects),
            opaque: counts(opaque),
            skipped: self.skipped.clone(),
        }
    }
}

/// The groups among `group`'s children.
fn groups(group: &Group) -> impl Iterator<Item = &Group> {
    group.children.iter().filter_map(|child| match child {
        Child::Group(group) => Some(group),
        Child::Property(_) => None,
    })
}

/// Why `property`'s value is opaque, if it is.
fn opaque_reason(property: &Property) -> Option<String> {
    let value = match &property.value {
        Stream::Static(value) => Some(value),
        Stream::Animated(keyframes) => keyframes.first().map(|key| &key.value),
    };
    match value {
        Some(Value::Opaque { reason }) => Some(label(reason)),
        _ => None,
    }
}

/// How often each name appears.
fn counts(names: impl Iterator<Item = String>) -> BTreeMap<String, usize> {
    names.fold(BTreeMap::new(), |mut counts, name| {
        *counts.entry(name).or_default() += 1;
        counts
    })
}

/// One line per count, as `layers: shape 3, text 1`.
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sections = [
            ("items", &self.items),
            ("layers", &self.layers),
            ("effects", &self.effects),
            ("opaque", &self.opaque),
            ("skipped", &self.skipped),
        ];
        writeln!(f, "report")?;
        sections.iter().try_for_each(|(title, counts)| {
            let line: Vec<String> = counts
                .iter()
                .map(|(name, count)| format!("{name} {count}"))
                .collect();
            match line.is_empty() {
                true => writeln!(f, "  {title}: none"),
                false => writeln!(f, "  {title}: {}", line.join(", ")),
            }
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/report.rs"]
mod tests;
