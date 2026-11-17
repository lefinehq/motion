//! Each operator family run as a small EPS program from `tests/programs`,
//! checked by what it paints: the items it leaves and the pixels they draw.

#[path = "support/raster.rs"]
mod raster;

use kurbo::{Point, Rect};
use nakkash_postscript::{FillRule, Item, NoteKind, Page, Rgb};
use pretty_assertions::assert_eq;
use raster::{Drawn, draw};

/// Runs the program `name` from `tests/programs`.
fn page(name: &str) -> Page {
    let path = format!("{}/tests/programs/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap();
    nakkash_postscript::read(&bytes).unwrap()
}

/// The page drawn at four pixels a point.
fn drawn(page: &Page) -> Drawn {
    draw(page, 4.0)
}

/// The pixel at point `(x, y)` of the program's own space (y up).
fn at(page: &Page, drawn: &Drawn, x: f64, y: f64) -> [u8; 4] {
    let b = page.bounding_box;
    drawn.at_point(4.0, x - b.x0, b.y1 - y)
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const EMPTY: [u8; 4] = [0, 0, 0, 0];

#[test]
fn paths_are_built_and_filled() {
    let page = page("paths.eps");
    assert_eq!(page.items.len(), 4);
    assert!(page.notes.is_empty(), "{:?}", page.notes);
    let d = drawn(&page);
    assert_eq!(at(&page, &d, 25.0, 25.0), RED);
    assert_eq!(at(&page, &d, 75.0, 15.0), GREEN);
    assert_eq!(at(&page, &d, 20.0, 70.0), BLUE);
    assert_eq!(at(&page, &d, 75.0, 75.0), BLACK);
    // Outside each shape nothing is painted.
    assert_eq!(at(&page, &d, 50.0, 50.0), EMPTY);
    assert_eq!(at(&page, &d, 35.0, 85.0), EMPTY);
}

#[test]
fn fill_rules_strokes_and_rectangles_paint() {
    let page = page("paint.eps");
    let d = drawn(&page);
    // Non-zero: the middle of the ring is filled; even-odd: it is not.
    assert_eq!(at(&page, &d, 25.0, 25.0), BLACK);
    assert_eq!(at(&page, &d, 75.0, 25.0), EMPTY);
    assert_eq!(at(&page, &d, 58.0, 25.0), BLACK);
    // The round cap reaches past the line's end; the line is 8 wide.
    assert_eq!(at(&page, &d, 8.0, 70.0), RED);
    assert_eq!(at(&page, &d, 25.0, 73.0), RED);
    assert_eq!(at(&page, &d, 25.0, 76.0), EMPTY);
    assert_eq!(at(&page, &d, 70.0, 60.0), BLUE);
    // A stroked rectangle paints its sides, not its inside.
    assert_eq!(at(&page, &d, 55.0, 82.0), GREEN);
    assert_eq!(at(&page, &d, 70.0, 82.0), EMPTY);
    let rules: Vec<FillRule> = page
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fill { rule, .. } => Some(*rule),
            _ => None,
        })
        .collect();
    assert_eq!(rules[..2], [FillRule::NonZero, FillRule::EvenOdd]);
}

#[test]
fn clips_narrow_and_restore() {
    let page = page("clip.eps");
    let d = drawn(&page);
    assert_eq!(at(&page, &d, 35.0, 35.0), RED);
    assert_eq!(at(&page, &d, 10.0, 10.0), EMPTY);
    assert_eq!(at(&page, &d, 75.0, 75.0), BLUE);
    assert_eq!(at(&page, &d, 63.0, 22.0), GREEN);
    assert_eq!(at(&page, &d, 77.0, 22.0), EMPTY);
    // The blue fill after `grestore` is outside every clip.
    let Item::Fill { clip, .. } = &page.items[1] else {
        panic!("{:?}", page.items[1]);
    };
    assert_eq!(*clip, None);
}

