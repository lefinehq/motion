//! Unit tests for `src/decode/property.rs`, on hand-built property trees.

use std::collections::BTreeMap;

use super::*;
use crate::test_support::{chunk, list, record};

/// A `tdmn` holding `match_name`, NUL-padded to 40 bytes.
fn tdmn(match_name: &str) -> Vec<u8> {
    chunk(b"tdmn", &record(40, &[(0, match_name.as_bytes())]))
}

/// A `tdsn` holding `name`, wrapped in a `Utf8` as from file version 92 on.
fn tdsn(name: &str) -> Vec<u8> {
    chunk(b"tdsn", &chunk(b"Utf8", name.as_bytes()))
}

/// A `tdb4` with `dims`, stream `flags`, value flags and type flags.
fn tdb4(dims: u16, flags: u16, valueless: bool, color: bool) -> Vec<u8> {
    let bytes = record(
        124,
        &[
            (0x02, &dims.to_be_bytes()),
            (0x04, &flags.to_be_bytes()),
            (0x38, &u16::from(valueless).to_be_bytes()),
            (0x3A, &u16::from(color).to_be_bytes()),
        ],
    );
    chunk(b"tdb4", &bytes)
}

/// A `cdat` holding `numbers`.
fn cdat(numbers: &[f64]) -> Vec<u8> {
    let bytes: Vec<u8> = numbers.iter().flat_map(|n| n.to_be_bytes()).collect();
    chunk(b"cdat", &bytes)
}

/// A stream list: flags, the default name, then `rest`.
fn tdbs(rest: &[Vec<u8>]) -> Vec<u8> {
    let head = [chunk(b"tdsb", &1u32.to_be_bytes()), tdsn("-_0_/-")];
    list(b"tdbs", &[&head, rest].concat())
}

/// The children of a group named `name`, ending with `ADBE Group End`.
fn group_body(flags: u32, name: &str, children: &[Vec<u8>]) -> Vec<u8> {
    let head = [chunk(b"tdsb", &flags.to_be_bytes()), tdsn(name)];
    [&head, children, &[tdmn(GROUP_END)]].concat().concat()
}

/// Reads `body` as a group's children, in a file of the wrapped-text era
/// or not, and returns the group with the chunks it skipped.
fn read(body: &[u8], wrapped: bool) -> (Result<Group>, BTreeMap<String, usize>) {
    let decoder = Decoder::default();
    decoder.wrapped_text.set(wrapped);
    let root = Chunk { name: TDGP, body };
    let group = root
        .chunks()
        .and_then(|chunks| Scan::new(&decoder, chunks, "LIST:tdgp".into(), 1))
        .and_then(|scan| decoder.group(scan, String::new()));
    (group, decoder.skipped.take())
}

/// The property at `index` of `group`.
fn property(group: &Group, index: usize) -> &Property {
    match group.children.get(index) {
        Some(Child::Property(property)) => property,
        other => panic!("a property, not {other:?}"),
    }
}

/// The group at `index` of `group`.
fn subgroup(group: &Group, index: usize) -> &Group {
    match group.children.get(index) {
        Some(Child::Group(group)) => group,
        other => panic!("a group, not {other:?}"),
    }
}

#[test]
fn reads_static_numbers_vectors_and_colours() {
    let body = group_body(
        1,
        "",
        &[
            tdmn("ADBE Opacity"),
            tdbs(&[
                tdb4(1, 0x01, false, false),
                cdat(&[0.5, 0.0, 0.0, 0.0, 0.0]),
                chunk(b"tdum", &0f64.to_be_bytes()),
                chunk(b"tduM", &100f64.to_be_bytes()),
            ]),
            tdmn("ADBE Vector Rect Size"),
            tdbs(&[tdb4(2, 0x01, false, false), cdat(&[10.0, 20.0, 0.0, 0.0])]),
            tdmn("ADBE Position"),
            tdbs(&[tdb4(3, 0x0F, false, false), cdat(&[1.0, 2.0, 3.0, 0.0])]),
            tdmn("ADBE Vector Fill Color"),
            tdbs(&[tdb4(4, 0x07, false, true), cdat(&[255.0, 255.0, 0.0, 51.0])]),
        ],
    );
    let (group, _) = read(&body, true);
    let group = group.unwrap();
    let values: Vec<&Stream> = (0..4).map(|i| &property(&group, i).value).collect();
    assert_eq!(
        values,
        [
            &Stream::Static(Value::Number(0.5)),
            &Stream::Static(Value::Vector2([10.0, 20.0])),
            &Stream::Static(Value::Vector3([1.0, 2.0, 3.0])),
            &Stream::Static(Value::Color(Color {
                r: 1.0,
                g: 0.0,
                b: 0.2,
                a: 1.0
            })),
        ]
    );
    let opacity = property(&group, 0);
    assert_eq!(
        (opacity.match_name.as_str(), opacity.name.as_deref()),
        ("ADBE Opacity", None)
    );
    assert_eq!((opacity.min, opacity.max), (Some(0.0), Some(100.0)));
}

