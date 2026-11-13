//! The `at` query: where things are at one moment.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). Prints resolved numbers, so the agent never has to
//! do easing or layout maths itself.

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_eval::evaluate;
use motion_format::fmt_num;
use motion_format::property::{PROPERTIES, names};
use motion_scene::SceneGraph;

/// Describes the requested nodes at `frame`, one line each.
///
/// An empty `ids` means every node. Lines follow scene order, and
/// the format is `{id} x={x} y={y} w={w} h={h}` followed by every other
/// registered property, in registry order, whose value differs from its
/// default, as `name={value}`. Values are compared as printed, so float dust
/// such as a scale of 1.0000000000000002 does not show up.
pub fn at(graph: &SceneGraph, frame: f64, ids: &[String]) -> Result<String> {
    if frame < 0.0 {
        return Err(Diagnostic::new(Kind::OutOfRange, "time must be 0 or later"));
    }
    // Reject unknown ids up front so a typo never yields a partial answer.
    if let Some(unknown) = ids
        .iter()
        .find(|id| !graph.nodes.iter().any(|n| &n.id == *id))
    {
        return Err(Diagnostic::new(
            Kind::UnknownName,
            format!("no layer with id `{unknown}`"),
        ));
    }
    let lines: Vec<String> = evaluate(graph, frame)
        .nodes
        .iter()
        .filter(|n| ids.is_empty() || ids.contains(&n.id))
        .map(|n| {
            let mut line = format!(
                "{} x={} y={} w={} h={}",
                n.id,
                fmt_num(n.get(names::X)),
                fmt_num(n.get(names::Y)),
                fmt_num(n.width),
                fmt_num(n.height)
            );
            for (spec, &value) in PROPERTIES.iter().zip(&n.values) {
                // x and y are always printed above.
                if spec.name == names::X || spec.name == names::Y {
                    continue;
                }
                let shown = fmt_num(value);
                if shown != fmt_num(spec.default) {
                    line.push_str(&format!(" {}={shown}", spec.name));
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
