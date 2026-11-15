//! A template's panel as slots ([D53]): each control the panel offers
//! becomes a slot of the scene, named and labelled by the panel, in its
//! order. The properties it drives follow it: a property of a layer
//! directly, where its value is still, and an expression control through
//! the expressions that read it ([`super::slots`]).
//!
//! [D53]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d53-a-mogrt-imports-as-its-project-with-its-controls-as-slots

use std::collections::HashMap;

use motion_format::SlotType;

use super::Walk;
use super::report::{Finding, PanelReport};
use super::slots::{Held, Offered, address, still};
use crate::model::{Child, ControlKind, Group, ItemKind, Layer, PanelControl, PathStep, Property};
use crate::template::Control;

impl<'p> Walk<'p> {
    /// Offers each control of `panel` as a slot, in its order, with the
    /// properties the project says it drives. Groups name the controls in
    /// them and are no slot.
    pub(super) fn offer_panel(&mut self, panel: &[Control]) {
        let mut report = PanelReport::default();
        let kept: HashMap<&str, &'p PanelControl> = self
            .project
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Composition(c) => Some(&c.panel),
                _ => None,
            })
            .flatten()
            .map(|control| (control.id.as_str(), control))
            .collect();
        let groups: HashMap<&str, &str> = panel
            .iter()
            .filter(|c| c.kind == ControlKind::Group)
            .flat_map(|group| {
                group
                    .value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| id.as_str())
                    .map(|id| (id, group.name.as_str()))
            })
            .collect();
        for control in panel.iter().filter(|c| c.kind != ControlKind::Group) {
            report.controls += 1;
            let label = match groups.get(control.id.as_str()) {
                Some(group) => format!("{group} / {}", control.name),
                None => control.name.clone(),
            };
            let left_out = |why: &str| Finding {
                at: label.clone(),
                what: why.to_owned(),
            };
            let Some(ty) = slot_type(control.kind) else {
                report
                    .left_out
                    .push(left_out("a kind of control no slot holds"));
                continue;
            };
            let Some(kept) = kept.get(control.id.as_str()) else {
                report
                    .left_out
                    .push(left_out("the project does not keep it"));
                continue;
            };
            let held: Vec<(Held, Option<&'p Property>)> = kept
                .targets
                .iter()
                .filter_map(|target| {
                    let layer = self
                        .items
                        .get(&target.composition)
                        .and_then(|item| match &item.kind {
                            ItemKind::Composition(c) => {
                                c.layers.iter().find(|l| l.id == target.layer)
                            }
                            _ => None,
                        })?;
                    resolve(layer, &target.path)
                })
                .collect();
            if held.is_empty() {
                report
                    .left_out
                    .push(left_out(
                        "the property it drives is not one a slot can follow (missing, or kept as separate dimensions)",
                    ));
                continue;
            }
            let index = self.slots.offer(Offered {
                name: control.name.clone(),
                label,
                ty: (ty == SlotType::Text).then_some(ty),
                properties: held.iter().map(|(h, _)| h.clone()).collect(),
                stored: held.iter().find_map(|(_, p)| p.and_then(still)),
            });
            for (held, _) in held {
                self.offered.insert(held, index);
            }
        }
        self.mapping.panel = Some(report);
    }
}

/// The type of the slot a control of `kind` becomes.
const fn slot_type(kind: ControlKind) -> Option<SlotType> {
    match kind {
        ControlKind::Text => Some(SlotType::Text),
        ControlKind::Color => Some(SlotType::Color),
        ControlKind::Checkbox | ControlKind::Slider | ControlKind::Angle => Some(SlotType::Number),
        ControlKind::Point | ControlKind::Scale => Some(SlotType::Point),
        ControlKind::Group | ControlKind::Other(_) => None,
    }
}

/// The match name of a layer's transform group.
const TRANSFORM: &str = "ADBE Transform Group";

/// What holds the property `path` names below `layer`, and the property
/// when the project stores it: each group by match name, or by its index
/// among its siblings where the path gives one, then the property by match
/// name. A property the project leaves out at its default is held by its
/// group, or by the layer for its transform, which the project may leave
/// out whole. A transform property kept as separate dimensions is held by
/// nothing one slot can follow.
fn resolve<'a>(layer: &'a Layer, path: &[PathStep]) -> Option<(Held, Option<&'a Property>)> {
    let (last, groups) = path.split_last()?;
    let found = |group: &'a Group| group.properties().find(|p| p.match_name == last.match_name);
    let held = |owner: usize, property: Option<&'a Property>| match property {
        Some(property) => (Held::Stored(address(property)), Some(property)),
        None => (Held::Default(owner, last.match_name.clone()), None),
    };
    if let [group] = groups
        && group.match_name == TRANSFORM
    {
        let transform = child_groups(&layer.properties).find(|g| g.match_name == TRANSFORM);
        let property = transform.and_then(found);
        // A position kept as separate dimensions is not left out: its
        // `_0` and `_1` hold it, and no one point follows a slot.
        let separated = format!("{}_0", last.match_name);
        if property.is_none()
            && transform.is_some_and(|t| t.properties().any(|p| p.match_name == separated))
        {
            return None;
        }
        return Some(held(address(layer), property));
    }
    let mut here = &layer.properties;
    for step in groups {
        let mut children = child_groups(here);
        here = match step.index {
            Some(index) => children
                .nth(usize::try_from(index).ok()?)
                .filter(|g| g.match_name == step.match_name)?,
            None => children.find(|g| g.match_name == step.match_name)?,
        };
    }
    Some(held(address(here), found(here)))
}

/// The groups among `group`'s children.
fn child_groups(group: &Group) -> impl Iterator<Item = &Group> {
    group.children.iter().filter_map(|child| match child {
        Child::Group(group) => Some(group),
        Child::Property(_) => None,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/map/panel.rs"]
mod tests;
