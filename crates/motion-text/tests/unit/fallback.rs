//! Unit tests for `src/fallback.rs`: the style table, and the face chosen
//! for a font the set lacks.

use rstest::rstest;

use super::*;

/// The raster crate's fixture font, an OFL subset of Lato Black whose
/// PostScript name is `FixtureSans-Black`.
const FIXTURE: &[u8] =
    include_bytes!("../../../motion-raster/tests/fixtures/FixtureSans-Black.ttf");

/// The built-in faces and the fixture's.
fn with_fixture() -> FontSet {
    let mut set = FontSet::builtin();
    set.add_bytes(FIXTURE.to_vec(), "FixtureSans-Black.ttf")
        .expect("the fixture is a font");
    set
}

#[rstest]
#[case::bold("Brand-Bold", "Brand", 700.0, false)]
#[case::semibold_italic("Brand-SemiBoldItalic", "Brand", 600.0, true)]
#[case::any_case("Brand-Semibold", "Brand", 600.0, false)]
#[case::longest_suffix("Brand-ExtraBold", "Brand", 800.0, false)]
#[case::black("Brand-Black", "Brand", 900.0, false)]
#[case::light("Brand-Light", "Brand", 300.0, false)]
#[case::italic("Brand-Italic", "Brand", 400.0, true)]
#[case::vendor_tag("Brand-BoldItalicMT", "Brand", 700.0, true)]
#[case::no_hyphen("BrandBold", "Brand", 700.0, false)]
#[case::no_suffix("Brand", "Brand", 400.0, false)]
#[case::unknown_suffix("Brand-Wide", "Brand", 400.0, false)]
fn a_postscript_name_says_its_family_weight_and_slant(
    #[case] name: &str,
    #[case] family: &str,
    #[case] weight: f32,
    #[case] italic: bool,
) {
    assert_eq!(
        Named::read(name),
        Named {
            family,
            weight,
            italic
        }
    );
}

#[test]
fn every_suffix_is_listed_once_with_a_weight() {
    let mut seen = std::collections::BTreeSet::new();
    for (suffix, weight, _) in STYLES {
        assert!(
            seen.insert(suffix.to_ascii_lowercase()),
            "{suffix} is listed twice"
        );
        assert!((100.0..=900.0).contains(weight), "{suffix}: {weight}");
    }
}

#[rstest]
#[case::same_weight("Brand-SemiBold", "InterVariable-SemiBold")]
#[case::same_slant("Brand-LightItalic", "InterVariableItalic-Light")]
#[case::regular("Brand", "InterVariable")]
#[case::builtin_by_its_own_name("Inter-Bold", "InterVariable-Bold")]
#[case::same_family("FixtureSans-Bold", "FixtureSans-Black")]
#[case::same_family_over_slant("FixtureSans-Italic", "FixtureSans-Black")]
#[case::family_by_name_without_style("FixtureSans", "FixtureSans-Black")]
fn a_missing_font_takes_the_closest_face(#[case] missing: &str, #[case] chosen: &str) {
    let set = with_fixture();
    assert_eq!(
        set.fallback(missing).map(|f| f.postscript_name.as_str()),
        Some(chosen)
    );
}

#[test]
fn a_weight_between_two_takes_the_lighter() {
    let builtin = FontSet::builtin();
    let pair = builtin.faces().iter().filter(|f| {
        ["InterVariable-Medium", "InterVariable-Bold"].contains(&f.postscript_name.as_str())
    });
    let chosen = closest(pair, &Named::read("Brand-SemiBold"));
    assert_eq!(
        chosen.map(|f| f.postscript_name.as_str()),
        Some("InterVariable-Medium")
    );
}

#[test]
fn a_set_without_the_family_or_inter_has_no_fallback() {
    assert!(FontSet::new().fallback("Brand-Bold").is_none());
}
