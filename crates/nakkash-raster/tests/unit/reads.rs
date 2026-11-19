use std::cell::Cell;

use super::*;
use crate::effect::{Effect, ParamValue};
use crate::list::{Layer, Matte, MatteMode, PathDraw};
use crate::pixels::Pixels;
use crate::style::{Color, DrawStyle, FillRule, Paint};
use vello_cpu::kurbo::{Rect, Shape};

/// A layer drawing a square of `side` px in `color`.
fn square(side: f64, color: Color) -> Node {
    Node::Layer(Layer::new(vec![Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, side, side).to_path(0.1),
        paint: Paint::Solid(color),
        style: DrawStyle::Fill(FillRule::NonZero),
    })]))
}

/// A layer whose effects read `read`, once each.
fn reading(read: &Arc<Node>, effects: usize) -> Node {
    let mut layer = Layer::new(Vec::new());
    layer.effects = (0..effects)
        .map(|_| {
            Effect::new("displacement-map", &[])
                .expect("a known effect")
                .with_input(Arc::clone(read))
        })
        .collect();
    Node::Layer(layer)
}

/// Draws `node` alone at `transform` through `reads`, counting the
/// drawings in `drawn`.
fn read(reads: &Reads, node: &Node, transform: Affine, drawn: &Cell<usize>) -> Pixels {
    reads
        .drawn(std::slice::from_ref(node), transform, (2, 2), None, || {
            drawn.set(drawn.get() + 1);
            Ok(Surface::Cpu(Pixels::transparent(2, 2)))
        })
        .and_then(|surface| surface.into_pixels(None))
        .expect("drawn")
}

#[test]
fn a_node_read_three_times_is_drawn_once_and_let_go_after_its_last_read() {
    let node = Arc::new(square(1.0, Color::WHITE));
    let reads = Reads::of(&[reading(&node, 3)], true);
    let drawn = Cell::new(0);
    for _ in 0..3 {
        assert_eq!(
            read(&reads, &node, Affine::IDENTITY, &drawn),
            Pixels::transparent(2, 2)
        );
    }
    assert_eq!(drawn.get(), 1);
    assert!(locked(&reads.drawn).is_empty());
    assert!(locked(&reads.left).is_empty());
}

#[test]
fn a_node_read_once_is_not_kept() {
    let node = Arc::new(square(1.0, Color::WHITE));
    let reads = Reads::of(&[reading(&node, 1)], true);
    let drawn = Cell::new(0);
    read(&reads, &node, Affine::IDENTITY, &drawn);
    assert_eq!(drawn.get(), 1);
    assert!(locked(&reads.drawn).is_empty());
}

#[test]
fn each_transform_a_node_is_read_at_is_drawn_once() {
    let node = Arc::new(square(1.0, Color::WHITE));
    let reads = Reads::of(&[reading(&node, 4)], true);
    let drawn = Cell::new(0);
    let moved = Affine::translate((1.0, 0.0));
    for transform in [Affine::IDENTITY, moved, Affine::IDENTITY, moved] {
        read(&reads, &node, transform, &drawn);
    }
    assert_eq!(drawn.get(), 2);
    assert!(locked(&reads.drawn).is_empty());
}

#[test]
fn equal_nodes_in_other_places_are_drawn_once() {
    // Two inputs that are different nodes, made apart, with the same
    // content: one identity, one drawing.
    let one = Arc::new(square(1.0, Color::WHITE));
    let other = Arc::new(square(1.0, Color::WHITE));
    assert!(!Arc::ptr_eq(&one, &other));
    let reads = Reads::of(&[reading(&one, 1), reading(&other, 1)], true);
    let drawn = Cell::new(0);
    read(&reads, &one, Affine::IDENTITY, &drawn);
    read(&reads, &other, Affine::IDENTITY, &drawn);
    assert_eq!(drawn.get(), 1);
    assert!(locked(&reads.drawn).is_empty());
}

#[test]
fn different_nodes_are_drawn_apart_wherever_they_were() {
    // A node let go and another made where it was: identities never come
    // from addresses, so the new node is drawn, never the old drawing.
    let drawn = Cell::new(0);
    let first = Arc::new(square(1.0, Color::WHITE));
    let reads = Reads::of(&[reading(&first, 2)], true);
    read(&reads, &first, Affine::IDENTITY, &drawn);
    drop(reads);
    drop(first);
    for side in [2.0, 3.0, 4.0] {
        let next = Arc::new(square(side, Color::WHITE));
        let reads = Reads::of(&[reading(&next, 2), reading(&next, 0)], true);
        let before = drawn.get();
        read(&reads, &next, Affine::IDENTITY, &drawn);
        assert_eq!(drawn.get(), before + 1, "a new node is drawn");
    }
    // In one frame, two nodes that differ in one value are two drawings.
    let red = Arc::new(square(1.0, Color::rgba(1.0, 0.0, 0.0, 1.0)));
    let white = Arc::new(square(1.0, Color::WHITE));
    let reads = Reads::of(&[reading(&red, 2), reading(&white, 2)], true);
    let before = drawn.get();
    for node in [&red, &white, &red, &white] {
        read(&reads, node, Affine::IDENTITY, &drawn);
    }
    assert_eq!(drawn.get(), before + 2);
}

#[test]
fn reads_inside_a_read_node_are_counted_once() {
    let inner = Arc::new(square(1.0, Color::WHITE));
    let outer = Arc::new(reading(&inner, 2));
    let reads = Reads::of(&[reading(&outer, 2)], true);
    let mut ids = Identities::default();
    let left = locked(&reads.left);
    assert_eq!(left.get(&ids.list(std::slice::from_ref(&*outer))), Some(&2));
    assert_eq!(left.get(&ids.list(std::slice::from_ref(&*inner))), Some(&2));
}

#[test]
fn a_matte_source_shared_by_layers_is_one_use_each() {
    let source = || square(3.0, Color::WHITE);
    let matted = || {
        let mut layer = Layer::new(vec![square(1.0, Color::WHITE)]);
        layer.matte = Some(Matte {
            source: Box::new(source()),
            mode: MatteMode::Alpha,
        });
        Node::Layer(layer)
    };
    let reads = Reads::of(&[matted(), matted(), matted()], true);
    let mut ids = Identities::default();
    let id = ids.list(&[source()]);
    assert_eq!(locked(&reads.left).get(&id), Some(&3));
}

#[test]
fn children_of_layers_drawn_alone_are_counted_by_what_they_draw() {
    // Three layers with the same children, not marked as sharing them,
    // each working its content as pixels.
    let layer = || {
        let mut layer = Layer::new(vec![square(2.0, Color::WHITE)]);
        layer.effects = vec![
            Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(2.0))])
                .expect("a known effect"),
        ];
        Node::Layer(layer)
    };
    let nodes = [layer(), layer(), layer()];
    let Node::Layer(first) = &nodes[0] else {
        unreachable!("a layer")
    };
    assert!(crate::render::draws_children_alone(first, true));
    let reads = Reads::of(&nodes, true);
    let mut ids = Identities::default();
    assert_eq!(
        locked(&reads.left).get(&ids.list(&first.children)),
        Some(&3)
    );
}
