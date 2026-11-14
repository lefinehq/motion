//! Unit tests for `src/font.rs`.

use motion_format::Kind;
use rstest::rstest;

use super::*;

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
