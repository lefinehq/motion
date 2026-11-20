//! Unit tests for `src/composite.rs`.

use nakkash_format::Kind;
use rstest::rstest;

use super::*;
use crate::blend::BLEND_MODES;

fn close(a: Pixel, b: Pixel) {
    for c in 0..4 {
        assert!((a[c] - b[c]).abs() < 1e-5, "{a:?} against {b:?}");
    }
}

/// `op` of `over` on `under` at the grid's first pixel.
fn at(op: &str, under: Pixel, over: Pixel) -> Pixel {
    Operation::named(op).unwrap().pixel(under, over, [0, 0])
}

const GREY: Pixel = [0.5, 0.5, 0.5, 1.0];
const CLEAR: Pixel = [0.0; 4];

#[test]
fn every_blend_mode_is_an_operation_of_its_own_name() {
    for spec in BLEND_MODES {
        assert_eq!(
            Operation::named(spec.name).unwrap(),
            Operation::Blend(spec.mode)
        );
        assert_eq!(Operation::Blend(spec.mode).name(), spec.name);
    }
}

#[test]
fn names_are_unique_and_round_trip() {
    let mut names: Vec<&str> = OPERATIONS.iter().map(|s| s.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), OPERATIONS.len());
    for spec in OPERATIONS {
        assert_eq!(Operation::named(spec.name).unwrap(), spec.operation);
    }
    // `over` is the generators' older name for normal; normal names it.
    assert_eq!(Operation::Blend(BlendMode::Normal).name(), "normal");
    assert_eq!(Operation::Replace.name(), "replace");
}

#[test]
fn an_unknown_operation_lists_every_name() {
    let e = Operation::named("glow").unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    let expected = e.expected.unwrap();
    for spec in OPERATIONS {
        assert!(expected.contains(spec.name), "{}", spec.name);
    }
}

#[rstest]
#[case::normal("normal", [0.2, 0.2, 0.2, 1.0])]
#[case::add("add", [0.7, 0.7, 0.7, 1.0])]
#[case::multiply("multiply", [0.1, 0.1, 0.1, 1.0])]
#[case::screen("screen", [0.6, 0.6, 0.6, 1.0])]
#[case::darken("darken", [0.2, 0.2, 0.2, 1.0])]
#[case::lighten("lighten", [0.5, 0.5, 0.5, 1.0])]
#[case::difference("difference", [0.3, 0.3, 0.3, 1.0])]
#[case::exclusion("exclusion", [0.5, 0.5, 0.5, 1.0])]
// Overlay is hard light with the sides swapped: grey under it is the
// switch-over, so it multiplies by twice the grey.
#[case::overlay("overlay", [0.2, 0.2, 0.2, 1.0])]
#[case::hard_light("hard-light", [0.2, 0.2, 0.2, 1.0])]
#[case::color_dodge("color-dodge", [0.625, 0.625, 0.625, 1.0])]
#[case::color_burn("color-burn", [0.0, 0.0, 0.0, 1.0])]
#[case::replace("replace", [0.2, 0.2, 0.2, 1.0])]
fn opaque_greys_blend_by_the_mode(#[case] op: &str, #[case] want: Pixel) {
    close(at(op, GREY, [0.2, 0.2, 0.2, 1.0]), want);
}

#[test]
fn soft_light_lifts_or_lowers_by_the_colour_above() {
    // Above grey it lightens by the square root, below it darkens.
    close(
        at("soft-light", [0.25, 0.25, 0.25, 1.0], [1.0, 1.0, 1.0, 1.0]),
        [0.5, 0.5, 0.5, 1.0],
    );
    close(
        at("soft-light", [0.5, 0.5, 0.5, 1.0], [0.0, 0.0, 0.0, 1.0]),
        [0.25, 0.25, 0.25, 1.0],
    );
}

#[test]
fn where_one_side_is_clear_the_other_shows() {
    let red = [0.5, 0.0, 0.0, 0.5];
    for spec in OPERATIONS {
        // A stencil keeps nothing under a clear result: it cuts what is
        // under it everywhere it is not.
        let Operation::Blend(mode) = spec.operation else {
            continue;
        };
        if mode.keeps_below() {
            continue;
        }
        close(at(spec.name, CLEAR, red), red);
        close(at(spec.name, red, CLEAR), red);
    }
}

#[test]
fn screen_of_premultiplied_pixels_is_their_sum_less_their_product() {
    let (under, over) = ([0.4, 0.1, 0.0, 0.5], [0.2, 0.2, 0.2, 0.25]);
    let want = std::array::from_fn(|c| under[c] + over[c] - under[c] * over[c]);
    close(at("screen", under, over), want);
}

