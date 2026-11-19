use super::*;
use crate::FontSet;

#[test]
fn inter_is_a_sans_of_normal_width() {
    let set = FontSet::builtin();
    let face = set.named("InterVariable").expect("Inter ships built in");
    let metrics = face.metrics();
    assert_eq!(metrics.class, Class::Sans);
    assert_eq!(metrics.stretch, 5);
    // Inter's x-height is about 0.55 em, its average advance about 0.6.
    assert!((0.5..0.6).contains(&metrics.x_height), "{metrics:?}");
    assert!((0.45..0.7).contains(&metrics.width), "{metrics:?}");
}

#[test]
fn heavier_weights_draw_thicker_strokes() {
    let set = FontSet::builtin();
    let stroke = |name: &str| {
        set.named(name)
            .unwrap_or_else(|| panic!("{name} ships built in"))
            .metrics()
            .stroke
    };
    let (thin, regular, bold, black) = (
        stroke("InterVariable-Thin"),
        stroke("InterVariable"),
        stroke("InterVariable-Bold"),
        stroke("InterVariable-Black"),
    );
    assert!(
        thin < regular && regular < bold && bold < black,
        "{thin} {regular} {bold} {black}"
    );
    // Inter's regular stems are about 0.08 em, its black ones about twice.
    assert!((0.05..0.11).contains(&regular), "{regular}");
    assert!(black > 1.7 * regular, "{regular} {black}");
}

#[test]
fn the_pen_counts_holes_against_the_ink() {
    let mut ink = Ink::default();
    // A unit square, then a hole half its side, wound the other way.
    ink.move_to(0.0, 0.0);
    for (x, y) in [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        ink.line_to(x, y);
    }
    ink.close();
    ink.move_to(0.25, 0.25);
    for (x, y) in [(0.25, 0.75), (0.75, 0.75), (0.75, 0.25)] {
        ink.line_to(x, y);
    }
    ink.close();
    assert!((ink.area - 0.75).abs() < 1e-9, "{}", ink.area);
    assert!((ink.length - 6.0).abs() < 1e-9, "{}", ink.length);
}
