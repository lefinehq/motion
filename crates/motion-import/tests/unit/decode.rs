//! Unit tests for `src/decode.rs`, on hand-built projects.

use super::*;
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
