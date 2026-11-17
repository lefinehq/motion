//! Unit tests for `src/decode/effect.rs`, on hand-built parameter definitions.

use std::collections::BTreeMap;

use pretty_assertions::assert_eq;

use super::*;
use crate::rifx::Chunk;
use crate::test_support::{chunk, file, list, record};

/// A `tdmn` holding `match_name`, NUL-padded to 40 bytes.
fn tdmn(match_name: &str) -> Vec<u8> {
    chunk(b"tdmn", &record(40, &[(0, match_name.as_bytes())]))
}

/// A `pard` of control type `control`, called `name`, with `fields` of its
/// body written at their offsets.
fn pard(control: u32, name: &str, fields: &[(usize, &[u8])]) -> Vec<u8> {
    let head: [(usize, &[u8]); 2] = [(0x0C, &control.to_be_bytes()), (0x10, name.as_bytes())];
    let fields: Vec<(usize, &[u8])> = head.iter().chain(fields).copied().collect();
    chunk(b"pard", &record(148, &fields))
}

/// A wrapper chunk `tag` holding `text` in a `Utf8`.
fn wrapped(tag: &[u8; 4], text: &str) -> Vec<u8> {
    chunk(tag, &chunk(b"Utf8", text.as_bytes()))
}

/// A signed `fixed 16.16` number's bytes.
fn fixed(value: f64) -> [u8; 4] {
    #[expect(clippy::cast_possible_truncation, reason = "test values fit 16.16")]
    let raw = (value * 65536.0).round() as i32;
    raw.to_be_bytes()
}

/// A `LIST:parT` counting `count` parameters and holding `children`.
fn part(count: u32, children: &[Vec<u8>]) -> Vec<u8> {
    let head = [chunk(b"parn", &count.to_be_bytes())];
    list(b"parT", &[&head, children].concat())
}

/// An effect body holding `part`, as a layer's effect or a definition has.
fn sspc(name: &str, part: Vec<u8>) -> Vec<u8> {
    list(b"sspc", &[wrapped(b"fnam", name), part, list(b"tdgp", &[])])
}

/// The parameters of a definition with one parameter of every control the
/// reader decodes, and one it does not.
fn every_control() -> Vec<Vec<u8>> {
    vec![
        tdmn("FX-0000"),
        pard(0, "", &[]),
        tdmn("FX-0001"),
        pard(5, "Color", &[(0x3C, &[255, 255, 0, 51])]),
        tdmn("FX-0002"),
        pard(
            7,
            "Mode",
            &[(0x3C, &3i16.to_be_bytes()), (0x3E, &2i16.to_be_bytes())],
        ),
        wrapped(b"pdnm", "Over|Under|Both"),
        tdmn("FX-0003"),
        pard(
            10,
            "Mix",
            &[
                (0x68, &f32::NEG_INFINITY.to_be_bytes()),
                (0x6C, &1f32.to_be_bytes()),
                (0x78, &0.5f32.to_be_bytes()),
                (0x7E, &1u16.to_be_bytes()),
            ],
        ),
        tdmn("FX-0004"),
        pard(6, "Centre", &[(0x44, &fixed(50.0)), (0x48, &fixed(25.0))]),
        tdmn("FX-0005"),
        pard(
            2,
            "Feather",
            &[
                (0x7C, &fixed(-10.0)),
                (0x80, &fixed(999.0)),
                (0x8C, &fixed(1.5)),
            ],
        ),
        tdmn("FX-0006"),
        pard(4, "Invert", &[(0x3C, &[1])]),
        tdmn("FX-0007"),
        pard(3, "Angle", &[(0x3C, &fixed(-45.0))]),
        tdmn("FX-0008"),
        pard(9, "", &[(0x30, &Pard::CANNOT_TIME_VARY.to_be_bytes())]),
        chunk(b"aRbp", &[0; 4]),
    ]
}

/// Reads `body` as an effect body of `match_name` in a wrapped-text file and
/// returns the definitions recorded, or the error.
fn define(body: &[u8], match_name: &str) -> Result<BTreeMap<String, EffectDefinition>> {
    let decoder = Decoder::default();
    decoder.wrapped_text.set(true);
    let chunks = Chunk { name: SSPC, body }.chunks()?;
    let mut scan = Scan::new(&decoder, chunks, "LIST:sspc".into(), 1)?;
    decoder.define_effect(&mut scan, match_name)?;
    drop(scan);
    Ok(decoder.effects.take())
}

/// The body of an `sspc` list, without its header.
fn body_of(list: &[u8]) -> &[u8] {
    &list[12..]
}

