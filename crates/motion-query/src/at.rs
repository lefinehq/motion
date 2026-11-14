//! The `at` query: resolved numbers at one moment, so an agent never does easing
//! or layout maths itself.

use motion_format::fmt_num;
use motion_format::property::Prop;
use motion_format::{Diagnostic, Kind, Result};
use motion_scene::{NodeState, SceneGraph, evaluate};
use tracing::instrument;

/// One line per node in `ids` (all when empty) at `frame`: `{id} x= y= w= h=`, then each
/// other property whose printed value differs from its default, so float dust stays hidden.
#[instrument(level = "debug", skip(graph, ids), fields(ids = ids.len()))]
pub fn at(graph: &SceneGraph, frame: f64, ids: &[String]) -> Result<String> {
    if frame < 0.0 {
        return Err(Diagnostic::new(Kind::OutOfRange, "time must be 0 or later"));
    }
    // Reject unknown ids up front so a typo never yields a partial answer.
    if let Some(unknown) = ids
        .iter()
        .find(|id| !graph.nodes.iter().any(|n| &n.id == *id))
    {
        let ids = graph.nodes.iter().map(|n| &n.id);
        return Err(Diagnostic::unknown("layer id", unknown, ids));
    }
    let lines: Vec<String> = evaluate(graph, frame)
        .nodes
        .iter()
        .filter(|n| ids.is_empty() || ids.contains(&n.id))
        .map(describe)
        .collect();
    Ok(lines.join("\n"))
}

/// The line [`at`] prints for `node`.
fn describe(node: &NodeState) -> String {
    // The properties `at` sets (x and y) are always printed, first.
    let changed: String = Prop::all()
        .filter(|prop| prop.spec().rest_from_at.is_none())
        .filter_map(|prop| {
            let shown = fmt_num(node.get(prop));
            (shown != fmt_num(prop.spec().default)).then(|| format!(" {}={shown}", prop.name()))
        })
        .collect();
    format!(
        "{} x={} y={} w={} h={}{changed}",
        node.id,
        fmt_num(node.get(Prop::X)),
        fmt_num(node.get(Prop::Y)),
        fmt_num(node.width),
        fmt_num(node.height)
    )
}

#[cfg(test)]
#[path = "../tests/unit/at.rs"]
mod tests;
