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
    let (host, targets) = ProjectHost::new(&project, &super::super::Layouts::default());
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
        Ok(Val::Vector(vec![100.0; 2]))
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
        Ok(Val::Vector(vec![10.0, 20.0]))
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

#[test]
fn a_sourced_layers_anchor_reads_in_pixels_of_its_source() {
    use crate::test_support::build::solid;
    // A null over a 100 by 100 solid stores its middle as [0.5, 0.5]; a
    // layer without a source stores its anchor in pixels.
    let anchored = |id, name| {
        layer(
            id,
            name,
            vec![transform(vec![property(
                "ADBE Anchor Point",
                point(0.5, 0.25),
            )])],
        )
    };
    let null = Layer {
        source: Some(9),
        ..anchored(1, "Null")
    };
    let mut project = project(vec![(
        5,
        "Main",
        comp(vec![null, anchored(2, "Free")], 2.0),
    )]);
    project.items.push(solid(9, 100, 100));
    let (host, _) = ProjectHost::new(&project, &super::super::Layouts::default());
    let main = child(&host, PROJECT, Select::Name("Main"));
    let anchor = |name| {
        let layer = child(&host, main, Select::Name(name));
        host.value_at(
            child(&host, layer, Select::Attribute("anchorPoint")),
            0.0,
            false,
        )
    };
    assert_eq!(anchor("Null"), Ok(Val::Vector(vec![50.0, 25.0])));
    assert_eq!(anchor("Free"), Ok(Val::Vector(vec![0.5, 0.25])));
}

#[test]
fn a_2d_layers_transform_reads_in_two_components_a_3d_layers_in_three() {
    use crate::model::{Interpolation, Keyframe, LayerKind};
    // The project keeps three components of each; an expression sees two
    // of a 2D layer's anchor point, position and scale, keyed or not.
    let still = || Stream::Static(Value::Vector3([1.0, 2.0, 3.0]));
    let keyed = Stream::Animated(vec![Keyframe {
        time: 0.0,
        value: Value::Vector3([4.0, 5.0, 6.0]),
        in_interpolation: Interpolation::Linear,
        out_interpolation: Interpolation::Linear,
        in_ease: vec![],
        out_ease: vec![],
        spatial: None,
        continuous: false,
        auto_bezier: false,
        roving: false,
    }]);
    let transformed = |id, name| {
        layer(
            id,
            name,
            vec![transform(vec![
                property("ADBE Anchor Point", still()),
                property("ADBE Position", keyed.clone()),
                property("ADBE Scale", still()),
                property("ADBE Orientation", still()),
            ])],
        )
    };
    let mut three_d = transformed(2, "Deep");
    three_d.switches.three_d = true;
    let camera = Layer {
        kind: LayerKind::Camera,
        ..transformed(3, "Camera")
    };
    let project = project(vec![(
        5,
        "Main",
        comp(vec![transformed(1, "Flat"), three_d, camera], 2.0),
    )]);
    let (host, _) = ProjectHost::new(&project, &super::super::Layouts::default());
    let main = child(&host, PROJECT, Select::Name("Main"));
    let read = |name, attribute| {
        let layer = child(&host, main, Select::Name(name));
        let node = child(&host, layer, Select::Attribute(attribute));
        (host.value_at(node, 0.0, false), host.key(node, 1))
    };
    assert_eq!(
        read("Flat", "anchorPoint").0,
        Ok(Val::Vector(vec![1.0, 2.0]))
    );
    assert_eq!(
        read("Flat", "position"),
        (
            Ok(Val::Vector(vec![4.0, 5.0])),
            Some((0.0, Val::Vector(vec![4.0, 5.0])))
        )
    );
    assert_eq!(read("Flat", "scale").0, Ok(Val::Vector(vec![100.0, 200.0])));
    assert_eq!(
        read("Flat", "orientation").0,
        Ok(Val::Vector(vec![1.0, 2.0, 3.0])),
        "a 3D-only property keeps its three"
    );
    for name in ["Deep", "Camera"] {
        assert_eq!(
            read(name, "anchorPoint").0,
            Ok(Val::Vector(vec![1.0, 2.0, 3.0]))
        );
        assert_eq!(
            read(name, "position").0,
            Ok(Val::Vector(vec![4.0, 5.0, 6.0]))
        );
        assert_eq!(
            read(name, "scale").0,
            Ok(Val::Vector(vec![100.0, 200.0, 300.0]))
        );
    }
}
