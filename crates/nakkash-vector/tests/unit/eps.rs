//! Unit tests for `src/eps.rs`: an EPS page laid on the canvas.

use nakkash_raster::kurbo::{Rect, Size};

use super::*;

/// The drawing of a program in a box of `x0 y0 x1 y1`.
fn drawing(boxes: &str, body: &str) -> Drawing {
    let program = format!("%!PS-3.0 EPSF-3.0\n{boxes}\n%%EndComments\n{body}\n");
    read(program.as_bytes()).unwrap()
}

#[test]
fn the_page_is_the_whole_point_box_and_the_art_the_precise_one() {
    let d = drawing(
        "%%BoundingBox: 10 20 50 44\n%%HiResBoundingBox: 10.5 20.25 49.5 43.75",
        "0 0 1 setrgbcolor 10 20 40 24 rectfill",
    );
    assert_eq!(d.size(), Size::new(40.0, 24.0));
    assert_eq!(d.art(), Some(Rect::new(0.5, 0.25, 39.5, 23.75)));
    assert_eq!(d.nodes().len(), 1);
    assert!(d.unsupported().is_empty(), "{:?}", d.unsupported());
}

#[test]
fn clips_wrap_what_they_clip() {
    let d = drawing(
        "%%BoundingBox: 0 0 10 10",
        "gsave 0 0 5 5 rectclip 0 0 10 10 rectfill 1 1 2 2 rectfill grestore 0 0 1 1 rectfill",
    );
    // One clip around the first two fills, then the third outside it.
    assert_eq!(d.nodes().len(), 2);
}

#[test]
fn what_is_not_drawn_and_an_early_end_are_said() {
    let d = drawing(
        "%%BoundingBox: 0 0 10 10",
        "/Helvetica findfont 9 scalefont setfont 0 0 moveto (A) show 0 0 5 5 rectfill nosuchname",
    );
    assert_eq!(d.nodes().len(), 1);
    let said = d.unsupported().join(" | ");
    assert!(said.contains("text (`show`)"), "{said}");
    assert!(said.contains("EPS footage cut short"), "{said}");
}

#[test]
fn a_zero_width_line_is_one_point_wide() {
    let pen = kurbo::Stroke::new(0.0);
    assert_eq!(style(&pen, Affine::scale(4.0)).width, 0.25);
    assert_eq!(style(&kurbo::Stroke::new(2.0), Affine::IDENTITY).width, 2.0);
}
