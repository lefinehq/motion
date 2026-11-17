//! Unit tests for `src/canvas.rs`: what the drawing model turns paints,
//! clips and groups into.

use nakkash_raster::kurbo::{Affine, BezPath, Line, Shape};
use nakkash_raster::{BlendMode, Color, MatteMode, Node, Paint, StrokeStyle};

use super::*;

fn line() -> BezPath {
    Line::new((0.0, 0.0), (10.0, 0.0)).to_path(0.1)
}

fn red() -> Paint {
    Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0))
}

/// The only node `canvas` painted.
fn only(canvas: Canvas) -> Node {
    let (mut nodes, _) = canvas.finish();
    assert_eq!(nodes.len(), 1, "{nodes:?}");
    nodes.remove(0)
}

#[test]
fn a_fill_is_moved_into_the_drawings_space() {
    let mut canvas = Canvas::new();
    let square = nakkash_raster::kurbo::Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1);
    canvas.fill(
        &square,
        Affine::translate((5.0, 6.0)),
        red(),
        FillRule::EvenOdd,
    );
    let Node::Path(path) = only(canvas) else {
        panic!("a path");
    };
    assert_eq!(
        path.path.bounding_box(),
        nakkash_raster::kurbo::Rect::new(5.0, 6.0, 6.0, 7.0)
    );
    assert_eq!(path.style, DrawStyle::Fill(FillRule::EvenOdd));
}

#[test]
fn an_even_scale_scales_the_line_and_its_dashes() {
    let mut canvas = Canvas::new();
    let mut style = StrokeStyle::new(2.0);
    style.dashes = Some(nakkash_raster::Dashes {
        offset: 1.0,
        pattern: vec![3.0, 1.0],
    });
    canvas.stroke(
        &line(),
        Affine::rotate(1.0) * Affine::scale(3.0),
        red(),
        &style,
    );
    let Node::Path(path) = only(canvas) else {
        panic!("a path, moved");
    };
    let DrawStyle::Stroke(stroke) = path.style else {
        panic!("a stroke");
    };
    assert!((stroke.width - 6.0).abs() < 1e-9);
    let dashes = stroke.dashes.expect("dashes");
    assert!((dashes.offset - 3.0).abs() < 1e-9);
    assert!((dashes.pattern[0] - 9.0).abs() < 1e-9);
}

#[test]
fn an_uneven_scale_strokes_where_the_line_is_given() {
    let mut canvas = Canvas::new();
    canvas.stroke(
        &line(),
        Affine::scale_non_uniform(1.0, 4.0),
        red(),
        &StrokeStyle::new(2.0),
    );
    let Node::Layer(layer) = only(canvas) else {
        panic!("a layer");
    };
    assert_eq!(layer.transform, Affine::scale_non_uniform(1.0, 4.0));
    let [Node::Path(path)] = layer.children.as_slice() else {
        panic!("one path");
    };
    assert_eq!(path.style, DrawStyle::Stroke(StrokeStyle::new(2.0)));
}

#[test]
fn a_clip_holds_what_was_painted_while_it_was_open() {
    let mut canvas = Canvas::new();
    let outline = nakkash_raster::kurbo::Rect::new(0.0, 0.0, 5.0, 5.0).to_path(0.1);
    canvas.push_clip(outline.clone(), FillRule::NonZero);
    assert_eq!(canvas.depth(), 1);
    canvas.fill(&line(), Affine::IDENTITY, red(), FillRule::NonZero);
    assert!(canvas.pop());
    assert!(!canvas.pop(), "the drawing itself does not close");
    let Node::Layer(layer) = only(canvas) else {
        panic!("a layer");
    };
    assert_eq!(layer.clip, Some(Clip::Path(outline, FillRule::NonZero)));
    assert_eq!(layer.children.len(), 1);
}

#[test]
fn an_empty_clip_or_group_leaves_nothing() {
    let mut canvas = Canvas::new();
    canvas.push_group(Group::new());
    canvas.push_clip(line(), FillRule::NonZero);
    let (nodes, _) = canvas.finish();
    assert!(nodes.is_empty());
}

#[test]
fn a_group_that_changes_nothing_is_not_isolated() {
    let mut canvas = Canvas::new();
    canvas.push_group(Group::new());
    canvas.fill(&line(), Affine::IDENTITY, red(), FillRule::NonZero);
    let Node::Layer(layer) = only(canvas) else {
        panic!("a layer");
    };
    assert!(!layer.is_isolated());
}

#[test]
fn a_group_with_opacity_a_blend_or_a_mask_is_isolated() {
    let mut canvas = Canvas::new();
    let mask = Matte {
        source: Box::new(Node::Layer(Layer::new(Vec::new()))),
        mode: MatteMode::Luma,
    };
    canvas.push_group(Group {
        opacity: 0.5,
        blend: BlendMode::Multiply,
        mask: Some(mask.clone()),
    });
    canvas.fill(&line(), Affine::IDENTITY, red(), FillRule::NonZero);
    let Node::Layer(layer) = only(canvas) else {
        panic!("a layer");
    };
    assert!(layer.isolate);
    assert_eq!((layer.opacity, layer.blend), (0.5, BlendMode::Multiply));
    assert_eq!(layer.matte, Some(mask));
}

#[test]
fn a_plain_group_holding_a_blend_stays_isolated() {
    let mut canvas = Canvas::new();
    canvas.push_group(Group::new());
    canvas.push_group(Group {
        blend: BlendMode::Screen,
        ..Group::new()
    });
    canvas.fill(&line(), Affine::IDENTITY, red(), FillRule::NonZero);
    let Node::Layer(outer) = only(canvas) else {
        panic!("a layer");
    };
    assert!(outer.isolate, "its screen sees only the group");
}

#[test]
fn what_cannot_be_drawn_is_noted_once_and_sorted() {
    let mut canvas = Canvas::new();
    canvas.unsupported("b");
    canvas.unsupported("a");
    canvas.unsupported("b");
    assert_eq!(canvas.finish().1, ["a", "b"]);
}

#[test]
fn a_picture_fills_its_pixels_rectangle_mapped_by_its_transform() {
    let image = nakkash_raster::Image::new(2, 1, [255, 0, 0, 255].repeat(2)).unwrap();
    let picture = Picture::from_image(&image).unwrap();
    let mut canvas = Canvas::new();
    canvas.picture(picture, Affine::scale(5.0));
    let Node::Layer(layer) = only(canvas) else {
        panic!("a layer");
    };
    assert_eq!(layer.transform, Affine::scale(5.0));
    let [Node::Image(draw)] = layer.children.as_slice() else {
        panic!("one picture");
    };
    assert_eq!(
        draw.rect,
        nakkash_raster::kurbo::Rect::new(0.0, 0.0, 2.0, 1.0)
    );
}
