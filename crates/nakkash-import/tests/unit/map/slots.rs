//! Unit tests for `src/map/slots.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use super::*;

fn name(text: &str) -> Pick {
    Pick::Name(text.to_owned())
}

#[rstest]
#[case::another_layer(
    r#"thisComp.layer("Controls").effect("Accent")("Color")"#,
    CompPick::This,
    Some(name("Controls")),
    name("Accent"),
    name("Color")
)]
#[case::by_index(
    "thisComp.layer(2).effect('Size')(1);",
    CompPick::This,
    Some(Pick::Index(2)),
    name("Size"),
    Pick::Index(1)
)]
#[case::another_comp(r#" comp("Main").layer("Ctrl").effect("Lift")("Slider").value "#, CompPick::Named("Main".into()), Some(name("Ctrl")), name("Lift"), name("Slider"))]
#[case::own_layer(
    r#"effect("Size")(1)"#,
    CompPick::This,
    None,
    name("Size"),
    Pick::Index(1)
)]
#[case::this_layer(
    r#"thisLayer.effect("Size")("Slider")"#,
    CompPick::This,
    None,
    name("Size"),
    name("Slider")
)]
fn a_plain_read_names_its_control(
    #[case] source: &str,
    #[case] comp: CompPick,
    #[case] layer: Option<Pick>,
    #[case] effect: Pick,
    #[case] param: Pick,
) {
    assert_eq!(
        read(source),
        Some(Read {
            comp,
            layer,
            effect,
            param
        })
    );
}

#[rstest]
#[case::arithmetic(r#"effect("Size")(1) * 2"#)]
#[case::variable(r#"x = effect("Size")(1); x"#)]
#[case::two_statements(r#"effect("Size")(1); effect("Size")(2)"#)]
#[case::escaped_quote(r#"effect("Si\"ze")(1)"#)]
#[case::wiggle("wiggle(2, 10)")]
fn anything_more_is_not_a_plain_read(#[case] source: &str) {
    assert_eq!(read(source), None);
}

#[test]
fn reads_are_written_as_decided_and_other_dollars_escaped() {
    let decided = Decided {
        declared: BTreeMap::new(),
        written: vec![vec![json!("$accent"), json!("#FFFFFF")]],
    };
    let mut layers = json!([{"id":"a",
        "fill":format!("{REFERENCE}0:0"),"stroke":format!("{REFERENCE}0:1"),
        "text":"$99","runs":["$x","$$y"]}]);
    write_references(&mut layers, &decided);
    assert_eq!(
        layers,
        json!([{"id":"a","fill":"$accent","stroke":"#FFFFFF","text":"$99","runs":["$$x","$$$y"]}])
    );
}

#[rstest]
#[case::number(json!(1.5), Some(SlotType::Number))]
#[case::point(json!([1, 2]), Some(SlotType::Point))]
#[case::colour(json!("#E63946"), Some(SlotType::Color))]
#[case::channels(json!([1, 0.5, 0, 1]), Some(SlotType::Color))]
#[case::three_numbers(json!([1, 2, 3]), None)]
#[case::text(json!("Launch"), None)]
fn a_slot_holds_values_written_alone(#[case] value: Json, #[case] ty: Option<SlotType>) {
    assert_eq!(shorthand(&value), ty);
}

mod imported {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::map::{Options, Outcome, import};
    use crate::test_support::build::{
        comp, expressed, layer, number, project, property, slider, transform,
    };

    const SPEED: &str = r#"thisComp.layer("CONTROLS").effect("Speed")("Slider")"#;

    /// Main holds `CONTROLS`, whose slider `Speed` is 4, stored at bytes
    /// 100 to 108, and layers whose transforms have the properties given.
    fn main(layers: Vec<Vec<crate::model::Property>>) -> crate::map::Import {
        let stored = crate::model::Property {
            origin: Some(crate::model::Origin {
                numbers: 100..108,
                dims: 1,
                color: false,
            }),
            ..property("ADBE Slider Control-0001", number(4.0))
        };
        let controls = layer(9, "CONTROLS", vec![slider("Speed", Some(stored))]);
        let mut all: Vec<_> = (1..)
            .zip(layers)
            .map(|(id, properties)| layer(id, &format!("L{id}"), vec![transform(properties)]))
            .collect();
        all.push(controls);
        import(
            &project(vec![(5, "Main", comp(all, 2.0))]),
            &Options::default(),
        )
        .expect("it imports")
    }

    /// The scene layer written for `L{n}`.
    fn written(import: &crate::map::Import, n: u32) -> &serde_json::Value {
        let id = format!("l{n}");
        import.json["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == json!(id))
            .expect("the layer")
    }

    #[test]
    fn reads_of_one_control_follow_one_slot() {
        let import = main(vec![
            vec![expressed("ADBE Rotate Z", number(0.0), SPEED)],
            vec![expressed(
                "ADBE Rotate Z",
                number(0.0),
                &format!("{SPEED};"),
            )],
        ]);
        assert_eq!(
            import.json["slots"],
            json!({"speed":{"number":4,"label":"Speed"}})
        );
        let rotations: Vec<_> = import.json["layers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|l| l.get("rotate"))
            .collect();
        assert_eq!(rotations, [&json!("$speed"), &json!("$speed")]);
        assert!(matches!(
            &import.mapping.expressions[0].outcome,
            Outcome::Slot { slot } if slot == "speed"
        ));
        assert!(
            import
                .mapping
                .to_string()
                .contains("slots: 1 from the controls expressions read, which 2 values follow"),
            "{}",
            import.mapping
        );
    }

    #[test]
    fn a_slot_goes_back_into_its_control() {
        let import = main(vec![vec![expressed("ADBE Rotate Z", number(0.0), SPEED)]]);
        let source = import
            .sources
            .0
            .iter()
            .find(|s| s.path.first() == Some(&nakkash_format::Step::Key("slots".into())))
            .expect("the slot is marked");
        assert_eq!(
            nakkash_format::address::key_text(&source.path),
            "slots.speed.number"
        );
        assert!(matches!(
            &source.writable,
            crate::map::sources::Writable::Static { origin, .. } if origin.numbers == (100..108)
        ));
        // The edit goes into the slider.
        let mut edited = import.json.clone();
        edited["slots"]["speed"]["number"] = json!(6);
        let plan = crate::export::plan(&import.json, &import.sources, &edited);
        assert_eq!(plan.written.len(), 1, "{:?}", plan.not_written);
        assert_eq!(plan.patched_bytes(), 8);
        assert_eq!(plan.written[0].at, "slots.speed.number");
    }

    #[test]
    fn a_read_in_other_units_stays_baked() {
        // Opacity is stored in percent and written as a fraction: 4% is
        // 0.04. Two rotations read 4, so the slot holds 4, though the
        // opacity is written first.
        let import = main(vec![
            vec![expressed("ADBE Rotate Z", number(0.0), SPEED)],
            vec![expressed("ADBE Opacity", number(100.0), SPEED)],
            vec![expressed("ADBE Rotate Z", number(0.0), SPEED)],
        ]);
        assert_eq!(written(&import, 1)["rotate"], json!("$speed"));
        assert_eq!(written(&import, 2)["opacity"], json!(0.04));
    }

    #[test]
    fn a_read_that_does_more_stays_baked() {
        let import = main(vec![vec![expressed(
            "ADBE Rotate Z",
            number(0.0),
            &format!("{SPEED} * 2"),
        )]]);
        assert_eq!(written(&import, 1)["rotate"], json!(8));
        assert_eq!(import.json.get("slots"), None);
    }
}
