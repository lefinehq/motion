//! Unit tests for `src/style.rs`.

use super::*;
use nakkash_format::Kind;

#[test]
fn every_table_maps_each_variant_once_and_names_round_trip() {
    for (name, rule, _) in FILL_RULES {
        assert_eq!(FillRule::named(name).unwrap(), *rule);
        assert_eq!(rule.name(), *name);
    }
    for (name, cap, _) in CAPS {
        assert_eq!(Cap::named(name).unwrap(), *cap);
        assert_eq!(cap.name(), *name);
    }
    for (name, join, _) in JOINS {
        assert_eq!(Join::named(name).unwrap(), *join);
        assert_eq!(join.name(), *name);
    }
    assert_eq!(FILL_RULES.len(), 2);
    assert_eq!(CAPS.len(), 3);
    assert_eq!(JOINS.len(), 3);
}

#[test]
fn an_unknown_name_lists_the_table() {
    let e = Cap::named("pointy").unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(e.expected.as_deref(), Some("one of butt, round, square"));
    assert_eq!(Join::named("x").unwrap_err().kind, Kind::UnknownName);
    assert_eq!(FillRule::named("x").unwrap_err().kind, Kind::UnknownName);
}

#[test]
fn each_variant_maps_to_its_vello_value() {
    assert_eq!(FillRule::EvenOdd.to_vello(), peniko::Fill::EvenOdd);
    assert_eq!(Cap::Square.to_vello(), kurbo::Cap::Square);
    assert_eq!(Join::Bevel.to_vello(), kurbo::Join::Bevel);
}

#[test]
fn a_stroke_style_carries_everything_to_vello() {
    let style = StrokeStyle {
        cap: Cap::Round,
        join: Join::Round,
        miter_limit: 2.0,
        dashes: Some(Dashes {
            offset: 1.5,
            pattern: vec![4.0, 2.0],
        }),
        ..StrokeStyle::new(6.0)
    };
    let stroke = style.to_vello();
    assert_eq!(stroke.width, 6.0);
    assert_eq!(
        (stroke.start_cap, stroke.end_cap),
        (kurbo::Cap::Round, kurbo::Cap::Round)
    );
    assert_eq!(stroke.join, kurbo::Join::Round);
    assert_eq!(stroke.miter_limit, 2.0);
    assert_eq!(stroke.dash_offset, 1.5);
    assert_eq!(stroke.dash_pattern.as_slice(), [4.0, 2.0]);
    let plain = StrokeStyle::new(1.0).to_vello();
    assert!(plain.dash_pattern.is_empty());
    assert_eq!((plain.join, plain.miter_limit), (kurbo::Join::Miter, 4.0));
}

#[test]
fn colours_keep_straight_alpha_and_scale_it() {
    let half = Color::rgba(1.0, 0.5, 0.25, 1.0).with_opacity(0.5);
    assert_eq!(half, Color::rgba(1.0, 0.5, 0.25, 0.5));
    assert_eq!(half.to_vello().components, [1.0, 0.5, 0.25, 0.5]);
    let (solid, placed) = Paint::Solid(Color::BLACK).to_vello();
    let vello_cpu::PaintType::Solid(solid) = solid else {
        panic!("a colour");
    };
    assert_eq!(solid.components, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(placed, kurbo::Affine::IDENTITY);
    assert_eq!(Color::TRANSPARENT.a, 0.0);
    assert_eq!(Color::WHITE.r, 1.0);
}

#[test]
fn a_gradient_keeps_its_stops_and_is_placed_by_its_transform() {
    let gradient = Gradient {
        shape: GradientShape::Radial {
            start: kurbo::Point::new(1.0, 2.0),
            start_radius: 0.0,
            end: kurbo::Point::new(1.0, 2.0),
            end_radius: 5.0,
        },
        stops: vec![
            Stop {
                offset: 0.0,
                color: Color::WHITE,
            },
            Stop {
                offset: 1.0,
                color: Color::BLACK,
            },
        ],
        transform: kurbo::Affine::scale(3.0),
    };
    let (paint, placed) = Paint::Gradient(Box::new(gradient)).to_vello();
    assert_eq!(placed, kurbo::Affine::scale(3.0));
    let vello_cpu::PaintType::Gradient(gradient) = paint else {
        panic!("a gradient");
    };
    assert_eq!(gradient.stops.len(), 2);
    assert_eq!(gradient.extend, peniko::Extend::Pad);
}

#[test]
fn a_linear_gradient_is_drawn_along_its_line() {
    use crate::{Frame, Node, PathDraw, RenderOptions, render_with};
    let square = kurbo::Rect::new(0.0, 0.0, 10.0, 2.0);
    let paint = Paint::Gradient(Box::new(Gradient {
        shape: GradientShape::Linear {
            start: kurbo::Point::new(0.0, 0.0),
            end: kurbo::Point::new(1.0, 0.0),
        },
        stops: vec![
            Stop {
                offset: 0.0,
                color: Color::rgba(1.0, 0.0, 0.0, 1.0),
            },
            Stop {
                offset: 1.0,
                color: Color::rgba(0.0, 0.0, 1.0, 1.0),
            },
        ],
        // The unit line stretched over the square's width.
        transform: kurbo::Affine::scale(10.0),
    }));
    let frame = Frame {
        nodes: vec![Node::Path(PathDraw {
            path: kurbo::Shape::to_path(&square, 0.1),
            paint,
            style: DrawStyle::Fill(FillRule::NonZero),
        })],
        ..Frame::default()
    };
    let image = render_with(&frame, 10, 2, RenderOptions::GOLDEN).unwrap();
    let [r, _, b, _] = image.pixel(0, 0).unwrap();
    assert!(r > 220 && b < 35, "{r} {b}");
    let [r, _, b, _] = image.pixel(9, 1).unwrap();
    assert!(r < 35 && b > 220, "{r} {b}");
}
