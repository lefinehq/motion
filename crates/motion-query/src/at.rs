//! The `at` query: resolved numbers at one moment, so an agent never does easing
//! or layout maths itself.

use kurbo::Point;
use motion_format::fmt_num;
use motion_format::property::{Prop, Rest};
use motion_format::shorthand::{Coord, Position};
use motion_format::{Diagnostic, Kind, Result};
use motion_scene::{NodeState, SceneGraph, evaluate};
use tracing::instrument;

/// One line per node in `ids` (all when empty) at `frame`: `{id} x= y= w= h=`, with `x`
/// and `y` where its anchor lands on the canvas, then each other property whose printed
/// value differs from its default, so float dust stays hidden.
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
    // The property `at` sets is always printed, first, by its components.
    let changed: String = Prop::all()
        .filter(|&prop| prop.spec().rest != Rest::At)
        .filter_map(|prop| {
            let shown = node.get(prop).to_string();
            let rest = rest_shown(prop, node);
            (shown != rest).then(|| format!(" {}={shown}", prop.name()))
        })
        .collect();
    // Where the anchor lands on the canvas: the position, through any parents.
    let [ax, ay] = node.point(Prop::ANCHOR);
    let Point { x, y } = node.transform * Point::new(ax, ay);
    format!(
        "{} x={} y={} w={} h={}{changed}",
        node.id,
        fmt_num(x),
        fmt_num(y),
        fmt_num(node.own_box.width()),
        fmt_num(node.own_box.height())
    )
}

/// How the registry's resting value of `prop` prints for `node`'s box.
fn rest_shown(prop: Prop, node: &NodeState) -> String {
    let at = Position {
        x: Coord::Px(0.0),
        y: Coord::Px(0.0),
    };
    let rest = prop.spec().rest(&at, kurbo::Rect::ZERO, node.own_box);
    rest.to_string()
}

#[cfg(test)]
#[path = "../tests/unit/at.rs"]
mod tests;
