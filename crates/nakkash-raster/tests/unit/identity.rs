use std::sync::Arc;

use super::*;
use crate::blend::BlendMode;
use crate::effect::{Effect, ParamValue};
use crate::list::{MaskMode, MatteMode};
use crate::picture::Picture;
use crate::style::FillRule;
use vello_cpu::kurbo::Shape;

fn square(color: Color) -> Node {
    Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, 4.0, 4.0).to_path(0.1),
        paint: Paint::Solid(color),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

fn id(node: &Node) -> Id {
    Identities::default().node(node)
}

/// A layer with something in every field.
fn full() -> Layer {
    let mut layer = Layer::new(vec![square(Color::WHITE)]);
    layer.transform = Affine::translate((3.0, 4.0));
    layer.opacity = 0.5;
    layer.blend = BlendMode::Multiply;
    layer.clip = Some(Clip::Rect(Rect::new(0.0, 0.0, 9.0, 9.0)));
    layer.masks = vec![LayerMask {
        path: Rect::new(1.0, 1.0, 3.0, 3.0).to_path(0.1),
        mode: MaskMode::Add,
        inverted: false,
        opacity: 1.0,
        feather: [0.0, 0.0],
        expand: 0.0,
    }];
    layer.effects = vec![
        Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(2.0))])
            .expect("known")
            .with_input(Arc::new(square(Color::WHITE))),
    ];
    layer.matte = Some(Matte {
        source: Box::new(square(Color::WHITE)),
        mode: MatteMode::Alpha,
    });
    layer.bounds = Some(Rect::new(0.0, 0.0, 4.0, 4.0));
    layer.time = 1.5;
    layer.motion = vec![Affine::IDENTITY];
    layer.projection = Some(Projection::new(Homography::IDENTITY));
    layer
}

#[test]
fn equal_nodes_have_one_identity_wherever_they_are() {
    let a = Node::Layer(full());
    let b = Node::Layer(full());
    assert_eq!(id(&a), id(&b));
    // A copy, in another place in memory, too.
    let boxed = Box::new(a.clone());
    assert_eq!(id(&boxed), id(&a));
    // Identities do not depend on the run: a fixed key.
    assert_eq!(id(&a), id(&Node::Layer(full())));
}

#[test]
fn a_change_to_any_field_changes_the_identity() {
    let base = id(&Node::Layer(full()));
    type Change = Box<dyn Fn(&mut Layer)>;
    let changes: Vec<(&str, Change)> = vec![
        (
            "transform",
            Box::new(|l| l.transform = Affine::translate((3.0, 5.0))),
        ),
        ("opacity", Box::new(|l| l.opacity = 0.25)),
        ("blend", Box::new(|l| l.blend = BlendMode::Screen)),
        ("clip", Box::new(|l| l.clip = None)),
        ("mask", Box::new(|l| l.masks[0].inverted = true)),
        (
            "mask feather",
            Box::new(|l| l.masks[0].feather = [0.0, 1.0]),
        ),
        (
            "effect value",
            Box::new(|l| {
                l.effects[0] =
                    Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(3.0))])
                        .expect("known")
                        .with_input(Arc::new(square(Color::WHITE)));
            }),
        ),
        (
            "effect input",
            Box::new(|l| {
                l.effects[0] =
                    Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(2.0))])
                        .expect("known")
                        .with_input(Arc::new(square(Color::rgba(1.0, 0.0, 0.0, 1.0))));
            }),
        ),
        (
            "children",
            Box::new(|l| l.children.push(square(Color::WHITE))),
        ),
        ("isolate", Box::new(|l| l.isolate = true)),
        (
            "matte mode",
            Box::new(|l| l.matte.as_mut().expect("matte").mode = MatteMode::Luma),
        ),
        (
            "matte source",
            Box::new(|l| {
                *l.matte.as_mut().expect("matte").source = square(Color::rgba(0.0, 0.0, 0.0, 1.0));
            }),
        ),
        ("adjust", Box::new(|l| l.adjust = true)),
        ("bounds", Box::new(|l| l.bounds = None)),
        ("time", Box::new(|l| l.time = 1.5000001)),
        ("motion", Box::new(|l| l.motion.push(Affine::scale(2.0)))),
        ("projection", Box::new(|l| l.projection = None)),
    ];
    for (field, change) in changes {
        let mut layer = full();
        change(&mut layer);
        assert_ne!(
            id(&Node::Layer(layer)),
            base,
            "{field} left the identity as it was"
        );
    }
    // A mark of sharing says nothing of what is drawn.
    let mut marked = full();
    marked.shared = Some(crate::list::Shared::new());
    assert_eq!(id(&Node::Layer(marked)), base);
}

#[test]
fn numbers_count_by_their_bits() {
    let at = |x: f64| {
        let mut layer = Layer::new(Vec::new());
        layer.transform = Affine::translate((x, 0.0));
        id(&Node::Layer(layer))
    };
    assert_ne!(at(0.0), at(-0.0));
    assert_eq!(at(f64::NAN), at(f64::NAN));
    assert_ne!(at(1.0), at(1.0 + f64::EPSILON));
}

#[test]
fn pictures_are_known_by_their_pixels() {
    let image = |v: u8| crate::Image::new(2, 2, vec![v; 16]).expect("an image");
    let draw = |picture: Picture| {
        Node::Image(ImageDraw {
            picture,
            rect: Rect::new(0.0, 0.0, 2.0, 2.0),
        })
    };
    let a = draw(Picture::from_image(&image(7)).expect("a picture"));
    let b = draw(Picture::from_image(&image(7)).expect("a picture"));
    let c = draw(Picture::from_image(&image(8)).expect("a picture"));
    assert_eq!(id(&a), id(&b), "two pictures of the same pixels");
    assert_ne!(id(&a), id(&c));
}

#[test]
fn kinds_of_nodes_with_the_same_numbers_differ() {
    let path = square(Color::WHITE);
    let in_layer = Node::Layer(Layer::new(vec![square(Color::WHITE)]));
    assert_ne!(id(&path), id(&in_layer));
    let mut ids = Identities::default();
    assert_ne!(ids.list(std::slice::from_ref(&path)), id(&path));
    assert_ne!(ids.list(&[]), ids.list(std::slice::from_ref(&path)));
}
