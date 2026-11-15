//! The `at` query: resolved numbers at one moment, so an agent never does easing
//! or layout maths itself.

use kurbo::Point;
use motion_format::fmt_num;
use motion_format::property::{Prop, Rest};
use motion_format::shorthand::{Coord, Position};
use motion_format::{Address, Diagnostic, Kind, Result};
use motion_scene::{NodeState, SceneGraph, Visit, address_of, evaluate, walk};
use tracing::instrument;

/// One line per node in `ids`, layer addresses such as `title` or
/// `card/title` (every node at any depth when empty), at `frame`:
/// `{address} x= y= w= h=`, with `x` and `y` where its anchor lands on the
/// scene's canvas, then each other property whose printed value differs
/// from its default, so float dust stays hidden. A node inside a
/// composition that is not shown at `frame` has no line.
///
/// # Errors
/// A diagnostic for a negative frame, or an entry of `ids` that is not the
/// address of a layer.
#[instrument(level = "debug", skip(graph, ids), fields(ids = ids.len()))]
pub fn at(graph: &SceneGraph, frame: f64, ids: &[String]) -> Result<String> {
    if frame < 0.0 {
        return Err(Diagnostic::new(Kind::OutOfRange, "time must be 0 or later"));
    }
    // Reject unknown addresses up front so a typo never yields a partial answer.
    let known: Vec<Address> = graph.paths().iter().map(|ids| address_of(ids)).collect();
    let wanted = ids
        .iter()
        .map(|text| {
            text.parse::<Address>()
                .ok()
                .filter(|address| known.contains(address))
                .ok_or_else(|| Diagnostic::unknown("layer", text, &known))
        })
        .collect::<Result<Vec<Address>>>()?;
    let state = evaluate(graph, frame);
    let lines: Vec<String> = walk(graph, &state)
        .map(|visit| (visit.address(), visit))
        .filter(|(address, _)| wanted.is_empty() || wanted.contains(address))
        .map(|(address, visit)| describe(&address, &visit))
        .collect();
    Ok(lines.join("\n"))
}

/// The line [`at`] prints for the node at `address`.
fn describe(address: &Address, visit: &Visit<'_>) -> String {
    let node = visit.node;
    // The property `at` sets is always printed, first, by its components.
    let changed: String = Prop::all()
        .filter(|&prop| prop.spec().rest != Rest::At)
        .filter_map(|prop| {
            let shown = node.get(prop).to_string();
            let rest = rest_shown(prop, node);
            (shown != rest).then(|| format!(" {}={shown}", prop.name()))
        })
        .collect();
    // Where the anchor lands on the scene's canvas: the position, through
    // any parents and the `comp` nodes around it.
    let [ax, ay] = node.point(Prop::ANCHOR);
    let Point { x, y } = visit.transform * Point::new(ax, ay);
    format!(
        "{address} x={} y={} w={} h={}{changed}",
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
