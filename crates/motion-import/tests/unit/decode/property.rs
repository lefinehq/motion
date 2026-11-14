//! Unit tests for `src/decode/property.rs`, on hand-built property trees.

use std::collections::BTreeMap;

use super::*;
use crate::model::Interpolation;
use crate::test_support::{chunk, list, record, text_body, text_document};

/// A `tdmn` holding `match_name`, NUL-padded to 40 bytes.
fn tdmn(match_name: &str) -> Vec<u8> {
    chunk(b"tdmn", &record(40, &[(0, match_name.as_bytes())]))
}

/// A `tdsn` holding `name`, wrapped in a `Utf8` as from file version 92 on.
fn tdsn(name: &str) -> Vec<u8> {
    chunk(b"tdsn", &chunk(b"Utf8", name.as_bytes()))
}

/// A `tdb4` with `dims`, stream `flags`, value flags and type flags, and a
/// time base of 600 units a second.
fn tdb4(dims: u16, flags: u16, valueless: bool, color: bool) -> Vec<u8> {
    let bytes = record(
        124,
        &[
            (0x02, &dims.to_be_bytes()),
            (0x04, &flags.to_be_bytes()),
            (0x0C, &600u32.to_be_bytes()),
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
        .and_then(|scan| decoder.group(scan, String::new(), TIMING));
    (group, decoder.skipped.take())
}

/// A `LIST:btdk` whose documents hold `texts`, each as stored.
fn btdk(texts: &[&str]) -> Vec<u8> {
    let documents: Vec<String> = texts
        .iter()
        .map(|text| {
            let count = text.encode_utf16().count();
            text_document(text, &[(count, "")], &[(count, "")], "")
        })
        .collect();
    let body = text_body(&["Inter-Bold"], &[], &documents);
    chunk(b"LIST", &[b"btdk".as_slice(), body.as_bytes()].concat())
}

/// The text of a static text document, or of nothing else.
fn text_of(stream: &Stream) -> Vec<String> {
    match stream {
        Stream::Static(Value::Text(document)) => vec![document.text.clone()],
        other => panic!("a text document, not {other:?}"),
    }
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
        &[
            tdbs(&[tdb4(1, 0x01, true, false), cdat(&[0.0])]),
            btdk(&["Hi\r"]),
        ],
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
    assert_eq!(text_of(&property(&group, 2).value), ["Hi"]);
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

/// The layer timing the tests read keyframes with: starting at 1 s, at
/// 200 % stretch.
const TIMING: Timing = Timing {
    start: 1.0,
    stretch: 2.0,
};

/// A keyframe list of `items`, each `size` bytes.
fn keys(size: u32, items: &[Vec<u8>]) -> Vec<u8> {
    let count = u32::try_from(items.len()).expect("a few items");
    let lhd3 = record(
        52,
        &[(0x08, &count.to_be_bytes()), (0x10, &size.to_be_bytes())],
    );
    list(
        b"list",
        &[chunk(b"lhd3", &lhd3), chunk(b"ldat", &items.concat())],
    )
}

/// A keyframe item at `time` with `interpolation` on both sides, key
/// `flags`, then `numbers`.
fn key_item(time: i32, interpolation: u8, flags: u16, numbers: &[f64]) -> Vec<u8> {
    let head = [
        time.to_be_bytes().as_slice(),
        &[interpolation, interpolation],
        &flags.to_be_bytes(),
    ]
    .concat();
    let numbers: Vec<u8> = numbers.iter().flat_map(|n| n.to_be_bytes()).collect();
    [head, numbers].concat()
}

/// The keyframes of the only property of a group holding `stream` as
/// `match_name`.
fn animated(match_name: &str, stream: Vec<u8>) -> Vec<Keyframe> {
    let body = group_body(1, "", &[tdmn(match_name), stream]);
    let group = read(&body, true).0.unwrap();
    match &property(&group, 0).value {
        Stream::Animated(keyframes) => keyframes.clone(),
        other => panic!("keyframes, not {other:?}"),
    }
}

#[test]
fn reads_number_keyframes_with_an_ease_per_dimension() {
    let stream = tdbs(&[
        tdb4(1, 0x00, false, false),
        keys(
            48,
            &[
                key_item(0, 2, 0x18, &[0.0, 1.0, 0.25, 2.0, 0.75]),
                key_item(300, 3, 0x00, &[1.0, 0.0, 0.5, 0.0, 0.5]),
            ],
        ),
    ]);
    let [first, second] = &animated("ADBE Opacity", stream)[..] else {
        panic!("two keyframes")
    };
    assert_eq!((first.time, second.time), (1.0, 2.0));
    assert_eq!(
        (first.value.clone(), second.value.clone()),
        (Value::Number(0.0), Value::Number(1.0))
    );
    assert_eq!(
        (first.in_interpolation, second.out_interpolation),
        (Interpolation::Bezier, Interpolation::Hold)
    );
    assert_eq!(
        (first.in_ease.clone(), first.out_ease.clone()),
        (
            vec![Ease {
                speed: 1.0,
                influence: 0.25
            }],
            vec![Ease {
                speed: 2.0,
                influence: 0.75
            }]
        )
    );
    assert_eq!(
        (
            first.continuous,
            first.auto_bezier,
            first.roving,
            first.spatial.clone()
        ),
        (true, true, false, None)
    );
}

#[test]
fn reads_spatial_keyframes_with_their_tangents() {
    let numbers = [
        [0.0; 2].as_slice(),
        &[3.0, 0.5, 4.0, 0.25],
        &[10.0, 20.0, 0.0],
        &[-1.0, 0.0, 0.0],
        &[1.0, 0.0, 0.0],
    ]
    .concat();
    let mut item = key_item(600, 2, 0x27, &numbers);
    item[8..12].copy_from_slice(&3u32.to_be_bytes());
    let stream = tdbs(&[tdb4(3, 0x0E, false, false), keys(128, &[item])]);
    let [key] = &animated("ADBE Position", stream)[..] else {
        panic!("one keyframe")
    };
    assert_eq!(
        (key.time, key.value.clone()),
        (3.0, Value::Vector3([10.0, 20.0, 0.0]))
    );
    assert_eq!(
        (key.in_ease.clone(), key.out_ease.clone()),
        (
            vec![Ease {
                speed: 3.0,
                influence: 0.5
            }],
            vec![Ease {
                speed: 4.0,
                influence: 0.25
            }]
        )
    );
    assert_eq!(
        key.spatial,
        Some(Spatial {
            in_tangent: vec![-1.0, 0.0, 0.0],
            out_tangent: vec![1.0, 0.0, 0.0],
            continuous: true,
            auto_bezier: true
        })
    );
    assert!(key.roving);
}

#[test]
fn reads_colour_and_valueless_keyframes() {
    let color = [[0.0; 6].as_slice(), &[255.0, 0.0, 255.0, 0.0], &[0.0; 8]].concat();
    let fill = tdbs(&[
        tdb4(4, 0x06, false, true),
        keys(152, &[key_item(0, 1, 1, &color)]),
    ]);
    let [key] = &animated("ADBE Vector Fill Color", fill)[..] else {
        panic!("one keyframe")
    };
    assert_eq!(
        (key.value.clone(), key.spatial.clone()),
        (
            Value::Color(Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0
            }),
            None
        )
    );
    let text = list(
        b"btds",
        &[
            tdbs(&[
                tdb4(1, 0x00, true, false),
                keys(
                    16,
                    &[key_item(0, 3, 0, &[0.0]), key_item(600, 3, 0, &[0.0])],
                ),
            ]),
            btdk(&["One\r", "Two\r"]),
        ],
    );
    let documents = animated("ADBE Text Document", text);
    let texts: Vec<Vec<String>> = documents
        .iter()
        .map(|key| text_of(&Stream::Static(key.value.clone())))
        .collect();
    assert_eq!(texts, [["One"], ["Two"]]);
    assert_eq!(documents[0].in_ease.len(), 0);
    let gradient = list(
        b"GCst",
        &[tdbs(&[
            tdb4(1, 0x06, true, false),
            keys(64, &[key_item(0, 1, 1, &[0.0; 7])]),
        ])],
    );
    let [key] = &animated("ADBE Vector Grad Colors", gradient)[..] else {
        panic!("one keyframe")
    };
    assert_eq!(
        (key.value.clone(), key.in_ease.len()),
        (
            Value::Opaque {
                reason: Reason::Gradient
            },
            1
        )
    );
}

#[test]
fn reversed_layers_shift_their_keyframes() {
    let timing = Timing {
        start: 2.0,
        stretch: -1.5,
    };
    let time_base = NonZeroU32::new(600).expect("not zero");
    assert_eq!(timing.seconds(600, time_base), 2.0 - 1.5 - 0.0005);
}

#[test]
fn refuses_keyframes_the_list_cannot_hold() {
    let short = tdbs(&[tdb4(1, 0x00, false, false), keys(48, &[vec![0; 40]])]);
    let body = group_body(1, "", &[tdmn("ADBE Opacity"), short]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref(), error.expected.as_deref()),
        (
            Kind::Invalid,
            Some("LIST:tdgp/ADBE Opacity/LIST:list/ldat"),
            Some("an `ldat` of count x item size bytes")
        )
    );

    let small = tdbs(&[tdb4(3, 0x00, false, false), keys(16, &[vec![0; 16]])]);
    let body = group_body(1, "", &[tdmn("ADBE Scale"), small]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::Invalid, Some("LIST:tdgp/ADBE Scale/LIST:list/ldat"))
    );

    let mut timeless = tdb4(1, 0x00, false, false);
    timeless[8 + 0x0C..8 + 0x10].fill(0);
    let stream = tdbs(&[timeless, keys(48, &[key_item(0, 1, 0, &[0.0; 5])])]);
    let body = group_body(1, "", &[tdmn("ADBE Opacity"), stream]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(
        error.expected.as_deref(),
        Some("a `tdb4` time base of at least 1")
    );
}

/// A `LIST:shap` in the box from (0, 0) to (100, 200), with `flags` and
/// `points` normalized to the box.
fn shap(flags: u8, points: &[[f32; 2]]) -> Vec<u8> {
    let bounds: Vec<u8> = [0.0f32, 0.0, 100.0, 200.0]
        .iter()
        .flat_map(|edge| edge.to_be_bytes())
        .collect();
    let shph = record(24, &[(0x03, &[flags]), (0x04, &bounds)]);
    let points: Vec<u8> = points
        .iter()
        .flatten()
        .flat_map(|c| c.to_be_bytes())
        .collect();
    let count = u32::try_from(points.len() / 8).expect("a few points");
    let lhd3 = record(52, &[(0x08, &count.to_be_bytes()), (0x10, &[0, 0, 0, 8])]);
    list(
        b"shap",
        &[
            chunk(b"shph", &shph),
            list(b"list", &[chunk(b"lhd3", &lhd3), chunk(b"ldat", &points)]),
            chunk(b"omtn", &[]),
        ],
    )
}

/// Two vertices: (0, 0) and (100, 200), with their control points.
const TWO_VERTICES: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [0.125, 0.0],
    [0.5, 0.875],
    [1.0, 1.0],
    [0.875, 1.0],
    [0.0, 0.25],
];

