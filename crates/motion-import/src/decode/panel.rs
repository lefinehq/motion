//! A template panel's controls, in the composition the template was made
//! from ([D53]): each `CCtl` list of its `CIF3`, with the id, the kind and
//! the `CPrp` targets of the control.
//!
//! [D53]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d53-a-mogrt-imports-as-its-project-with-its-controls-as-slots

use std::collections::BTreeMap;

use motion_format::Result;
use serde::Deserialize;

use super::{Decoder, Scan, UTF8};
use crate::model::{ControlKind, ControlTarget, PanelControl, PathStep};
use crate::record::{Ccid, Clid, Ctyp, text};
use crate::rifx::Name;

/// The panel's controls, the newest of the three copies a composition keeps.
pub(super) const CIF3: Name = Name::list(b"CIF3");
/// One control.
const CCTL: Name = Name::list(b"CCtl");
/// One property a control drives.
const CPRP: Name = Name::list(b"CPrp");

/// What the file gives for an index it does not use.
const NO_INDEX: u32 = u32::MAX;

/// One step of a target's path as the file writes it, under its position.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    match_name: String,
    index: u32,
}

impl Decoder {
    /// The controls of a `CIF3` list, in the panel's order.
    pub(super) fn panel(&self, mut cif3: Scan<'_, '_>) -> Result<Vec<PanelControl>> {
        cif3.lists(CCTL)
            .map(|control| self.panel_control(control?))
            .collect()
    }

    /// One `CCtl`: its id, the first `Utf8` among its own children, then its
    /// kind and its targets.
    fn panel_control(&self, mut scan: Scan<'_, '_>) -> Result<PanelControl> {
        let id = scan.find(UTF8).map(|c| text(c.body)).unwrap_or_default();
        let kind = ControlKind::from_code(scan.record::<Ctyp>()?.code);
        let targets = scan
            .lists(CPRP)
            .map(|target| self.target(target?))
            .filter_map(Result::transpose)
            .collect::<Result<_>>()?;
        Ok(PanelControl { id, kind, targets })
    }

    /// One `CPrp`: the composition, the layer and the path, a JSON object of
    /// steps by position. A path that does not read is counted as skipped,
    /// and the control drives nothing there.
    fn target(&self, mut scan: Scan<'_, '_>) -> Result<Option<ControlTarget>> {
        let composition = scan.record::<Ccid>()?.id;
        let layer = scan.record::<Clid>()?.id;
        let steps = scan
            .find(UTF8)
            .and_then(|path| serde_json::from_str::<BTreeMap<String, Step>>(&text(path.body)).ok())
            .and_then(|steps| {
                let mut steps = steps
                    .into_iter()
                    .map(|(at, step)| at.parse::<usize>().ok().map(|at| (at, step)))
                    .collect::<Option<Vec<_>>>()?;
                steps.sort_by_key(|(at, _)| *at);
                Some(steps)
            });
        let Some(steps) = steps else {
            self.skip([CPRP]);
            return Ok(None);
        };
        Ok(Some(ControlTarget {
            composition,
            layer,
            path: steps
                .into_iter()
                .map(|(_, step)| PathStep {
                    match_name: step.match_name,
                    index: (step.index != NO_INDEX).then_some(step.index),
                })
                .collect(),
        }))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/decode/panel.rs"]
mod tests;
