//! Golden images of whole scenes: each authored scene in `tests/golden/`,
//! drawn through a [`Session`] at one frame with [`RenderOptions::GOLDEN`]
//! and compared byte for byte with `tests/golden/<scene>-<frame>.png` ([D12]).
//! They guard the mapping from evaluated frames to display lists; the
//! rasteriser has goldens of its own.
//!
//! Text is set in the built-in Inter, or in the rasteriser's fixture font
//! (a renamed subset of Lato Black) where a scene needs a face without a
//! bold or an italic.
//!
//! Run with `NAKKASH_BLESS=1` to rewrite the PNGs after a deliberate change;
//! a failing comparison writes what it drew next to the target directory as
//! `<scene>-<frame>.actual.png`.
//!
//! [D12]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d12-what-repeatable-means

use std::path::PathBuf;

use nakkash_engine::{Image, RenderOptions, Session};
use rstest::rstest;

/// Draws `scene` at `frame` and compares it with its golden image.
#[rstest]
// Rects with rounded corners, both fill rules, a stroke under a squash, caps,
// joins and miters, and opacity on paints, groups and layers.
#[case::shapes("shapes", 0)]
// Stars and polygons: a star filled and stroked, one with rounded points and
// inner corners, a rounded hexagon stroked, a rounded square, a star whose
// negative roundness loops its points, and two polygons cut to 40 % of their
// outline, one each way round from the top.
#[case::stars("stars", 0)]
// Lines rising and fading in one at a time, half way: fill under a stroke,
// a line half faded, glyphs turned, scaled and recoloured one by one.
#[case::text_animator_half_way("text_animator", 10)]
#[case::text_animator_settled("text_animator", 25)]
// Faux bold, faux italic, both, and a translucent faux bold fill under a stroke.
#[case::faux("faux", 0)]
// Two copies of a composition on their own clocks, clipped to its canvas, one
// of them inside the other's opacity, with a composition nested inside.
#[case::nested_comp("nested_comp", 12)]
// The fill effect on text, on a composition and twice on one shape.
#[case::fill_effect("fill_effect", 0)]
// A hidden text layer as an alpha matte; a moving rect as an inverted one;
// a luma matte of stripes inside a nested composition; and a matte that is
// matted itself, at the start and with the door moved across.
#[case::mattes_start("mattes", 0)]
#[case::mattes_moved("mattes", 12)]
// A picture file as drawn; stretched, turned and faded under a turned
// parent, over a bar that shows through; scaled from a box off its origin
// inside a round alpha matte; and, hidden, as the luma matte of a rect.
#[case::images("images", 0)]
// Masks: one subtracted from another, intersected, lightened with their
// opacities, and differing; an outline feathered more across than down;
// an expanded mask keeping the outside; a blur that spreads past its mask,
// which cuts before it; a mask turning and scaling with its layer and
// shrunk; and a filter layer inverting only where its feathered mask is.
#[case::masks("masks", 0)]
// Motion blur over a half-frame shutter: a rect moving fast beside the same
// rect unblurred, a bar spinning, a layer whose span starts while the
// shutter is open, and a nested composition with a whole-frame shutter of
// its own.
#[case::motion_blur("motion_blur", 6)]
fn golden(#[case] scene: &str, #[case] frame: u32) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let session = Session::open(dir.join(format!("{scene}.json"))).unwrap();
    let image = session
        .render_with(f64::from(frame), RenderOptions::GOLDEN)
        .unwrap();
    let name = format!("{scene}-{frame}");
    let path = dir.join(format!("{name}.png"));
    if std::env::var_os("NAKKASH_BLESS").is_some() {
        std::fs::write(&path, image.to_png().unwrap()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("{} is missing; run with NAKKASH_BLESS=1", path.display()));
    let expected = Image::from_png(&bytes).unwrap();
    if expected != image {
        let actual = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        std::fs::write(&actual, image.to_png().unwrap()).unwrap();
        let (count, worst) = difference(&expected, &image);
        panic!(
            "{name}: {count} bytes differ, by up to {worst}; drawn: {}",
            actual.display()
        );
    }
}

/// How many bytes differ, and the largest difference.
fn difference(a: &Image, b: &Image) -> (usize, u8) {
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return (usize::MAX, u8::MAX);
    }
    a.pixels()
        .iter()
        .zip(b.pixels())
        .filter(|(x, y)| x != y)
        .fold((0, 0), |(n, worst), (x, y)| {
            (n + 1, worst.max(x.abs_diff(*y)))
        })
}