#[test]
fn colours_resolve_to_srgb() {
    let page = page("colour.eps");
    let colours: Vec<[u8; 3]> = page
        .items
        .iter()
        .map(|item| match item {
            Item::Fill { colour, .. } => colour.to_u8(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        colours,
        [
            [128, 128, 128],
            [255, 128, 0],
            [255, 255, 0],
            [0, 255, 255],
            [255, 0, 255],
            [0, 255, 0],
            [255, 64, 0],
        ]
    );
    // The separation of no colorant painted nothing: seven items, not eight.
    assert_eq!(page.items.len(), 7);
}

#[test]
fn cmyk_follows_the_language_rule() {
    // Each additive primary is one minus its subtractive one plus black.
    let program = b"%!PS-3.0 EPSF-3.0\n%%BoundingBox: 0 0 1 1\n\
        0.2 0.4 0.6 0.1 setcmykcolor 0 0 1 1 rectfill";
    let page = nakkash_postscript::read(program).unwrap();
    let Item::Fill { colour, .. } = &page.items[0] else {
        panic!("{:?}", page.items);
    };
    let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
    assert!(
        close(colour.r, 0.7) && close(colour.g, 0.5) && close(colour.b, 0.3),
        "{colour:?}"
    );
}

#[test]
fn matrices_place_paths_and_pens() {
    let page = page("matrix.eps");
    let d = drawn(&page);
    assert_eq!(at(&page, &d, 20.0, 20.0), RED);
    assert_eq!(at(&page, &d, 32.0, 32.0), EMPTY);
    // The turned square stands on a corner at (75, 10).
    assert_eq!(at(&page, &d, 75.0, 20.0), GREEN);
    assert_eq!(at(&page, &d, 88.0, 12.0), EMPTY);
    // The pen squashed to a quarter: 20 wide becomes 5 tall.
    assert_eq!(at(&page, &d, 50.0, 70.0), BLUE);
    assert_eq!(at(&page, &d, 50.0, 72.0), BLUE);
    assert_eq!(at(&page, &d, 50.0, 74.0), EMPTY);
    let Some(Item::Stroke {
        transform, style, ..
    }) = page.items.get(2)
    else {
        panic!("{:?}", page.items);
    };
    assert_eq!(style.width, 20.0);
    assert_eq!(transform.as_coeffs()[3], -0.25);
}

#[test]
fn images_read_their_samples() {
    let page = page("image.eps");
    assert!(page.notes.is_empty(), "{:?}", page.notes);
    let images: Vec<(&nakkash_postscript::Raster, kurbo::Affine)> = page
        .items
        .iter()
        .map(|item| match item {
            Item::Image {
                raster, transform, ..
            } => (&**raster, *transform),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(images.len(), 3);
    let (rgb, place) = images[0];
    assert_eq!((rgb.width, rgb.height), (2, 2));
    assert_eq!(
        rgb.pixels,
        [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255
        ]
    );
    // The first row is the top of the unit square, here y = 10 up, which
    // is the top of the page.
    assert_eq!(place * Point::ZERO, Point::new(0.0, 10.0));
    assert_eq!(place * Point::new(2.0, 2.0), Point::new(10.0, 20.0));
    let (mask, _) = images[1];
    let alpha: Vec<u8> = mask.pixels.chunks(4).map(|p| p[3]).collect();
    assert_eq!(alpha[..8], [255, 255, 255, 255, 0, 0, 0, 0]);
    assert_eq!(alpha[8..], [0, 0, 0, 0, 255, 255, 255, 255]);
    assert_eq!(mask.pixels[..4], [255, 0, 0, 255]);
    let (gray, _) = images[2];
    assert_eq!(gray.pixels, [0, 0, 0, 255, 255, 255, 255, 255]);
}

#[test]
fn procedure_sets_run_as_resources() {
    let page = page("procset.eps");
    assert!(page.notes.is_empty(), "{:?}", page.notes);
    assert_eq!(page.bounding_box, Rect::new(0.0, 0.0, 100.0, 100.0));
    let d = drawn(&page);
    // Level 2 chose the red of `0 1 1 0 k`; the triangle is white.
    assert_eq!(at(&page, &d, 15.0, 15.0), RED);
    assert_eq!(at(&page, &d, 50.0, 40.0), WHITE);
    assert_eq!(at(&page, &d, 5.0, 5.0), EMPTY);
    assert_eq!(page.clips.len(), 1);
}

/// A binary EPS file: the 30-byte header, a preview that is never read,
/// then the program.
fn binary(program: &[u8]) -> Vec<u8> {
    let preview = [0xAAu8; 16];
    let start = 30 + preview.len();
    let mut bytes = vec![0xC5, 0xD0, 0xD3, 0xC6];
    for word in [start, program.len(), 0, 0, 30, preview.len()] {
        bytes.extend_from_slice(&u32::try_from(word).unwrap().to_le_bytes());
    }
    bytes.extend_from_slice(&[0xFF, 0xFF]);
    bytes.extend_from_slice(&preview);
    bytes.extend_from_slice(program);
    bytes
}

#[test]
fn binary_files_run_their_program() {
    let program = b"%!PS-3.0 EPSF-3.0\n%%BoundingBox: 0 0 10 10\n\
        %%HiResBoundingBox: 0 0 9.5 9.25\n%%EndComments\n0 0 1 setrgbcolor 0 0 5 5 rectfill\n";
    let page = nakkash_postscript::read(&binary(program)).unwrap();
    assert_eq!(page.bounding_box, Rect::new(0.0, 0.0, 10.0, 10.0));
    assert_eq!(page.art, Some(Rect::new(0.0, 0.75, 9.5, 10.0)));
    assert_eq!(page.items.len(), 1);
    let truncated = &binary(program)[..40];
    assert_eq!(
        nakkash_postscript::read(truncated),
        Err(nakkash_postscript::Error::Truncated)
    );
}

#[test]
fn the_box_may_wait_for_the_trailer() {
    let program = b"%!PS-3.0 EPSF-3.0\n%%BoundingBox: (atend)\n%%EndComments\n\
        0 0 moveto\n%%Trailer\n%%BoundingBox: 5 6 25 46\n%%EOF\n";
    let page = nakkash_postscript::read(program).unwrap();
    assert_eq!(page.bounding_box, Rect::new(5.0, 6.0, 25.0, 46.0));
    assert_eq!(page.size, kurbo::Size::new(20.0, 40.0));
}

#[test]
fn files_that_are_not_eps_are_refused() {
    use nakkash_postscript::Error;
    assert_eq!(
        nakkash_postscript::read(b"\x89PNG"),
        Err(Error::NotPostScript)
    );
    assert_eq!(
        nakkash_postscript::read(b"%!PS\n1 2 add"),
        Err(Error::NoBoundingBox)
    );
}

/// The notes of running `body` in a 10-point box.
fn notes(body: &str) -> Vec<(NoteKind, String)> {
    let program = format!("%!PS-3.0 EPSF-3.0\n%%BoundingBox: 0 0 10 10\n{body}");
    let page = nakkash_postscript::read(program.as_bytes()).unwrap();
    page.notes
        .into_iter()
        .map(|n| (n.kind, n.message))
        .collect()
}

#[test]
fn what_is_not_drawn_is_said() {
    let text = notes("/Helvetica findfont 12 scalefont setfont 0 0 moveto (Hi) show (Ho) show");
    assert_eq!(
        text,
        [(
            NoteKind::Unsupported,
            "text (`show`), drawn as nothing".into()
        )]
    );
    let shading = notes("<< /ShadingType 2 >> shfill");
    assert!(shading[0].1.contains("shfill"), "{shading:?}");
    let pattern = notes("<< /PatternType 1 >> matrix makepattern setpattern 0 0 5 5 rectfill");
    assert!(pattern[0].1.contains("pattern"), "{pattern:?}");
}

#[test]
fn an_uncaught_error_keeps_what_was_drawn() {
    let program = b"%!PS-3.0 EPSF-3.0\n%%BoundingBox: 0 0 10 10\n\
        0 0 5 5 rectfill nosuchoperator 5 5 5 5 rectfill";
    let page = nakkash_postscript::read(program).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.notes.len(), 1);
    assert_eq!(page.notes[0].kind, NoteKind::Error);
    assert!(
        page.notes[0].message.contains("undefined"),
        "{:?}",
        page.notes
    );
}

#[test]
fn colours_are_opaque_srgb() {
    assert_eq!(Rgb::new(2.0, -1.0, 0.5).to_u8(), [255, 0, 128]);
}