#[test]
fn reads_names_flags_and_expressions() {
    let position = tdbs(&[
        tdb4(1, 0x01, false, false),
        cdat(&[7.0]),
        chunk(b"Utf8", b"time * 10"),
    ]);
    let body = group_body(
        0,
        "Fill 1",
        &[tdmn("ADBE Rotate Z"), position, tdmn("ADBE Group End")],
    );
    let (group, _) = read(&body, true);
    let group = group.unwrap();
    assert_eq!(
        (group.name.as_deref(), group.enabled),
        (Some("Fill 1"), false)
    );
    let rotation = property(&group, 0);
    assert_eq!(
        rotation.expression,
        Some(Expression {
            text: "time * 10".into(),
            enabled: true
        })
    );
    assert_eq!(group.children.len(), 1);
}

#[test]
fn reads_legacy_names_and_expressions() {
    let stream = list(
        b"tdbs",
        &[
            chunk(b"tdsb", &1u32.to_be_bytes()),
            chunk(b"tdsn", b"Spin\0"),
            chunk(b"tdb4", &record(124, &[(0x02, &[0, 1]), (0x77, &[1])])),
            cdat(&[1.0]),
            chunk(b"expr", b"wiggle(1, 2)\0"),
        ],
    );
    let body = [
        chunk(b"tdsb", &1u32.to_be_bytes()),
        chunk(b"tdsn", b"Polystar 1\0"),
        tdmn("ADBE Rotate Z"),
        stream,
        tdmn(GROUP_END),
    ]
    .concat();
    let (group, _) = read(&body, false);
    let group = group.unwrap();
    assert_eq!(group.name.as_deref(), Some("Polystar 1"));
    let rotation = property(&group, 0);
    assert_eq!(rotation.name.as_deref(), Some("Spin"));
    assert_eq!(
        rotation.expression,
        Some(Expression {
            text: "wiggle(1, 2)".into(),
            enabled: false
        })
    );
}

#[test]
fn reads_effects_masks_and_wrapped_values_by_their_body() {
    let slider = tdbs(&[tdb4(1, 0x01, false, false), cdat(&[3.0])]);
    let effect = list(
        b"sspc",
        &[
            chunk(b"fnam", &chunk(b"Utf8", b"Slider Control")),
            list(b"parT", &[]),
            list(
                b"tdgp",
                &[group_body(
                    1,
                    "-_0_/-",
                    &[tdmn("ADBE Slider Control-0001"), slider],
                )],
            ),
        ],
    );
    let parade = list(
        b"tdgp",
        &[group_body(1, "", &[tdmn("ADBE Slider Control"), effect])],
    );
    let mask = list(b"tdgp", &[group_body(1, "Mask 1", &[])]);
    let text = list(
        b"btds",
        &[tdbs(&[tdb4(1, 0x01, true, false), cdat(&[0.0])])],
    );
    let body = group_body(
        1,
        "",
        &[
            tdmn("ADBE Effect Parade"),
            parade,
            tdmn("ADBE Mask Atom"),
            chunk(b"mkif", &[0; 48]),
            mask,
            tdmn("ADBE Text Document"),
            text,
            tdmn("ADBE Layer Source Alternate"),
            chunk(b"blsv", &[0; 4]),
        ],
    );
    let (group, skipped) = read(&body, true);
    let group = group.unwrap();
    let effect = subgroup(subgroup(&group, 0), 0);
    assert_eq!(effect.match_name, "ADBE Slider Control");
    assert_eq!(
        property(effect, 0).value,
        Stream::Static(Value::Number(3.0))
    );
    assert_eq!(subgroup(&group, 1).name.as_deref(), Some("Mask 1"));
    let opaque = |reason| Stream::Static(Value::Opaque { reason });
    assert_eq!(property(&group, 2).value, opaque(Reason::TextDocument));
    assert_eq!(property(&group, 3).value, opaque(Reason::OtherBody));
    let skipped: Vec<&str> = skipped.keys().map(String::as_str).collect();
    assert_eq!(skipped, ["LIST:parT", "blsv", "fnam", "mkif"]);
}

#[test]
fn keeps_values_of_unknown_dimensions_opaque() {
    let body = group_body(
        1,
        "",
        &[
            tdmn("ADBE Odd"),
            tdbs(&[tdb4(5, 0x01, false, false), cdat(&[0.0; 25])]),
        ],
    );
    let (group, _) = read(&body, true);
    assert_eq!(
        property(&group.unwrap(), 0).value,
        Stream::Static(Value::Opaque {
            reason: Reason::Dimensions
        })
    );
}

#[test]
fn refuses_a_value_shorter_than_its_dimensions() {
    let body = group_body(
        1,
        "",
        &[
            tdmn("ADBE Position"),
            tdbs(&[tdb4(3, 0x0F, false, false), cdat(&[1.0, 2.0])]),
        ],
    );
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref(), error.expected.as_deref()),
        (
            Kind::Invalid,
            Some("LIST:tdgp/ADBE Position/cdat"),
            Some("3 numbers")
        )
    );
}

#[test]
fn refuses_a_stream_without_its_metadata() {
    let body = group_body(1, "", &[tdmn("ADBE Opacity"), tdbs(&[cdat(&[1.0])])]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::Missing, Some("LIST:tdgp/ADBE Opacity"))
    );
}
