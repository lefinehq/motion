//! Unit tests for `src/decode/panel.rs`, on hand-built projects.

use pretty_assertions::assert_eq;

use crate::decode;
use crate::model::{ControlKind, ControlTarget, ItemKind, PanelControl, PathStep};
use crate::record::Idta;
use crate::test_support::{cdta, chunk, item, list, project};

/// A `CCtl` with `id` and `code`, driving each of `targets`: a composition,
/// a layer and a path as the file writes it.
fn control(id: &str, code: u32, targets: &[(u32, u32, &str)]) -> Vec<u8> {
    let names = list(b"CapS", &[chunk(b"Utf8", b"Title")]);
    let targets = targets.iter().map(|(comp, layer, path)| {
        list(
            b"CPrp",
            &[
                chunk(b"CCId", &comp.to_be_bytes()),
                chunk(b"CLId", &layer.to_be_bytes()),
                chunk(b"Utf8", path.as_bytes()),
            ],
        )
    });
    let head = [
        names,
        chunk(b"Utf8", id.as_bytes()),
        chunk(b"CTyp", &code.to_be_bytes()),
    ];
    list(
        b"CCtl",
        &head.into_iter().chain(targets).collect::<Vec<_>>(),
    )
}

fn panel(controls: &[Vec<u8>]) -> Vec<PanelControl> {
    let cif3 = list(b"CIF3", &[&[chunk(b"CcCt", &[0; 4])], controls].concat());
    let bytes = project(
        94,
        &[item(
            Idta::COMPOSITION,
            7,
            "Main",
            &[cdta(640, 360, 24, 10), cif3],
        )],
    );
    let project = decode(&bytes).expect("a valid project");
    let ItemKind::Composition(comp) = &project.items[0].kind else {
        panic!("a composition")
    };
    comp.panel.clone()
}

#[test]
fn reads_each_control_with_its_kind_and_the_properties_it_drives() {
    let text = r#"{"0":{"index":4294967295,"matchName":"ADBE Text Properties"},"1":{"index":4294967295,"matchName":"ADBE Text Document"}}"#;
    let slider = r#"{"1":{"index":2,"matchName":"ADBE Slider Control"},"0":{"index":4294967295,"matchName":"ADBE Effect Parade"},"2":{"index":1,"matchName":"ADBE Slider Control-0001"}}"#;
    let step = |match_name: &str, index| PathStep {
        match_name: match_name.into(),
        index,
    };
    assert_eq!(
        panel(&[
            control("a1", 6, &[(7, 3, text)]),
            control("b2", 2, &[(9, 4, slider)]),
            control("c3", 10, &[]),
        ]),
        vec![
            PanelControl {
                id: "a1".into(),
                kind: ControlKind::Text,
                targets: vec![ControlTarget {
                    composition: 7,
                    layer: 3,
                    path: vec![
                        step("ADBE Text Properties", None),
                        step("ADBE Text Document", None)
                    ],
                }],
            },
            PanelControl {
                id: "b2".into(),
                kind: ControlKind::Slider,
                targets: vec![ControlTarget {
                    composition: 9,
                    layer: 4,
                    path: vec![
                        step("ADBE Effect Parade", None),
                        step("ADBE Slider Control", Some(2)),
                        step("ADBE Slider Control-0001", Some(1)),
                    ],
                }],
            },
            PanelControl {
                id: "c3".into(),
                kind: ControlKind::Group,
                targets: vec![],
            },
        ]
    );
}

#[test]
fn keeps_a_kind_it_does_not_know_and_drops_a_path_that_does_not_read() {
    let controls = panel(&[control("d4", 7, &[(7, 3, "not a path")])]);
    assert_eq!(controls[0].kind, ControlKind::Other(7));
    assert_eq!(controls[0].targets, vec![]);
}

#[test]
fn a_composition_without_a_panel_has_none() {
    let bytes = project(
        94,
        &[item(
            Idta::COMPOSITION,
            7,
            "Main",
            &[cdta(640, 360, 24, 10)],
        )],
    );
    let project = decode(&bytes).expect("a valid project");
    let ItemKind::Composition(comp) = &project.items[0].kind else {
        panic!("a composition")
    };
    assert!(comp.panel.is_empty());
}