#[test]
fn reads_paths_with_tangents_relative_to_their_vertices() {
    let stream = tdbs(&[tdb4(1, 0x07, true, false), chunk(b"cdat", &[0; 4])]);
    let path = list(
        b"om-s",
        &[stream, list(b"omks", &[shap(0x01, &TWO_VERTICES)])],
    );
    let body = group_body(1, "", &[tdmn("ADBE Vector Shape"), path]);
    let group = read(&body, true).0.unwrap();
    assert_eq!(
        property(&group, 0).value,
        Stream::Static(Value::Path(Path {
            closed: true,
            vertices: vec![
                Vertex {
                    point: [0.0, 0.0],
                    in_tangent: [0.0, 50.0],
                    out_tangent: [12.5, 0.0],
                },
                Vertex {
                    point: [100.0, 200.0],
                    in_tangent: [-50.0, -25.0],
                    out_tangent: [-12.5, 0.0],
                },
            ],
        }))
    );
}

#[test]
fn gives_each_path_keyframe_its_own_path() {
    let stream = tdbs(&[
        tdb4(1, 0x06, true, false),
        keys(
            64,
            &[key_item(0, 1, 1, &[0.0; 7]), key_item(600, 1, 1, &[0.0; 7])],
        ),
    ]);
    let paths = list(b"omks", &[shap(0x09, &[]), shap(0x01, &TWO_VERTICES)]);
    let keyframes = animated("ADBE Mask Shape", list(b"om-s", &[stream, paths]));
    let shapes: Vec<(bool, usize)> = keyframes
        .iter()
        .map(|key| match &key.value {
            Value::Path(path) => (path.closed, path.vertices.len()),
            other => panic!("a path, not {other:?}"),
        })
        .collect();
    assert_eq!(shapes, [(false, 0), (true, 2)]);
}

