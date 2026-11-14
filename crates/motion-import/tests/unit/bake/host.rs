//! Unit tests for `src/bake/host.rs`: the project as an expression reads it.

use pretty_assertions::assert_eq;

use super::*;
use crate::test_support::build::{
    comp, expressed, layer, number, point, project, property, slider, transform,
};

/// Main: CONTROLS (a slider left at its default), and Title (opacity at
/// half, scale whole, and a position under an expression), parented to it.
fn host() -> (ProjectHost, Vec<NodeId>) {
    let controls = layer(1, "CONTROLS", vec![slider("Speed", None)]);
    let title = Layer {
        parent: Some(1),
        ..layer(
            2,
            "Title",
            vec![transform(vec![
                property("ADBE Opacity", number(0.5)),
                property(
                    "ADBE Scale",
                    Stream::Static(Value::Vector3([1.0, 1.0, 1.0])),
                ),
                expressed("ADBE Position", point(10.0, 20.0), "value + [1, 0]"),
            ])],
        )
    };
    let project = project(vec![(5, "Main", comp(vec![controls, title], 2.0))]);
    let (host, targets) = ProjectHost::new(&project);
    let nodes = targets.iter().map(|t| t.node).collect();
    (host, nodes)
}

fn child(host: &ProjectHost, node: NodeId, select: Select<'_>) -> NodeId {
    host.child(node, select)
        .unwrap_or_else(|| panic!("no {select:?} under {node}"))
}

#[test]
fn children_are_found_by_name_index_match_name_and_attribute() {
    let (host, _) = host();
    let main = child(&host, PROJECT, Select::Name("Main"));
    let controls = child(&host, main, Select::Index(1));
    assert_eq!(child(&host, main, Select::Name("CONTROLS")), controls);
    let effects = child(&host, controls, Select::Attribute("effect"));
    let speed = child(&host, effects, Select::Name("Speed"));
    assert_eq!(
        child(&host, effects, Select::Name("ADBE Slider Control")),
        speed
    );
    // The slider the project leaves out is there, at its index and name.
    let by_name = child(&host, speed, Select::Name("Slider"));
    assert_eq!(child(&host, speed, Select::Index(1)), by_name);
    assert_eq!(host.value_at(by_name, 0.0, false), Ok(Val::Number(0.0)));
    assert_eq!(host.kind(by_name), NodeKind::Property);
}

#[test]
fn a_layer_reaches_its_transform_and_its_parent() {
    let (host, _) = host();
    let main = child(&host, PROJECT, Select::Name("Main"));
    let title = child(&host, main, Select::Name("Title"));
    let opacity = child(&host, title, Select::Attribute("opacity"));
    let transform = child(&host, title, Select::Attribute("transform"));
    assert_eq!(child(&host, transform, Select::Name("Opacity")), opacity);
    assert_eq!(
        host.link(title, "parent"),
        Some(child(&host, main, Select::Index(1)))
    );
    assert_eq!(host.attribute(title, "index"), Some(Val::Number(2.0)));
    assert_eq!(host.attribute(title, "width"), Some(Val::Number(200.0)));
    assert_eq!(
        host.attribute(opacity, "name"),
        Some(Val::Text("Opacity".into()))
    );
    assert_eq!(
        host.attribute(main, "frameDuration"),
        Some(Val::Number(0.1))
    );
}

#[test]
fn scale_and_opacity_read_in_percent() {
    let (host, _) = host();
    let main = child(&host, PROJECT, Select::Name("Main"));
    let title = child(&host, main, Select::Name("Title"));
    let opacity = child(&host, title, Select::Attribute("opacity"));
    let scale = child(&host, title, Select::Attribute("scale"));
    assert_eq!(host.value_at(opacity, 0.0, false), Ok(Val::Number(50.0)));
    assert_eq!(
        host.value_at(scale, 0.0, false),
        Ok(Val::Vector(vec![100.0; 3]))
    );
}

#[test]
fn a_property_under_an_expression_waits_until_baked() {
    let (host, targets) = host();
    let position = targets[0];
    assert_eq!(
        host.value_at(position, 0.5, false),
        Err(Missing::Pending(position))
    );
    assert_eq!(host.demand.take(), Some((position, 0.5)));
    // Its own value is what `value` reads inside its expression.
    assert_eq!(
        host.value_at(position, 0.5, true),
        Ok(Val::Vector(vec![10.0, 20.0, 0.0]))
    );
    host.baked.borrow_mut().insert(
        position,
        Baked::Values(HashMap::from([(
            0.5f64.to_bits(),
            Val::Vector(vec![1.0, 2.0]),
        )])),
    );
    assert_eq!(
        host.value_at(position, 0.5, false),
        Ok(Val::Vector(vec![1.0, 2.0]))
    );
}
