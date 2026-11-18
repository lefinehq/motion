use std::cell::Cell;

use super::*;
use crate::effect::Effect;
use crate::list::Layer;

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

/// Reads `node` at `transform`, counting the drawings in `drawn`.
fn read(reads: &Reads, node: &Arc<Node>, transform: Affine, drawn: &Cell<usize>) -> Pixels {
    reads
        .read(node, transform, (2, 2), || {
            drawn.set(drawn.get() + 1);
            Ok(Pixels::transparent(2, 2))
        })
        .expect("drawn")
}

#[test]
fn a_node_read_three_times_is_drawn_once_and_let_go_after_its_last_read() {
    let node = Arc::new(Node::Layer(Layer::new(Vec::new())));
    let reads = Reads::of(&[reading(&node, 3)]);
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
    let node = Arc::new(Node::Layer(Layer::new(Vec::new())));
    let reads = Reads::of(&[reading(&node, 1)]);
    let drawn = Cell::new(0);
    read(&reads, &node, Affine::IDENTITY, &drawn);
    assert_eq!(drawn.get(), 1);
    assert!(locked(&reads.drawn).is_empty());
}

#[test]
fn each_transform_a_node_is_read_at_is_drawn_once() {
    let node = Arc::new(Node::Layer(Layer::new(Vec::new())));
    let reads = Reads::of(&[reading(&node, 4)]);
    let drawn = Cell::new(0);
    let moved = Affine::translate((1.0, 0.0));
    for transform in [Affine::IDENTITY, moved, Affine::IDENTITY, moved] {
        read(&reads, &node, transform, &drawn);
    }
    assert_eq!(drawn.get(), 2);
    assert!(locked(&reads.drawn).is_empty());
}

#[test]
fn reads_inside_a_read_node_are_counted_once() {
    let inner = Arc::new(Node::Layer(Layer::new(Vec::new())));
    let outer = Arc::new(reading(&inner, 2));
    let reads = Reads::of(&[reading(&outer, 2)]);
    let left = locked(&reads.left);
    assert_eq!(left.get(&(Arc::as_ptr(&outer) as usize)), Some(&2));
    assert_eq!(left.get(&(Arc::as_ptr(&inner) as usize)), Some(&2));
}

#[test]
fn children_shared_by_three_layers_are_drawn_once_at_each_transform() {
    let shared = Shared::new();
    let layer = || {
        let mut layer = Layer::new(Vec::new());
        layer.shared = Some(shared.clone());
        Node::Layer(layer)
    };
    let reads = Reads::of(&[layer(), layer(), layer()]);
    let drawn = Cell::new(0);
    let draw = || {
        drawn.set(drawn.get() + 1);
        Ok(Pixels::transparent(2, 2))
    };
    for _ in 0..3 {
        reads
            .shared(&shared, Affine::IDENTITY, (2, 2), draw)
            .expect("drawn");
    }
    assert_eq!(drawn.get(), 1);
    assert!(locked(&reads.drawn).is_empty());
}
