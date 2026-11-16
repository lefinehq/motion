//! Unit tests for `src/boolean.rs`: the areas each operation keeps, how an
//! outline is read, and curves kept as curves.

use super::*;
use kurbo::{Circle, Rect};

/// The area a result covers: its contours' signed areas, outer and inner
/// ones of opposite signs, summed.
fn area(path: &BezPath) -> f64 {
    path.area().abs()
}

fn square(x: f64, y: f64, side: f64) -> BezPath {
    Rect::new(x, y, x + side, y + side).to_path(0.1)
}

fn near(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() <= tolerance, "{a} vs {b}");
}

#[test]
fn each_operation_keeps_its_share_of_two_overlapping_squares() {
    // Two 100 px squares overlapping in a 50 by 50 corner.
    let (a, b) = (square(0.0, 0.0, 100.0), square(50.0, 50.0, 100.0));
    let of = |op| area(&combine(&a, &b, op));
    near(of(Op::Union), 17_500.0, 0.5);
    near(of(Op::Difference), 7_500.0, 0.5);
    near(of(Op::Intersection), 2_500.0, 0.5);
    near(of(Op::Xor), 15_000.0, 0.5);
}

#[test]
fn a_result_is_simple_so_either_fill_rule_reads_it_alike() {
    let ring = combine(
        &square(0.0, 0.0, 100.0),
        &square(25.0, 25.0, 50.0),
        Op::Difference,
    );
    near(area(&ring), 7_500.0, 0.5);
    // Even-odd and nonzero agree when no point is wound around twice: the
    // ring read again covers what it covered.
    near(area(&region(&ring)), 7_500.0, 0.5);
    assert_eq!(
        ring.elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count(),
        2
    );
}

#[test]
fn an_outline_covers_what_it_winds_around() {
    // Two nested squares running the same way wind twice around the inner
    // one, which stays covered; running opposite ways, they leave a hole.
    let mut same = square(0.0, 0.0, 100.0);
    same.extend(&square(25.0, 25.0, 50.0));
    near(area(&region(&same)), 10_000.0, 0.5);
    let mut opposite = square(0.0, 0.0, 100.0);
    opposite.extend(&square(25.0, 25.0, 50.0).reverse_subpaths());
    near(area(&region(&opposite)), 7_500.0, 0.5);
}

#[test]
fn curves_stay_curves() {
    let a = Circle::new((0.0, 0.0), 50.0).to_path(0.01);
    let b = Circle::new((50.0, 0.0), 50.0).to_path(0.01);
    let union = combine(&a, &b, Op::Union);
    assert!(
        union
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::CurveTo(..)))
    );
    // The union and the lens between them add up to the two circles.
    let lens = combine(&a, &b, Op::Intersection);
    near(area(&union) + area(&lens), area(&a) + area(&b), 0.01);
    // Two circles of radius r, centres r apart, overlap in r²(2π/3 − √3/2),
    // to within what cubics that follow a circle miss of it.
    let r2 = 2_500.0;
    near(
        area(&lens),
        r2 * (2.0 * std::f64::consts::PI / 3.0 - 3f64.sqrt() / 2.0),
        1.0,
    );
}

#[test]
fn an_open_outline_is_closed_by_a_straight_line() {
    // Three sides of a square: the area of the whole square.
    let mut open = BezPath::new();
    open.move_to((0.0, 0.0));
    open.line_to((100.0, 0.0));
    open.line_to((100.0, 100.0));
    open.line_to((0.0, 100.0));
    near(area(&region(&open)), 10_000.0, 0.5);
}

#[test]
fn nothing_combines_into_nothing_and_a_lone_point_covers_nothing() {
    assert!(region(&BezPath::new()).elements().is_empty());
    let mut point = BezPath::new();
    point.move_to((5.0, 5.0));
    assert!(region(&point).elements().is_empty());
    let b = square(0.0, 0.0, 10.0);
    let kept = combine(&point, &b, Op::Union);
    near(area(&kept), 100.0, 1e-6);
}

#[test]
fn an_outline_that_is_not_finite_covers_nothing() {
    let mut broken = square(0.0, 0.0, 10.0);
    broken.line_to((f64::NAN, 0.0));
    assert!(region(&broken).elements().is_empty());
    let b = square(0.0, 0.0, 10.0);
    near(area(&combine(&b, &broken, Op::Difference)), 100.0, 1e-6);
}