#[test]
fn add_sums_premultiplied_pixels_held_to_their_alpha() {
    close(
        at("add", [0.6, 0.2, 0.0, 0.6], [0.6, 0.6, 0.0, 0.6]),
        [1.0, 0.8, 0.0, 1.0],
    );
    close(
        at("add", [0.2, 0.0, 0.0, 0.2], [0.0, 0.0, 0.3, 0.3]),
        [0.2, 0.0, 0.3, 0.5],
    );
}

#[test]
fn the_non_separable_modes_trade_hue_saturation_and_luminosity() {
    let red = [1.0, 0.0, 0.0, 1.0];
    // Grey has no saturation: hue and saturation from red over it keep it
    // grey, colour takes red's hue at grey's luminosity.
    close(at("hue", GREY, red), GREY);
    close(at("saturation", red, GREY), [0.3, 0.3, 0.3, 1.0]);
    let coloured = at("color", GREY, red);
    assert!(coloured[0] > coloured[1] && coloured[1] == coloured[2]);
    let lum = 0.3 * coloured[0] + 0.59 * coloured[1] + 0.11 * coloured[2];
    assert!((lum - 0.5).abs() < 1e-5, "{coloured:?}");
    // Luminosity keeps red's hue at the luminosity of grey.
    let lit = at("luminosity", red, GREY);
    assert!(lit[0] > lit[1] && lit[1] == lit[2]);
}

#[test]
fn stencils_keep_and_silhouettes_cut_what_is_under() {
    let half = [0.0, 0.0, 0.0, 0.5];
    let white = [1.0, 1.0, 1.0, 1.0];
    close(at("stencil-alpha", GREY, half), [0.25, 0.25, 0.25, 0.5]);
    close(at("silhouette-alpha", GREY, half), [0.25, 0.25, 0.25, 0.5]);
    // Black at half alpha has no luma: the stencil hides all, the
    // silhouette nothing.
    close(at("stencil-luma", GREY, half), CLEAR);
    close(at("silhouette-luma", GREY, half), GREY);
    close(at("stencil-luma", GREY, white), GREY);
    close(at("silhouette-luma", GREY, white), CLEAR);
}

#[test]
fn alpha_add_closes_two_edges_that_share_a_pixel() {
    // Two halves of a pixel's coverage add to all of it, each its colour.
    close(
        at("alpha-add", [0.5, 0.0, 0.0, 0.5], [0.0, 0.0, 0.5, 0.5]),
        [0.5, 0.0, 0.5, 1.0],
    );
    // Over an opaque pixel the result covers what it can, as over does.
    close(
        at("alpha-add", GREY, [0.2, 0.2, 0.2, 1.0]),
        [0.2, 0.2, 0.2, 1.0],
    );
    close(
        at("alpha-add", GREY, [0.1, 0.1, 0.1, 0.5]),
        at("normal", GREY, [0.1, 0.1, 0.1, 0.5]),
    );
}

#[test]
fn premultiplied_over_adds_light_past_its_alpha() {
    // Light at 0.8 with alpha 0.2 adds 0.8 over the 0.8 left of the grey.
    close(
        at("premultiplied-over", GREY, [0.8, 0.8, 0.8, 0.2]),
        [1.0, 1.0, 1.0, 1.0],
    );
    // Held to its alpha first, over adds only 0.2.
    close(
        at("normal", GREY, [0.8, 0.8, 0.8, 0.2]),
        [0.6, 0.6, 0.6, 1.0],
    );
}

#[test]
fn dissolve_shows_the_result_at_a_share_of_pixels_as_large_as_its_alpha() {
    let under = Pixels::transparent(100, 100);
    let mut over = Pixels::transparent(100, 100);
    over.data_mut().fill([0.3, 0.0, 0.0, 0.3]);
    let mut out = under.clone();
    Operation::Dissolve.pixels(&mut out, &over);
    let shown = out.data().iter().filter(|p| p[3] == 1.0).count();
    assert!((2_800..3_200).contains(&shown), "{shown}");
    // Every shown pixel is the result's straight colour, opaque; the rest
    // are what was under.
    assert!(
        out.data()
            .iter()
            .all(|p| *p == [1.0, 0.0, 0.0, 1.0] || *p == CLEAR)
    );
    // The pattern stays put from one run to the next.
    let mut again = under.clone();
    Operation::Dissolve.pixels(&mut again, &over);
    assert_eq!(out, again);
}

#[test]
fn pixels_combine_each_pixel_with_the_one_under_it() {
    let mut under = Pixels::transparent(2, 1);
    under.data_mut().fill(GREY);
    let mut over = Pixels::transparent(2, 1);
    over.data_mut()[1] = [0.2, 0.2, 0.2, 1.0];
    Operation::named("multiply")
        .unwrap()
        .pixels(&mut under, &over);
    close(under.get(0, 0), GREY);
    close(under.get(1, 0), [0.1, 0.1, 0.1, 1.0]);
}