#[test]
fn refuses_a_path_keyframe_without_its_path() {
    let stream = tdbs(&[
        tdb4(1, 0x06, true, false),
        keys(64, &[key_item(0, 1, 1, &[0.0; 7])]),
    ]);
    let body = group_body(1, "", &[tdmn("ADBE Mask Shape"), list(b"om-s", &[stream])]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (
            Kind::Missing,
            Some("LIST:tdgp/ADBE Mask Shape/LIST:tdbs/LIST:list")
        )
    );
}

/// An `otst` wrapper: the orientation `stream`, then one `otda` per angle
/// triple in `angles`.
fn otst(stream: Vec<u8>, angles: &[[f64; 3]]) -> Vec<u8> {
    let otdas: Vec<Vec<u8>> = angles
        .iter()
        .map(|angles| {
            let bytes: Vec<u8> = angles.iter().flat_map(|a| a.to_be_bytes()).collect();
            chunk(b"otda", &bytes)
        })
        .collect();
    list(b"otst", &[stream, list(b"otky", &otdas)])
}

#[test]
fn reads_static_orientation_angles() {
    // The stream's own little-endian copy is not read; `otda` is.
    let stream = tdbs(&[tdb4(1, 0x06, true, false), cdat(&[0.0; 3])]);
    let body = group_body(
        1,
        "",
        &[
            tdmn("ADBE Orientation"),
            otst(stream, &[[10.0, 20.0, 30.0]]),
        ],
    );
    let group = read(&body, true).0.unwrap();
    assert_eq!(
        property(&group, 0).value,
        Stream::Static(Value::Vector3([10.0, 20.0, 30.0]))
    );
}

