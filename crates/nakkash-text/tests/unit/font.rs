//! Unit tests for `src/font.rs`.

use nakkash_format::Kind;
use rstest::rstest;

use super::*;
use crate::test_support::with_names;

fn builtin(index: usize) -> Vec<Face> {
    let bytes: Bytes = Arc::new(crate::set::BUILTIN[index].1);
    faces(bytes, crate::set::BUILTIN[index].0).unwrap()
}

#[test]
fn a_variable_font_offers_a_face_per_named_instance() {
    let upright = builtin(0);
    let names: Vec<&str> = upright.iter().map(|f| f.postscript_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "InterVariable-Thin",
            "InterVariable-ExtraLight",
            "InterVariable-Light",
            "InterVariable",
            "InterVariable-Medium",
            "InterVariable-SemiBold",
            "InterVariable-Bold",
            "InterVariable-ExtraBold",
            "InterVariable-Black",
        ]
    );
    // Every face shares the one copy of the bytes.
    assert!(upright.iter().all(|f| f.data == upright[0].data));
}

#[rstest]
#[case::thin(0, 0, 100.0, Slant::Normal)]
#[case::bold(0, 6, 700.0, Slant::Normal)]
#[case::italic(1, 3, 400.0, Slant::Italic)]
#[case::black_italic(1, 8, 900.0, Slant::Italic)]
fn faces_carry_their_weight_slant_and_axes(
    #[case] file: usize,
    #[case] index: usize,
    #[case] weight: f32,
    #[case] slant: Slant,
) {
    let face = &builtin(file)[index];
    assert_eq!(
        (face.family.as_str(), face.weight, face.slant),
        ("Inter Variable", weight, slant)
    );
    assert_eq!(
        face.variations,
        [(Tag::new(b"opsz"), 14.0), (Tag::new(b"wght"), weight)]
    );
}

#[rstest]
#[case::junk(b"nope".to_vec(), "an OpenType or TrueType font (`.ttf`, `.otf`, `.ttc`, `.otc`)")]
#[case::empty(
    Vec::new(),
    "an OpenType or TrueType font (`.ttf`, `.otf`, `.ttc`, `.otc`)"
)]
#[case::truncated(
    crate::set::BUILTIN[0].1[..64].to_vec(),
    "a font whose `name` table names its family and PostScript name"
)]
fn bytes_that_are_not_a_font_are_refused(#[case] bytes: Vec<u8>, #[case] expected: &str) {
    let error = faces(Arc::new(bytes), "x.ttf").unwrap_err();
    assert_eq!(
        (error.kind, error.expected.as_deref()),
        (Kind::Invalid, Some(expected))
    );
}

#[test]
fn a_face_without_names_is_refused() {
    let error = missing_name("x.ttf", "family");
    assert_eq!(error.kind, Kind::Invalid);
    assert!(error.message.contains("no family name"));
}

#[rstest]
#[case(skrifa::attribute::Style::Normal, Slant::Normal)]
#[case(skrifa::attribute::Style::Italic, Slant::Italic)]
#[case(skrifa::attribute::Style::Oblique(None), Slant::Oblique)]
fn styles_map_to_slants(#[case] style: skrifa::attribute::Style, #[case] expected: Slant) {
    assert_eq!(slant(style), expected);
}

/// The one face of the fixture under `names`.
fn named(names: &[(u16, &str)]) -> Face {
    let mut faces = faces(Arc::new(with_names(names)), "named.ttf").expect("a font");
    assert_eq!(faces.len(), 1);
    faces.remove(0)
}

/// A face whose PostScript name abbreviates its style, whose typographic
/// names part the style from the family, and whose legacy names fold the
/// weight into the family, as `Open Sans` 3's do.
fn extra_bold() -> Face {
    named(&[
        (1, "Brand Sans ExtraBold"),
        (2, "Regular"),
        (4, "Brand Sans ExtraBold"),
        (6, "BrandSans-ExtBd"),
        (16, "Brand Sans"),
        (17, "ExtraBold"),
    ])
}

/// A regular face with only legacy names: `IBM Plex Sans` as Google Fonts
/// builds it.
fn regular() -> Face {
    named(&[
        (1, "Brand Sans"),
        (2, "Regular"),
        (4, "Brand Sans Regular"),
        (6, "BrandSans-Regular"),
    ])
}

#[test]
fn a_face_reads_its_style_and_other_names_from_its_name_table() {
    let face = extra_bold();
    assert_eq!(
        (
            face.family.as_str(),
            face.style.as_str(),
            face.names.as_slice()
        ),
        (
            "Brand Sans",
            "ExtraBold",
            ["Brand Sans ExtraBold".to_owned()].as_slice()
        )
    );
    let face = regular();
    assert_eq!(
        (
            face.family.as_str(),
            face.style.as_str(),
            face.names.as_slice()
        ),
        (
            "Brand Sans",
            "Regular",
            ["Brand Sans".to_owned()].as_slice()
        )
    );
}

#[test]
fn a_named_instance_is_named_by_its_family_and_subfamily() {
    let upright = builtin(0);
    let names = |i: usize| (upright[i].style.as_str(), upright[i].names.clone());
    // `Inter Variable Bold` and, for the regular instance, the family alone
    // are their PostScript names but for spaces and hyphens, so neither is
    // another name.
    assert_eq!(names(6), ("Bold", Vec::new()));
    assert_eq!(names(3), ("Regular", Vec::new()));
    assert!(upright[6].answers_to("Inter Variable Bold"));
    assert!(upright[3].answers_to("Inter Variable"));
}

#[rstest]
#[case::its_postscript_name("BrandSans-ExtBd", true)]
#[case::its_family_and_style("BrandSans-ExtraBold", true)]
#[case::another_case("BrandSans-Extrabold", true)]
#[case::other_punctuation("brand_sans extrabold", true)]
#[case::its_full_name("Brand Sans ExtraBold", true)]
#[case::the_family_alone("BrandSans", false)]
#[case::another_style("BrandSans-Bold", false)]
#[case::nothing("", false)]
#[case::punctuation_only("-", false)]
fn a_face_answers_to_its_names_in_any_case_and_punctuation(
    #[case] name: &str,
    #[case] answers: bool,
) {
    assert_eq!(extra_bold().answers_to(name), answers);
}

#[rstest]
#[case::without_its_regular_style("BrandSans", true)]
#[case::with_it("Brand Sans Regular", true)]
#[case::bold("BrandSansBold", false)]
#[case::italic("BrandSans-Italic", false)]
fn a_regular_face_answers_to_its_family(#[case] name: &str, #[case] answers: bool) {
    assert_eq!(regular().answers_to(name), answers);
}

#[rstest]
#[case("Open Sans ExtraBold", "opensansextrabold")]
#[case("OpenSans-Extrabold", "opensansextrabold")]
#[case("Grotesk Ä-Bold", "groteskäbold")]
#[case("- _", "")]
fn names_are_compared_without_case_spaces_or_punctuation(#[case] name: &str, #[case] key: &str) {
    assert_eq!(name_key(name), key);
}