#[test]
fn a_blend_lays_as_the_float_canvas_lays_a_layer() {
    let (under, over) = ([0.3, 0.2, 0.1, 0.6], [0.1, 0.4, 0.2, 0.5]);
    for spec in BLEND_MODES {
        close(
            Operation::Blend(spec.mode).pixel(under, over, [0, 0]),
            spec.mode.lay(under, over),
        );
    }
}

/// A 4 by 1 grid: clear, half-covered red, opaque grey, and clear again
/// past the layer's box, which covers the first three pixels.
fn layer() -> (Pixels, Space) {
    let mut pixels = Pixels::transparent(4, 1);
    pixels.data_mut()[1] = [0.5, 0.0, 0.0, 0.5];
    pixels.data_mut()[2] = GREY;
    let space = Space {
        transform: vello_cpu::kurbo::Affine::IDENTITY,
        bounds: Some(vello_cpu::kurbo::Rect::new(0.0, 0.0, 3.0, 1.0)),
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
    };
    (pixels, space)
}

fn solid(source: f32, color: [f32; 3], opacity: f32, mode: &str) -> Pixels {
    let (mut pixels, space) = layer();
    let at = SOLID_COMPOSITE_MODES
        .iter()
        .position(|m| *m == mode)
        .unwrap();
    let values = [
        ParamValue::Number(source),
        ParamValue::Color(crate::style::Color::rgba(color[0], color[1], color[2], 0.0)),
        ParamValue::Number(opacity),
        ParamValue::Choice(u8::try_from(at).unwrap()),
    ];
    solid_composite(&mut pixels, &values, &space);
    pixels
}

#[test]
fn every_solid_composite_mode_is_an_operation() {
    assert_eq!(SOLID_COMPOSITE_MODES[0], "normal");
    for name in SOLID_COMPOSITE_MODES {
        assert!(Operation::named(name).is_ok(), "{name}");
    }
}

#[test]
fn a_solid_composite_lays_the_layer_over_a_solid_in_its_box() {
    // Over black, the clear pixel turns black and the half-covered red
    // opaque; the colour's alpha of 0 changes nothing. Past the box,
    // nothing is drawn.
    let black = solid(1.0, [0.0; 3], 1.0, "normal");
    close(black.get(0, 0), [0.0, 0.0, 0.0, 1.0]);
    close(black.get(1, 0), [0.5, 0.0, 0.0, 1.0]);
    close(black.get(2, 0), GREY);
    close(black.get(3, 0), CLEAR);
    // The layer at half its opacity over a half-opaque white solid.
    let faded = solid(0.5, [1.0; 3], 0.5, "normal");
    close(faded.get(0, 0), [0.5, 0.5, 0.5, 0.5]);
    close(faded.get(2, 0), [0.5, 0.5, 0.5, 0.75]);
    // With no solid, the layer at its source opacity alone.
    let alone = solid(0.25, [1.0; 3], 0.0, "normal");
    close(alone.get(2, 0), GREY.map(|c| c * 0.25));
    // Screened over white, every pixel in the box turns white.
    let screened = solid(1.0, [1.0; 3], 1.0, "screen");
    close(screened.get(2, 0), [1.0; 4]);
    close(screened.get(3, 0), CLEAR);
}

#[test]
fn the_solid_covers_the_share_of_a_pixel_its_box_does() {
    let mut pixels = Pixels::transparent(4, 1);
    let space = Space {
        transform: vello_cpu::kurbo::Affine::IDENTITY,
        bounds: Some(vello_cpu::kurbo::Rect::new(0.25, 0.0, 2.5, 1.0)),
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
    };
    let values = [
        ParamValue::Number(1.0),
        ParamValue::Color(crate::style::Color::rgba(1.0, 1.0, 1.0, 1.0)),
        ParamValue::Number(1.0),
        ParamValue::Choice(0),
    ];
    solid_composite(&mut pixels, &values, &space);
    let alpha: Vec<f32> = pixels.data().iter().map(|p| p[3]).collect();
    assert_eq!(alpha, [0.75, 1.0, 0.5, 0.0]);
    // Seen through a transform that doubles it, the box covers twice as
    // many pixels.
    let doubled = Space {
        transform: vello_cpu::kurbo::Affine::scale(2.0),
        bounds: Some(vello_cpu::kurbo::Rect::new(0.0, 0.0, 1.25, 1.0)),
        ..space
    };
    let mut pixels = Pixels::transparent(4, 2);
    solid_composite(&mut pixels, &values, &doubled);
    let alpha: Vec<f32> = pixels.data()[..4].iter().map(|p| p[3]).collect();
    assert_eq!(alpha, [1.0, 1.0, 0.5, 0.0]);
}
