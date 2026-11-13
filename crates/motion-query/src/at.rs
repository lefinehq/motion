//! The `at` query: where things are at one moment.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). Prints resolved numbers, so the agent never has to
//! do easing or layout maths itself.

use motion_eval::evaluate;
use motion_format::fmt_num;
use motion_scene::SceneGraph;

use crate::error::QueryError;

/// Describes the requested nodes at `frame`, one line each.
///
/// An empty `ids` means every node. Lines follow scene order, and
/// the format is `{id} x={x} y={y} w={w} h={h} opacity={o}`.
pub fn at(graph: &SceneGraph, frame: f64, ids: &[String]) -> Result<String, QueryError> {
    if frame < 0.0 {
        return Err(QueryError::new("time must be 0 or later"));
    }
    // Reject unknown ids up front so a typo never yields a partial answer.
    if let Some(unknown) = ids
        .iter()
        .find(|id| !graph.nodes.iter().any(|n| &n.id == *id))
    {
        return Err(QueryError::new(format!("no layer with id `{unknown}`")));
    }
    let lines: Vec<String> = evaluate(graph, frame)
        .nodes
        .iter()
        .filter(|n| ids.is_empty() || ids.contains(&n.id))
        .map(|n| {
            format!(
                "{} x={} y={} w={} h={} opacity={}",
                n.id,
                fmt_num(n.x),
                fmt_num(n.y),
                fmt_num(n.width),
                fmt_num(n.height),
                fmt_num(n.opacity)
            )
        })
        .collect();
    Ok(lines.join("\n"))
}

#[cfg(test)]
#[path = "../tests/unit/at.rs"]
mod tests;
