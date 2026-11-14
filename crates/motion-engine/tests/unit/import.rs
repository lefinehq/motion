//! Unit tests for `src/import.rs`.

use tracing_subscriber::layer::SubscriberExt;

use super::*;
use crate::Kind;
use crate::telemetry::timing::Timings;
use crate::test_support::with_subscriber;

/// A chunk with `tag` and `body` (of even length).
fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = u32::try_from(body.len()).expect("a small body");
    [tag.as_slice(), &size.to_be_bytes(), body].concat()
}

/// The smallest project: a file version and an empty root folder.
fn empty_project() -> Vec<u8> {
    let head = chunk(
        b"head",
        &[0, 94, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let fold = chunk(b"LIST", b"Fold");
    chunk(b"RIFX", &[b"Egg!".as_slice(), &head, &fold].concat())
}

/// A file holding `bytes` in a fresh directory of its own.
fn scratch_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-engine-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("project.aep");
    fs::write(&path, bytes).expect("write project");
    path
}

#[test]
fn reads_a_project_in_the_read_and_decode_steps() {
    let path = scratch_file("import", &empty_project());
    let timings = Timings::default();
    let project = with_subscriber(tracing_subscriber::registry().with(timings.layer()), || {
        import(&path)
    })
    .expect("a project");
    assert_eq!(project.version.to_string(), "94.9");
    assert!(project.items.is_empty());
    let steps: Vec<String> = timings.steps().into_iter().map(|(name, _)| name).collect();
    assert_eq!(steps, ["read", "decode"]);
}

#[test]
fn reports_a_missing_file_and_a_file_that_is_not_a_project() {
    let missing = import("/no/such/project.aep").unwrap_err();
    assert_eq!(missing.kind, Kind::Io);
    let path = scratch_file("not-a-project", b"{\"motion\":1}");
    let error = import(&path).unwrap_err();
    assert_eq!(
        (error.kind, error.expected.as_deref()),
        (
            Kind::Invalid,
            Some("a file that starts with `RIFX` and `Egg!`")
        )
    );
}

/// A committed fixture whose one text layer is set in `BritannicBold`, a
/// font the tests are not given.
const TEXT_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../motion-import/tests/fixtures/Property-01.aep"
);

/// The fixture's first composition imported, without substitutes.
fn imported_text() -> Import {
    let project = import(TEXT_FIXTURE).expect("the fixture decodes");
    let comp = project.items.iter().find_map(|item| match item.kind {
        motion_import::ItemKind::Composition(_) => Some(item.name.clone()),
        _ => None,
    });
    let options = Options {
        comp,
        fonts: Vec::new(),
    };
    import_scene(TEXT_FIXTURE, &options).expect("the fixture imports")
}

#[test]
fn a_missing_font_is_substituted_reported_and_drawn() {
    let mut import = imported_text();
    let unset = motion_format::write_document(&import.json)
        .parse::<crate::Session>()
        .expect("a scene")
        .check()
        .unwrap_err();
    assert_eq!(unset.kind, Kind::UnknownName);
    substitute_fonts(&mut import, &FontSet::builtin()).expect("substituted");
    let layer = import.json["layers"]
        .as_array()
        .and_then(|layers| layers.iter().find(|l| l["text"].is_string()))
        .expect("a text layer");
    assert_eq!(layer["font"], "InterVariable-Bold 185");
    let what: Vec<&str> = import
        .mapping
        .unsupported
        .iter()
        .map(|finding| finding.what.as_str())
        .collect();
    assert_eq!(
        what,
        ["missing fonts, substituted (`BritannicBold → InterVariable-Bold`)"]
    );
    let session: crate::Session = motion_format::write_document(&import.json)
        .parse()
        .expect("a scene");
    session.check().expect("the text is set");
}

#[test]
fn a_font_the_set_has_is_kept() {
    let mut import = imported_text();
    let mut fonts = FontSet::builtin();
    fonts
        .add_bytes(
            include_bytes!("../../../motion-raster/tests/fixtures/FixtureSans-Black.ttf").to_vec(),
            "fixture",
        )
        .expect("a font");
    import
        .substitute_fonts(&BTreeMap::from([(
            "BritannicBold".to_owned(),
            "FixtureSans-Black".to_owned(),
        )]))
        .expect("substituted");
    let before = import.json.clone();
    substitute_fonts(&mut import, &fonts).expect("nothing to do");
    assert_eq!(import.json, before);
}
