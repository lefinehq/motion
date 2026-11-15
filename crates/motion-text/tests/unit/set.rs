//! Unit tests for `src/set.rs`.

use std::path::PathBuf;

use motion_format::Kind;
use rstest::rstest;

use super::*;
use crate::test_support::with_names;

/// A fresh, empty folder under the system's temporary folder.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-text-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[rstest]
#[case::postscript(FontQuery::PostScript("InterVariable-Bold".into()), "InterVariable-Bold")]
#[case::family(
    FontQuery::Family { family: "Inter Variable".into(), weight: 700.0, slant: Slant::Normal },
    "InterVariable-Bold"
)]
#[case::family_any_case(
    FontQuery::Family { family: "inter variable".into(), weight: 300.0, slant: Slant::Italic },
    "InterVariableItalic-Light"
)]
fn finds_a_face_by_name_or_family(#[case] query: FontQuery, #[case] expected: &str) {
    assert_eq!(
        FontSet::builtin().find(&query).unwrap().postscript_name,
        expected
    );
}

#[rstest]
#[case::postscript(FontQuery::PostScript("Inter-Bold".into()))]
#[case::weight(FontQuery::Family { family: "Inter Variable".into(), weight: 650.0, slant: Slant::Normal })]
#[case::slant(FontQuery::Family { family: "Inter Variable".into(), weight: 400.0, slant: Slant::Oblique })]
#[case::empty_set(FontQuery::PostScript("InterVariable".into()))]
fn a_face_that_is_not_there_is_never_replaced(#[case] query: FontQuery) {
    let set = match &query {
        FontQuery::PostScript(name) if name == "InterVariable" => FontSet::new(),
        _ => FontSet::builtin(),
    };
    let error = set.find(&query).unwrap_err();
    assert_eq!(error.kind, Kind::UnknownName);
    assert!(error.message.contains(&query.to_string()));
}

#[test]
fn queries_print_as_their_parts() {
    let query = FontQuery::Family {
        family: "Lato".into(),
        weight: 900.0,
        slant: Slant::Italic,
    };
    assert_eq!(query.to_string(), "Lato 900 Italic");
    assert_eq!(
        FontQuery::PostScript("Lato-Black".into()).to_string(),
        "Lato-Black"
    );
}

#[test]
fn the_builtin_set_is_inter() {
    let set = FontSet::builtin();
    assert_eq!(set.faces().len(), 18);
    let mut own = FontSet::new();
    assert_eq!(
        own.add_bytes(BUILTIN[1].1.to_vec(), BUILTIN[1].0).unwrap(),
        9
    );
    assert_eq!(
        own.position(&FontQuery::PostScript("InterVariableItalic".into()))
            .unwrap(),
        3
    );
}

#[test]
fn a_folder_adds_its_fonts_in_path_order() {
    let dir = scratch("folder");
    std::fs::create_dir_all(dir.join("b")).unwrap();
    std::fs::write(dir.join("b/upright.TTF"), BUILTIN[0].1).unwrap();
    std::fs::write(dir.join("a-italic.ttf"), BUILTIN[1].1).unwrap();
    std::fs::write(dir.join("readme.txt"), "not a font").unwrap();
    std::fs::write(dir.join("noextension"), "not a font").unwrap();
    let mut set = FontSet::new();
    assert_eq!(set.add_folder(&dir).unwrap(), 18);
    assert_eq!(set.faces()[0].postscript_name, "InterVariableItalic-Thin");
    assert_eq!(set.faces()[9].postscript_name, "InterVariable-Thin");
    std::fs::write(dir.join("broken.otf"), "not a font").unwrap();
    let error = FontSet::new().add_folder(&dir).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_file_that_cannot_be_read_is_an_io_error() {
    let dir = scratch("missing");
    let mut set = FontSet::new();
    assert_eq!(
        set.add_file(&dir.join("gone.ttf")).unwrap_err().kind,
        Kind::Io
    );
    assert_eq!(
        set.add_folder(&dir.join("gone")).unwrap_err().kind,
        Kind::Io
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[rstest]
#[case("a.ttf", true)]
#[case("a.OTF", true)]
#[case("a.ttc", true)]
#[case("a.otc", true)]
#[case("a.woff2", false)]
#[case("ttf", false)]
fn font_files_are_known_by_extension(#[case] path: &str, #[case] font: bool) {
    assert_eq!(is_font_file(Path::new(path)), font);
}

/// A set of the fixture under each of `faces`, a PostScript name and a
/// family with `Regular` legacy style each, in that order.
fn named_set(faces: &[(&str, &str)]) -> FontSet {
    let mut set = FontSet::new();
    for (postscript, family) in faces {
        let bytes = with_names(&[(1, family), (2, "Regular"), (6, postscript)]);
        set.add_bytes(bytes, postscript).expect("a font");
    }
    set
}

#[rstest]
#[case::exactly("BrandSans-Regular", Some("BrandSans-Regular"))]
#[case::in_another_case("brandsans-regular", Some("BrandSans-Regular"))]
#[case::by_its_family("BrandSans", Some("BrandSans-Regular"))]
#[case::another_style("BrandSans-Bold", None)]
#[case::another_family("OtherSans", None)]
fn a_face_is_named_by_its_own_names(#[case] name: &str, #[case] face: Option<&str>) {
    let set = named_set(&[("BrandSans-Regular", "Brand Sans")]);
    assert_eq!(set.named(name).map(|f| f.postscript_name.as_str()), face);
}

#[test]
fn a_postscript_name_wins_over_an_earlier_face_that_answers_to_it() {
    let set = named_set(&[
        ("BrandSans-Regular", "Brand Sans"),
        ("Brand-Sans", "Brand Sans Old"),
        ("BrandSans", "Brand Sans Legacy"),
    ]);
    assert_eq!(set.named("BrandSans").unwrap().family, "Brand Sans Legacy");
    // Of the faces that only answer to a name, the first added.
    assert_eq!(
        set.named("brand sans").unwrap().postscript_name,
        "BrandSans-Regular"
    );
}
