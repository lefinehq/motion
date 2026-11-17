//! Unit tests for `src/merge.rs`: what each mode makes of the outlines above
//! it, which outlines it reaches and what the paints around it draw.

use crate::{ContentState, ShapeDraw, compile, evaluate};
use kurbo::{BezPath, PathEl, Shape};
use nakkash_format::parse_document;
use nakkash_text::{FontSet, Typesetter};

/// The frame-0 draws of a scene whose one layer holds `shapes`.
fn draws(shapes: &str) -> Vec<ShapeDraw> {
    let json = format!(
        r#"{{"nakkash":1,"dur":"1s","fps":10,"layers":[{{"id":"a","shapes":[{shapes}]}}]}}"#
    );
    let doc = parse_document(&json).unwrap();
    let graph = compile(doc.scene(), &mut Typesetter::new(FontSet::builtin())).unwrap();
    match evaluate(&graph, 0.0).nodes.swap_remove(0).content {
        ContentState::Shapes(draws) => draws,
        other => panic!("not shapes: {other:?}"),
    }
}

fn one(shapes: &str) -> BezPath {
    let mut all = draws(shapes);
    assert_eq!(all.len(), 1, "{all:?}");
    all.remove(0).outline
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 0.5, "{a} vs {b}");
}

fn subpaths(path: &BezPath) -> usize {
    path.elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count()
}

/// Two 100 px squares, the second moved by 50 px both ways: they overlap
/// in a 50 px square.
const SQUARES: &str = r#"{"rect":[100,100]},{"rect":[100,100],"position":[50,50]}"#;
const FILL: &str = r##"{"fill":"#FFFFFF"}"##;

#[test]
fn each_mode_combines_two_squares() {
    for (mode, area) in [
        ("add", 17_500.0),
        ("subtract", 7_500.0),
        ("intersect", 2_500.0),
        ("exclude", 15_000.0),
    ] {
        let path = one(&format!(r#"{SQUARES},{{"merge":"{mode}"}},{FILL}"#));
        near(path.area().abs(), area);
    }
}

#[test]
fn merge_keeps_the_outlines_as_they_are() {
    let path = one(&format!(r#"{SQUARES},{{"merge":"merge"}},{FILL}"#));
    assert_eq!(subpaths(&path), 2);
    // Both squares run clockwise, so their signed areas add up, overlap
    // and all.
    near(path.area().abs(), 20_000.0);
}

#[test]
fn the_first_outline_is_the_one_others_are_taken_from() {
    // A third square away from both: subtracting it changes nothing, and
    // the order of the first two decides which corner is cut.
    let path = one(&format!(
        r#"{SQUARES},{{"rect":[10,10],"position":[500,500]}},{{"merge":"subtract"}},{FILL}"#
    ));
    near(path.area().abs(), 7_500.0);
    let bounds = path.bounding_box();
    near(bounds.x0, -50.0);
    near(bounds.x1, 50.0);
}

#[test]
fn paints_above_a_merge_draw_nothing_of_what_it_takes() {
    // The stroke above the merge, in its group, and the fill in the inner
    // group lose their outlines; only the paint after the merge draws.
    let all = draws(&format!(
        r##"{{"group":[{{"rect":[100,100]}},{{"fill":"#FF0000"}}]}},{{"rect":[100,100],"position":[50,50]}},{{"stroke":"#00FF00","width":2}},{{"merge":"add"}},{FILL}"##
    ));
    assert_eq!(all.len(), 1, "{all:?}");
    near(all[0].outline.area().abs(), 17_500.0);
}

#[test]
fn a_merge_sees_inner_groups_through_their_transforms() {
    let path = one(&format!(
        r#"{{"group":[{{"rect":[100,100]}}],"position":[50,50]}},{{"rect":[100,100]}},{{"merge":"intersect"}},{FILL}"#
    ));
    near(path.area().abs(), 2_500.0);
    let bounds = path.bounding_box();
    near(bounds.x0, 0.0);
    near(bounds.y0, 0.0);
}

#[test]
fn a_merged_outline_is_one_outline_for_the_groups_around_it() {
    // The inner merge leaves one outline, which the outer one subtracts
    // from the square before it.
    let path = one(&format!(
        r#"{{"rect":[200,200]}},{{"group":[{SQUARES},{{"merge":"add"}}]}},{{"merge":"subtract"}},{FILL}"#
    ));
    near(path.area().abs(), 40_000.0 - 17_500.0);
}

#[test]
fn a_trim_after_a_merge_cuts_the_merged_outline() {
    let whole = one(&format!(
        r##"{SQUARES},{{"merge":"add"}},{{"stroke":"#FFFFFF","width":2}}"##
    ));
    let cut = one(&format!(
        r##"{SQUARES},{{"merge":"add"}},{{"trim":0.5}},{{"stroke":"#FFFFFF","width":2}}"##
    ));
    let length = |p: &BezPath| {
        p.segments()
            .map(|s| kurbo::ParamCurveArclen::arclen(&s, 1e-6))
            .sum::<f64>()
    };
    // The union's outline is 600 px around.
    near(length(&whole), 600.0);
    near(length(&cut), 300.0);
}

#[test]
fn a_merge_with_nothing_above_it_does_nothing() {
    let all = draws(&format!(r#"{{"merge":"add"}},{SQUARES},{FILL}"#));
    assert_eq!(all.len(), 1);
    assert_eq!(subpaths(&all[0].outline), 2);
}
