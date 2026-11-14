//! The `at` query: resolved numbers at one moment, so an agent never does easing
//! or layout maths itself.

use motion_format::fmt_num;
use motion_format::property::Prop;
use motion_format::{Diagnostic, Kind, Result};
use motion_scene::SceneGraph;
use motion_scene::evaluate;
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
        .map(|n| {
            let mut line = format!(
                "{} x={} y={} w={} h={}",
                n.id,
                fmt_num(n.get(Prop::X)),
                fmt_num(n.get(Prop::Y)),
                fmt_num(n.width),
                fmt_num(n.height)
            );
            // The properties `at` sets (x and y) are always printed above.
            for prop in Prop::all().filter(|p| p.spec().rest_from_at.is_none()) {
                let shown = fmt_num(n.get(prop));
                if shown != fmt_num(prop.spec().default) {
                    line.push_str(&format!(" {}={shown}", prop.name()));
                }
            }
            line
        })
        .collect();
    Ok(lines.join("\n"))
}

#[cfg(test)]
#[path = "../tests/unit/at.rs"]
mod tests;
