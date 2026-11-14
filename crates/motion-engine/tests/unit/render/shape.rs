//! Unit tests for `src/render/shape.rs`.

use motion_format::Result;
use motion_format::Role;
use motion_format::item::SHAPES;
use motion_raster::Layer;
use motion_raster::kurbo::{Affine, Shape};
use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::render::tests::{list, only_layer, scene};

/// Reads a setting's choice as the rasteriser's name for it.
type Reader = fn(&str) -> Result<()>;

/// Every scene setting a paint has, and how the rasteriser reads its choices.
const SETTINGS: [(&str, Reader); 3] = [
    ("rule", |name| FillRule::named(name).map(drop)),
    ("cap", |name| Cap::named(name).map(drop)),
    ("join", |name| Join::named(name).map(drop)),
];

#[test]
fn every_paint_kind_has_a_row() {
    for kind in SHAPES.iter().filter(|kind| kind.role == Role::Paint) {
        assert!(
            PAINTS.iter().any(|(name, _)| *name == kind.name),
            "no row for paint `{}`",
            kind.name
        );
    }
}

#[test]
fn every_paint_setting_choice_is_a_rasteriser_name() {
    for kind in SHAPES.iter().filter(|kind| kind.role == Role::Paint) {
        for setting in kind.settings {
            let (_, read) = SETTINGS
                .iter()
                .find(|(name, _)| *name == setting.name)
                .unwrap_or_else(|| panic!("no reader for setting `{}`", setting.name));
            for choice in setting.choices {
                read(choice).unwrap_or_else(|e| panic!("`{}`: {e}", setting.name));
            }
        }
    }
}

/// The leaves of the one shape layer `shapes` draws, each with the transform
/// it is drawn under.
fn leaves(shapes: &str) -> Vec<(Affine, PathDraw)> {
    let frame = list(&scene(&format!(r#"{{"id":"a","shapes":[{shapes}]}}"#)), 0.0);
    only_layer(&frame)
        .children
        .iter()
        .map(|node| match node {
            Node::Path(path) => (Affine::IDENTITY, path.clone()),
            Node::Layer(Layer {
                transform,
                children,
                ..
            }) => match children.as_slice() {
                [Node::Path(path)] => (*transform, path.clone()),
                other => panic!("not one path: {other:?}"),
            },
            Node::Glyphs(_) | Node::Image(_) => panic!("not an outline: {node:?}"),
        })
        .collect()
}

#[test]
fn a_fill_takes_its_rule_and_folds_its_and_its_groups_opacity() {
    let [(transform, fill)] = &leaves(
        r##"{"group":[{"rect":[10,10]},{"fill":"#FF000080","opacity":0.5,"rule":"evenodd"}],"opacity":0.5}"##,
    )[..] else {
        panic!("one fill")
    };
    assert_eq!(*transform, Affine::IDENTITY);
    assert_eq!(fill.style, DrawStyle::Fill(FillRule::EvenOdd));
    let Paint::Solid(color) = &fill.paint;
    assert_eq!(color.a, 128.0 / 255.0 * 0.25);
}

#[test]
fn a_stroke_takes_its_width_cap_join_and_miter() {
    let [(_, stroke)] = &leaves(
        r##"{"path":"M0,0 L10,0 L10,10"},{"stroke":"#000000","width":3,"cap":"round","join":"bevel","miter":7}"##,
    )[..] else {
        panic!("one stroke")
    };
    assert_eq!(
        stroke.style,
        DrawStyle::Stroke(StrokeStyle {
            cap: Cap::Round,
            join: Join::Bevel,
            miter_limit: 7.0,
            ..StrokeStyle::new(3.0)
        })
    );
}

#[test]
fn a_stroke_under_a_squash_keeps_the_transform_as_a_layer() {
    let [(transform, stroke)] = &leaves(
        r##"{"group":[{"rect":[10,10]},{"stroke":"#000000","width":2}],"scale":[2,1]}"##,
    )[..] else {
        panic!("one stroke")
    };
    // The outline stays in the group's space; the layer squashes the stroke with it.
    assert_eq!(transform.as_coeffs()[0], 2.0);
    assert_eq!(stroke.path.bounding_box().width(), 10.0);
}

#[rstest]
#[case::a_clear_fill(r##"{"rect":[10,10]},{"fill":"#FF000000"}"##)]
#[case::a_transparent_fill(r##"{"rect":[10,10]},{"fill":"#FF0000","opacity":0}"##)]
#[case::a_stroke_of_no_width(r##"{"rect":[10,10]},{"stroke":"#000000","width":0}"##)]
fn a_paint_that_leaves_no_mark_draws_nothing(#[case] shapes: &str) {
    let frame = list(&scene(&format!(r#"{{"id":"a","shapes":[{shapes}]}}"#)), 0.0);
    assert!(frame.nodes.is_empty(), "{:?}", frame.nodes);
}

#[test]
fn draws_keep_their_order_back_to_front() {
    let colors: Vec<Paint> =
        leaves(r##"{"rect":[10,10]},{"fill":"#FF0000"},{"rect":[20,20]},{"fill":"#0000FF"}"##)
            .into_iter()
            .map(|(_, path)| path.paint)
            .collect();
    // The first item is on top, so it is drawn last.
    assert_eq!(
        colors,
        [
            Paint::Solid(motion_raster::Color::rgba(0.0, 0.0, 1.0, 1.0)),
            Paint::Solid(motion_raster::Color::rgba(1.0, 0.0, 0.0, 1.0)),
        ]
    );
}