#[test]
fn reads_each_control_with_its_default_and_bounds() {
    let effect = sspc("Effect", part(9, &every_control()));
    let effects = define(body_of(&effect), "FX").unwrap();
    let definition = &effects["FX"];
    assert_eq!(definition.name, "Effect");
    let controls: Vec<(&str, &Control)> = definition
        .parameters
        .iter()
        .map(|p| (p.match_name.as_str(), &p.control))
        .collect();
    assert_eq!(
        controls,
        [
            ("FX-0000", &Control::Layer),
            (
                "FX-0001",
                &Control::Color {
                    default: Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.2,
                        a: 1.0
                    }
                }
            ),
            (
                "FX-0002",
                &Control::Popup {
                    default: 2,
                    choices: vec!["Over".into(), "Under".into(), "Both".into()]
                }
            ),
            (
                "FX-0003",
                &Control::Slider {
                    default: 0.5,
                    min: None,
                    max: Some(1.0),
                    percent: true
                }
            ),
            (
                "FX-0004",
                &Control::Point {
                    default: vec![0.5, 0.25]
                }
            ),
            (
                "FX-0005",
                &Control::Slider {
                    default: 1.5,
                    min: Some(-10.0),
                    max: Some(999.0),
                    percent: false
                }
            ),
            ("FX-0006", &Control::Checkbox { default: true }),
            ("FX-0007", &Control::Angle { default: -45.0 }),
            ("FX-0008", &Control::Other { code: 9 }),
        ]
    );
    let names: Vec<&str> = definition
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names[1..4], ["Color", "Mode", "Mix"]);
    let animatable: Vec<bool> = definition.parameters.iter().map(|p| p.animatable).collect();
    assert_eq!(animatable.last(), Some(&false));
    assert!(animatable[..8].iter().all(|a| *a));
    assert_eq!(
        definition.parameter("FX-0006").map(|p| p.name.as_str()),
        Some("Invert")
    );
}

#[test]
fn records_nothing_for_an_instance_without_definitions() {
    let effect = sspc("Effect", list(b"parT", &[]));
    assert_eq!(define(body_of(&effect), "FX").unwrap(), BTreeMap::new());
}

#[test]
fn refuses_a_count_that_disagrees_with_the_list() {
    let effect = sspc("Effect", part(3, &[tdmn("FX-0000"), pard(0, "", &[])]));
    let error = define(body_of(&effect), "FX").unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(
        error
            .message
            .contains("counts 3 parameters, the list holds 1")
    );
}

#[test]
fn refuses_a_parameter_without_its_definition() {
    let effect = sspc(
        "Effect",
        part(1, &[tdmn("FX-0000"), chunk(b"aRbp", &[0; 4])]),
    );
    let error = define(body_of(&effect), "FX").unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert!(error.message.contains("`FX-0000` has no `pard`"));
}

#[test]
fn reads_the_project_table_and_keeps_the_first_definition() {
    let colour = |value: u8| {
        part(
            1,
            &[
                tdmn("FX-0001"),
                pard(5, "Color", &[(0x3C, &[255, value, 0, 0])]),
            ],
        )
    };
    let table = list(
        b"EfdG",
        &[
            chunk(b"EfDC", &3u32.to_le_bytes()),
            list(b"EfDf", &[tdmn("FX"), sspc("First", colour(255))]),
            list(b"EfDf", &[tdmn("FX"), sspc("Second", colour(0))]),
            list(b"EfDf", &[tdmn("Other"), sspc("Other", colour(0))]),
        ],
    );
    let bytes = file(&[
        chunk(b"head", &record(20, &[(0, &94u16.to_be_bytes())])),
        table,
        list(b"Fold", &[chunk(b"fdta", &[0; 14])]),
    ]);
    let project = crate::decode(&bytes).unwrap();
    let names: Vec<(&str, &str)> = project
        .effects
        .iter()
        .map(|(match_name, d)| (match_name.as_str(), d.name.as_str()))
        .collect();
    assert_eq!(names, [("FX", "First"), ("Other", "Other")]);
    assert!(!project.skipped.contains_key("LIST:EfdG"));
    assert!(!project.skipped.contains_key("EfDC"));
}

#[test]
fn refuses_a_table_whose_count_disagrees() {
    let table = list(b"EfdG", &[chunk(b"EfDC", &2u32.to_le_bytes())]);
    let bytes = file(&[
        chunk(b"head", &record(20, &[(0, &94u16.to_be_bytes())])),
        table,
        list(b"Fold", &[chunk(b"fdta", &[0; 14])]),
    ]);
    let error = crate::decode(&bytes).unwrap_err();
    assert!(
        error
            .message
            .contains("`EfDC` counts 2 definitions, the list holds 0")
    );
}