#[test]
fn reads_one_orientation_per_keyframe() {
    let stream = tdbs(&[
        tdb4(1, 0x06, true, false),
        keys(
            80,
            &[key_item(0, 2, 0, &[0.0; 9]), key_item(600, 2, 0, &[0.0; 9])],
        ),
    ]);
    let keyframes = animated(
        "ADBE Orientation",
        otst(stream, &[[0.0, 0.0, 0.0], [0.0, 90.0, 0.0]]),
    );
    let values: Vec<Value> = keyframes.into_iter().map(|key| key.value).collect();
    assert_eq!(
        values,
        [Value::Vector3([0.0; 3]), Value::Vector3([0.0, 90.0, 0.0])]
    );
}

#[test]
fn refuses_an_orientation_without_its_angles() {
    let stream = tdbs(&[tdb4(1, 0x06, true, false), cdat(&[0.0; 3])]);
    let wrapper = list(b"otst", &[stream]);
    let body = group_body(1, "", &[tdmn("ADBE Orientation"), wrapper]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::Missing, Some("LIST:tdgp/ADBE Orientation"))
    );
    let stream = tdbs(&[
        tdb4(1, 0x06, true, false),
        keys(80, &[key_item(0, 2, 0, &[0.0; 9])]),
    ]);
    let body = group_body(1, "", &[tdmn("ADBE Orientation"), otst(stream, &[])]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert!(
        error
            .expected
            .as_deref()
            .unwrap_or_default()
            .contains("otda"),
        "{error}"
    );
}

#[test]
fn text_document_errors_name_the_key_path_inside_the_body() {
    let text = list(
        b"btds",
        &[
            tdbs(&[tdb4(1, 0x01, true, false), cdat(&[0.0])]),
            chunk(b"LIST", b"btdk /0 << /1 ( >>"),
        ],
    );
    let body = group_body(1, "", &[tdmn("ADBE Text Document"), text]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(error.kind, Kind::Syntax);
    assert_eq!(
        error.path.as_deref(),
        Some("LIST:tdgp/ADBE Text Document/LIST:btdk")
    );
    let missing = list(
        b"btds",
        &[tdbs(&[tdb4(1, 0x01, true, false), cdat(&[0.0])])],
    );
    let body = group_body(1, "", &[tdmn("ADBE Text Document"), missing]);
    let error = read(&body, true).0.unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
}
