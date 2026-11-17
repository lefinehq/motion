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
        ..Default::default()
    };
    import_scene(TEXT_FIXTURE, &options, None).expect("the fixture imports")
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

#[test]
fn a_font_named_by_another_name_of_a_face_is_renamed_not_substituted() {
    let mut import = imported_text();
    let mut fonts = FontSet::builtin();
    fonts
        .add_bytes(
            include_bytes!("../../../motion-raster/tests/fixtures/FixtureSans-Black.ttf").to_vec(),
            "fixture",
        )
        .expect("a font");
    // The project's font as an earlier build of the fixture might name it.
    import
        .rename_fonts(&BTreeMap::from([(
            "BritannicBold".to_owned(),
            "FixtureSans-black".to_owned(),
        )]))
        .expect("renamed");
    import.mapping.renamed_fonts.clear();
    substitute_fonts(&mut import, &fonts).expect("renamed");
    let layer = import.json["layers"]
        .as_array()
        .and_then(|layers| layers.iter().find(|l| l["text"].is_string()))
        .expect("a text layer");
    assert_eq!(layer["font"], "FixtureSans-Black 185");
    assert!(import.mapping.unsupported.is_empty());
    assert_eq!(
        import.mapping.renamed_fonts,
        BTreeMap::from([(
            "FixtureSans-black".to_owned(),
            "FixtureSans-Black".to_owned()
        )])
    );
}

/// A committed fixture with one still footage layer, `Heart-80.png`, whose
/// stored path is on its author's machine, under `…/assets/Images/`.
const FOOTAGE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../motion-import/tests/fixtures/Benchmark.aep"
);

/// The footage fixture copied into a fresh folder, with an 80x80 picture
/// at each of `pictures` beside it; returns the copy's path.
fn footage_project(name: &str, pictures: &[&str]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "motion-engine-footage-import-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("template/Benchmark.aep");
    fs::create_dir_all(project.parent().unwrap()).expect("create the project folder");
    fs::copy(FOOTAGE_FIXTURE, &project).expect("copy the fixture");
    let png = motion_raster::Image::new(80, 80, [200, 20, 60, 255].repeat(80 * 80))
        .unwrap()
        .to_png()
        .unwrap();
    for picture in pictures {
        let path = project.with_file_name(picture);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &png).unwrap();
    }
    project
}

/// The imported layer whose id is `id`.
fn layer<'j>(import: &'j Import, id: &str) -> &'j serde_json::Value {
    import.json["layers"]
        .as_array()
        .and_then(|layers| layers.iter().find(|l| l["id"] == id))
        .expect("the layer")
}

#[test]
fn still_footage_found_beside_the_project_becomes_an_image_layer() {
    // A copy elsewhere shares less of the stored path, so it loses.
    let project = footage_project("found", &["Images/Heart-80.png", "spare/Heart-80.png"]);
    let scene = project
        .parent()
        .unwrap()
        .with_file_name("out")
        .join("scene.json");
    fs::create_dir_all(scene.parent().unwrap()).unwrap();
    let mut import = import_scene(&project, &Options::default(), Some(&scene)).expect("imports");
    let heart = layer(&import, "heart-80-png");
    assert_eq!(heart["image"], "../template/Images/Heart-80.png");
    assert_eq!(heart["box"], serde_json::json!([80, 80]));
    assert!(heart.get("pivot").is_none());
    assert_eq!(import.mapping.counts["footage: image"].mapped, 1);
    assert!(
        import
            .mapping
            .unsupported
            .iter()
            .all(|finding| !finding.what.contains("footage")),
        "{:?}",
        import.mapping.unsupported
    );
    // Saved where it was meant to be, the scene draws the picture.
    substitute_fonts(&mut import, &FontSet::builtin()).expect("substituted");
    fs::write(&scene, motion_format::write_document(&import.json)).unwrap();
    let session = crate::Session::open(&scene).expect("a scene");
    let frame = session.display_list(0.0).expect("a frame");
    let drawn = frame.nodes.iter().any(|node| {
        matches!(node, motion_raster::Node::Layer(layer)
            if matches!(layer.children.as_slice(), [motion_raster::Node::Image(image)]
                if (image.picture.width(), image.picture.height()) == (80, 80)))
    });
    assert!(drawn, "{frame:?}");
}

#[test]
fn still_footage_that_cannot_be_found_is_reported_with_its_file() {
    let project = footage_project("missing", &[]);
    let import = import_scene(&project, &Options::default(), None).expect("imports");
    assert_eq!(layer(&import, "heart-80-png")["pivot"], true);
    let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
    assert!(
        what.contains(&"image footage it cannot find (drawn as nothing)"),
        "{what:?}"
    );
}

