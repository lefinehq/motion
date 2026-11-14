//! Unit tests for `src/decode.rs`, on hand-built projects.

use pretty_assertions::assert_eq;

use super::*;
use crate::model::{BlendMode, LayerKind, TrackMatte};
use crate::test_support::{cdta, chunk, item, list, project, record};

/// A `LIST:Pin` whose `sspc` has `code` and a 100 x 50 size, then `rest`.
fn pin(code: &[u8; 4], rest: &[Vec<u8>]) -> Vec<u8> {
    let sspc = record(
        222,
        &[
            (0x16, code),
            (0x1E, &100u32.to_be_bytes()),
            (0x22, &50u32.to_be_bytes()),
            (0x88, &[0, 0, 0, 1, 0, 0, 0, 1]),
        ],
    );
    list(b"Pin ", &[&[chunk(b"sspc", &sspc)], rest].concat())
}

/// An `opti` whose `fields` are followed by the NUL-padded `name`.
fn opti(fields: &[u8], name: &str) -> Vec<u8> {
    chunk(b"opti", &[fields, name.as_bytes(), &[0; 8]].concat())
}

/// A solid's `opti`: kind, version, length, then alpha, red, green, blue.
fn solid_opti(name: &str) -> Vec<u8> {
    let argb: Vec<u8> = [1.0f32, 1.0, 0.5, 0.0]
        .iter()
        .flat_map(|c| c.to_be_bytes())
        .collect();
    opti(&[b"Soli".as_slice(), &[0; 6], &argb].concat(), name)
}

fn decoded(bytes: &[u8]) -> Project {
    decode(bytes).expect("a valid project")
}

#[test]
fn reads_the_version_and_the_colour_depth() {
    let project = decoded(&project(93, &[]));
    assert_eq!(
        project.version,
        Version {
            major: 93,
            minor: 0
        }
    );
    assert_eq!(project.bits_per_channel, Some(16));
    assert!(project.items.is_empty());
}

#[test]
fn reads_folders_and_names_their_contents_folder() {
    let comp = item(Idta::COMPOSITION, 2, "Main", &[cdta(1920, 1080, 30, 4)]);
    let folder = item(Idta::FOLDER, 1, "Scenes", &[list(b"Sfdr", &[comp])]);
    let project = decoded(&project(94, &[folder, item(Idta::FOLDER, 3, "", &[])]));
    let summary: Vec<(u32, &str, Option<u32>)> = project
        .items
        .iter()
        .map(|i| (i.id, i.name.as_str(), i.folder))
        .collect();
    assert_eq!(
        summary,
        [(1, "Scenes", None), (2, "Main", Some(1)), (3, "", None)]
    );
}

#[test]
fn reads_a_composition() {
    let project = decoded(&project(
        94,
        &[item(
            Idta::COMPOSITION,
            2,
            "Main",
            &[cdta(640, 360, 24, 10)],
        )],
    ));
    let ItemKind::Composition(comp) = &project.items[0].kind else {
        panic!("a composition")
    };
    assert_eq!((comp.width, comp.height, comp.frame_rate), (640, 360, 24.0));
    assert_eq!((comp.duration, comp.pixel_aspect), (10.0, 1.0));
    assert_eq!(
        comp.work_area,
        Span {
            start: 0.0,
            end: 10.0
        }
    );
    assert_eq!(
        comp.background,
        Color {
            r: 1.0,
            g: 0.0,
            b: 0.2,
            a: 1.0
        }
    );
}

#[test]
fn names_footage_after_its_solid_placeholder_or_file() {
    let alias = br#"{"fullpath":"C:\\art\\logo.png","platform":1}"#;
    let items = [
        item(
            Idta::FOOTAGE,
            4,
            "",
            &[pin(b"Soli", &[solid_opti("Red Solid")])],
        ),
        item(
            Idta::FOOTAGE,
            5,
            "",
            &[pin(&[0; 4], &[opti(&[0; 10], "Missing")])],
        ),
        item(
            Idta::FOOTAGE,
            6,
            "",
            &[pin(b"png!", &[list(b"Als2", &[chunk(b"alas", alias)])])],
        ),
        item(
            Idta::FOOTAGE,
            7,
            "Renamed",
            &[pin(b"png!", &[list(b"Als2", &[chunk(b"alas", b"{")])])],
        ),
    ];
    let project = decoded(&project(94, &items));
    let names: Vec<&str> = project.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Red Solid", "Missing", "logo.png", "Renamed"]);
    let sources: Vec<&Source> = project
        .items
        .iter()
        .map(|i| match &i.kind {
            ItemKind::Footage(footage) => &footage.source,
            _ => panic!("footage"),
        })
        .collect();
    assert_eq!(
        sources,
        [
            &Source::Solid {
                color: Color {
                    r: 1.0,
                    g: 0.5,
                    b: 0.0,
                    a: 1.0
                }
            },
            &Source::Placeholder,
            &Source::File {
                path: Some("C:\\art\\logo.png".into())
            },
            &Source::File { path: None },
        ]
    );
    let ItemKind::Footage(footage) = &project.items[0].kind else {
        panic!("footage")
    };
    assert_eq!(
        (footage.width, footage.height, footage.pixel_aspect),
        (100, 50, 1.0)
    );
}

