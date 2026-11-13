//! The `at` query: where things are at one moment.
//!
//! Part of step 4 (Query). Prints resolved numbers, so the agent never has to
//! do easing or layout maths itself.

use motion_eval::evaluate;
use motion_format::fmt_num;
use motion_scene::SceneGraph;
use std::fmt;

/// Why a query could not be answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    /// A message ready to show to the user.
    pub message: String,
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QueryError {}

fn error(message: impl Into<String>) -> QueryError {
    QueryError {
        message: message.into(),
    }
}

/// Describes the requested nodes at `frame`, one line each.
///
/// An empty `ids` means every node. Lines follow scene order, and
/// the format is `{id} x={x} y={y} w={w} h={h} opacity={o}`.
pub fn at(graph: &SceneGraph, frame: f64, ids: &[String]) -> Result<String, QueryError> {
    if frame < 0.0 {
        return Err(error("time must be 0 or later"));
    }
    // Reject unknown ids up front so a typo never yields a partial answer.
    if let Some(unknown) = ids
        .iter()
        .find(|id| !graph.nodes.iter().any(|n| &n.id == *id))
    {
        return Err(error(format!("no layer with id `{unknown}`")));
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
mod tests {
    use crate::at;
    #[test]
    fn at_prints_resolved_values() {
        let s = motion_format::parse_document(include_str!("../../../examples/launch.json"))
            .unwrap()
            .scene;
        let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
        assert_eq!(
            at(&g, 200.0, &["title".into()]).unwrap(),
            "title x=960 y=260 w=580.8 h=115.2 opacity=1"
        );
        assert_eq!(at(&g, 0.0, &[]).unwrap().lines().count(), 2);
        assert_eq!(
            at(&g, 0.0, &["nope".into()]).unwrap_err().to_string(),
            "no layer with id `nope`"
        );
        assert_eq!(
            at(&g, -1.0, &[]).unwrap_err().to_string(),
            "time must be 0 or later"
        );
        let empty = motion_scene::compile(
            &motion_format::parse_document(r#"{"motion":1,"dur":"1s","layers":[]}"#)
                .unwrap()
                .scene,
            &motion_scene::ApproxTextMeasure,
        );
        assert_eq!(at(&empty, 0.0, &[]).unwrap(), "");
    }
}