#[test]
fn vector_footage_becomes_an_image_layer_reporting_what_it_cannot_draw() {
    // The fixture's 80x80 still, as vector footage under its picture's name.
    let project = footage_project("vector", &[]);
    let file = project.with_file_name("Images/Heart-80.png");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let heart = crate::test_support::pdf(
        80,
        80,
        "",
        "/ExtGState << /G0 << /OP true >> >>",
        "/G0 gs 1 0 0 rg 10 10 60 60 re f",
    );
    fs::write(&file, heart).unwrap();
    let import = import_scene(&project, &Options::default(), None).expect("imports");
    let layer = layer(&import, "heart-80-png");
    assert!(layer["image"].as_str().unwrap().ends_with("Heart-80.png"));
    assert_eq!(layer["box"], serde_json::json!([80, 80]));
    // The page is the footage's size: no crop, and drawn into a picture.
    assert!(layer.get("crop").is_none() && layer.get("continuous").is_none());
    let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
    assert!(
        what.contains(&"vector footage: overprinting (drawn as if off)"),
        "{what:?}"
    );
}

#[test]
fn vector_footage_that_cannot_be_read_is_reported_by_its_format() {
    let project = footage_project("vector-broken", &[]);
    let file = project.with_file_name("Images/Heart-80.png");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "%PDF-1.7\nno objects").unwrap();
    let import = import_scene(&project, &Options::default(), None).expect("imports");
    assert_eq!(layer(&import, "heart-80-png")["pivot"], true);
    let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
    assert!(
        what.contains(&"vector footage in PDF it cannot read (drawn as nothing)"),
        "{what:?}"
    );
}

/// An import of the scene `json`, as the map writes one.
fn import_of(json: &str) -> Import {
    let json: serde_json::Value = serde_json::from_str(json).expect("json");
    Import {
        document: motion_format::parse_document(&json.to_string()).expect("a scene"),
        json,
        mapping: motion_import::map::Mapping::default(),
        texts: Vec::new(),
        fonts: std::collections::BTreeMap::new(),
        sources: motion_import::map::Sources::default(),
        unmeasured: Vec::new(),
    }
}

/// An ask for layer `layer` of the comp with item id 1, at `time`.
fn unmeasured(layer: u32, time: f64, at: Option<&str>, settled: bool) -> Unmeasured {
    Unmeasured {
        ask: motion_import::bake::Ask {
            comp: 1,
            layer,
            time,
            extents: false,
        },
        layer: at.map(|id| motion_format::Address::written(Some("card"), id.parse().unwrap())),
        settled,
    }
}

#[test]
fn a_pass_measures_settled_layers_first() {
    let asks = [
        unmeasured(1, 0.0, Some("a"), false),
        unmeasured(2, 0.0, Some("b"), true),
    ];
    let picked: Vec<u32> = to_measure(&asks).iter().map(|u| u.ask.layer).collect();
    assert_eq!(picked, [2]);
    let picked: Vec<u32> = to_measure(&asks[..1]).iter().map(|u| u.ask.layer).collect();
    assert_eq!(picked, [1], "when none has settled, each as it draws now");
}

#[test]
fn rectangles_are_measured_in_the_scene_the_import_wrote() {
    let import = import_of(
        r#"{"motion":1,"dur":"1s","fps":10,
            "comps":{"card":{"size":"200x100","dur":"1s","layers":[
              {"id":"title","text":"Hey"},{"id":"plate","rect":"30x20"}]}},
            "layers":[{"id":"c","comp":"card"}]}"#,
    );
    let asks = [
        unmeasured(1, 0.5, Some("title"), true),
        unmeasured(2, 0.5, Some("plate"), true),
        unmeasured(3, 0.5, None, true),
        unmeasured(4, 0.5, Some("gone"), true),
    ];
    let mut layouts = motion_import::bake::Layouts::default();
    measure(
        &import,
        &asks.iter().collect::<Vec<_>>(),
        &FontSet::builtin(),
        &mut layouts,
    );
    let title = layouts.get(&asks[0].ask).unwrap().clone().unwrap();
    assert!(
        title[0] > 0.0 && title[1] < 0.0 && title[2] > 20.0 && title[3] > 10.0,
        "{title:?}"
    );
    assert_eq!(layouts.get(&asks[1].ask), Some(&Ok([0.0, 0.0, 30.0, 20.0])));
    assert_eq!(
        layouts.get(&asks[2].ask),
        Some(&Err("the scene leaves the layer out".into()))
    );
    assert_eq!(
        layouts.get(&asks[3].ask),
        Some(&Err("the scene has no such layer".into()))
    );
}

#[test]
fn nothing_unmeasured_imports_nothing_again() {
    let mut import = import_of(r#"{"motion":1,"dur":"1s","layers":[]}"#);
    let before = import.json.clone();
    measure_layouts(
        "no such project.aep",
        &Options::default(),
        None,
        &mut import,
        &FontSet::builtin(),
    )
    .expect("the project is not read");
    assert_eq!(import.json, before);
}