#[test]
fn counts_the_chunks_it_passes_over() {
    let comp = item(
        Idta::COMPOSITION,
        2,
        "Main",
        &[chunk(b"cdrp", &[0]), cdta(8, 8, 24, 1)],
    );
    let project = decoded(&project(
        94,
        &[comp, chunk(b"fvdv", &[0; 4]), item(9, 3, "?", &[])],
    ));
    let skipped: Vec<(&str, usize)> = project
        .skipped
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();
    assert_eq!(
        skipped,
        [("LIST:Item", 1), ("cdrp", 1), ("fdta", 1), ("fvdv", 1)]
    );
}

#[test]
fn names_the_chunk_path_of_a_record_too_short() {
    let comp = item(Idta::COMPOSITION, 2, "Main", &[chunk(b"cdta", &[0; 10])]);
    let error = decode(&project(94, &[item(Idta::FOLDER, 1, "", &[]), comp])).unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref(), error.expected.as_deref()),
        (
            Kind::Invalid,
            Some("LIST:Fold/LIST:Item[1]/cdta"),
            Some("a whole `cdta` record")
        )
    );
}

#[test]
fn refuses_a_project_without_a_required_chunk() {
    let error = decode(&crate::test_support::file(&[])).unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref(), error.expected.as_deref()),
        (Kind::Missing, Some("RIFX"), Some("a `head` chunk"))
    );
    let footage = item(Idta::FOOTAGE, 4, "", &[]);
    let error = decode(&project(94, &[footage])).unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::Missing, Some("LIST:Fold/LIST:Item[0]"))
    );
}

#[test]
fn refuses_lists_nested_too_deep() {
    let nested = (0..MAX_DEPTH).fold(Vec::new(), |inner, id| {
        item(Idta::FOLDER, id as u32, "", &[list(b"Sfdr", &[inner])])
    });
    let error = decode(&project(94, &[nested])).unwrap_err();
    assert_eq!(
        (error.kind, error.expected.as_deref()),
        (Kind::Invalid, Some("a shallower project"))
    );
}

/// A layer list: an `ldta` of `size` bytes with `id`, `kind`, `source` and
/// `flags`, 200 % stretch, starting at 1 s, in at 0.5 s and out at 2 s of
/// layer time, then `name` and an empty property tree.
fn layer(size: usize, id: u32, kind: u8, source: u32, flags: [u8; 3], name: &str) -> Vec<u8> {
    let ldta = record(
        size,
        &[
            (0x00, &id.to_be_bytes()),
            (0x08, &2i32.to_be_bytes()),
            (0x0C, &[0, 0, 2, 88, 0, 0, 2, 88]),
            (0x14, &[0, 0, 1, 44, 0, 0, 2, 88]),
            (0x1C, &[0, 0, 4, 176, 0, 0, 2, 88]),
            (0x25, &flags),
            (0x28, &source.to_be_bytes()),
            (0x63, &[5]),
            (0x6B, &[3]),
            (0x6C, &1u32.to_be_bytes()),
            (0x83, &[kind]),
            (0x84, &id.saturating_sub(1).to_be_bytes()),
            (size - 4, &(size as u32).to_be_bytes()),
        ],
    );
    list(
        b"Layr",
        &[
            chunk(b"ldta", &ldta),
            chunk(b"Utf8", name.as_bytes()),
            list(b"tdgp", &[]),
        ],
    )
}

/// The layers of the composition that is the first item of `project`.
fn layers(project: &Project) -> &[Layer] {
    match project.items.first().map(|item| &item.kind) {
        Some(ItemKind::Composition(composition)) => &composition.layers,
        _ => panic!("a composition first"),
    }
}

#[test]
fn reads_layers_with_their_timing_switches_and_links() {
    let comp = item(
        Idta::COMPOSITION,
        1,
        "Main",
        &[
            cdta(100, 100, 24, 4),
            layer(164, 10, 4, 0, [0x02, 0x8C, 0x41], "Shapes"),
            layer(160, 11, 0, 2, [0, 0, 0x01], ""),
        ],
    );
    let solid = item(Idta::FOOTAGE, 2, "", &[pin(b"Soli", &[solid_opti("Grey")])]);
    let project = decoded(&project(94, &[comp, solid]));
    let [shapes, grey] = layers(&project) else {
        panic!("two layers")
    };
    assert_eq!(
        (shapes.id, shapes.name.as_str(), shapes.kind),
        (10, "Shapes", LayerKind::Shape)
    );
    assert_eq!(
        (shapes.source, shapes.parent, shapes.matte_layer),
        (None, Some(9), Some(164))
    );
    assert_eq!(
        (
            shapes.start_time,
            shapes.stretch,
            shapes.in_point,
            shapes.out_point
        ),
        (1.0, 2.0, 2.0, 5.0)
    );
    assert_eq!(
        (shapes.blend_mode, shapes.track_matte),
        (BlendMode::Multiply, TrackMatte::Luma)
    );
    assert_eq!(
        shapes.switches,
        Switches {
            enabled: true,
            three_d: true,
            solo: true,
            shy: true,
            guide: true,
            null: true,
            ..Switches::default()
        }
    );
    assert_eq!(
        (grey.name.as_str(), grey.source, grey.matte_layer),
        ("Grey", Some(2), None)
    );
    assert_eq!(grey.kind, LayerKind::Av);
}

#[test]
fn keeps_codes_it_does_not_know() {
    assert_eq!(BlendMode::from(99), BlendMode::Other(99));
    assert_eq!(BlendMode::from(0), BlendMode::Normal);
    assert_eq!(LayerKind::from(6), LayerKind::Other(6));
}
