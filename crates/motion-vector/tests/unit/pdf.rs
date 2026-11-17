//! Unit tests for `src/pdf.rs`: one or more small PDFs, written by hand,
//! for each family of drawing operators, drawn and read back pixel by pixel.

use rstest::rstest;

use super::*;
use crate::sample::{at, near, page, pdf, plain, read, render, stream, zlib_stored};

const CLEAR: [u8; 4] = [0, 0, 0, 0];
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

#[rstest]
#[case::header(b"%PDF-1.6\n".as_slice(), true)]
#[case::junk_before(b"\r\n\r\n%PDF-1.4\n".as_slice(), true)]
#[case::png(b"\x89PNG\r\n\x1a\n".as_slice(), false)]
#[case::postscript(b"%!PS\n".as_slice(), false)]
fn a_pdf_is_known_by_its_header(#[case] head: &[u8], #[case] is: bool) {
    assert_eq!(recognise(head), is);
}

#[test]
fn bytes_that_are_not_a_pdf_are_an_error() {
    let e = read_pdf(b"%PDF-1.7\nnothing else").unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
}

fn read_pdf(bytes: &[u8]) -> Result<Drawing> {
    super::read(bytes)
}

#[test]
fn the_page_is_the_drawings_space_y_down_with_its_art_box() {
    let bytes = pdf(
        (40.0, 30.0),
        "/ArtBox [5 10 25 20]",
        "",
        "1 0 0 rg 0 0 10 10 re f",
        vec![],
    );
    let drawing = read(&bytes);
    assert_eq!(drawing.size(), Size::new(40.0, 30.0));
    // The art box's bottom, y 10 up, is 20 down from the top.
    assert_eq!(drawing.art(), Some(Rect::new(5.0, 10.0, 25.0, 20.0)));
    let image = render(&bytes);
    // The square at the page's origin lands at its bottom-left.
    assert_eq!(at(&image, 2, 27), RED);
    assert_eq!(at(&image, 2, 2), CLEAR);
}

#[test]
fn a_crop_box_moves_the_origin() {
    let bytes = pdf(
        (40.0, 40.0),
        "/CropBox [10 10 30 30]",
        "",
        "1 0 0 rg 10 10 5 5 re f",
        vec![],
    );
    let drawing = read(&bytes);
    assert_eq!(drawing.size(), Size::new(20.0, 20.0));
    assert_eq!(at(&crate::sample::drawn(&drawing, 1.0), 2, 17), RED);
}

#[rstest]
#[case::nonzero("f", RED)]
#[case::even_odd("f*", CLEAR)]
fn fill_rules_decide_the_inside_of_overlapping_outlines(
    #[case] fill: &str,
    #[case] middle: [u8; 4],
) {
    // Two squares drawn the same way round: the inner one winds twice.
    let image = render(&page(
        "",
        &format!("1 0 0 rg 2 2 16 16 re 6 6 8 8 re {fill}"),
        vec![],
    ));
    assert_eq!(at(&image, 10, 10), middle);
    assert_eq!(at(&image, 3, 3), RED);
}

#[test]
fn colours_in_each_device_space_become_srgb() {
    let image = render(&page(
        "",
        "0.5 g 0 0 5 20 re f 0 0 1 rg 5 0 5 20 re f 0 1 1 0 k 10 0 5 20 re f",
        vec![],
    ));
    assert!(near(at(&image, 2, 10), [128, 128, 128, 255], 1));
    assert_eq!(at(&image, 7, 10), BLUE);
    // Magenta and yellow ink make a red, through a press profile.
    let [r, g, b, a] = at(&image, 12, 10);
    assert!(
        r > 200 && g < 80 && b < 80 && a == 255,
        "{:?}",
        [r, g, b, a]
    );
}

#[test]
fn an_icc_colour_space_that_cannot_be_read_falls_back_to_its_alternate() {
    let image = render(&page(
        "/ColorSpace << /CS0 5 0 R >>",
        "/CS0 cs 0 0 1 scn 0 0 20 20 re f",
        vec![
            plain("[/ICCBased 6 0 R]"),
            stream(
                "<< /N 3 /Alternate /DeviceRGB >>",
                b"not a profile".to_vec(),
            ),
        ],
    ));
    assert_eq!(at(&image, 10, 10), BLUE);
}

#[rstest]
#[case::butt(0, CLEAR)]
#[case::square(2, RED)]
fn strokes_have_their_width_and_caps(#[case] cap: u8, #[case] past_end: [u8; 4]) {
    let image = render(&page(
        "",
        &format!("1 0 0 RG 4 w {cap} J 5 10 m 15 10 l S"),
        vec![],
    ));
    // Four wide about y 10: rows 8 to 11.
    assert_eq!(at(&image, 10, 8), RED);
    assert_eq!(at(&image, 10, 11), RED);
    assert_eq!(at(&image, 10, 12), CLEAR);
    assert_eq!(at(&image, 10, 7), CLEAR);
    assert_eq!(at(&image, 16, 10), past_end);
}

#[test]
fn dashes_leave_gaps() {
    let image = render(&page("", "1 0 0 RG 4 w [4 4] 0 d 0 10 m 20 10 l S", vec![]));
    assert_eq!(at(&image, 2, 10), RED);
    assert_eq!(at(&image, 6, 10), CLEAR);
    assert_eq!(at(&image, 10, 10), RED);
}

#[test]
fn a_stroke_under_an_uneven_scale_is_squashed_too() {
    // Two units wide across x, which is stretched four times.
    let image = render(&page(
        "",
        "q 4 0 0 1 0 0 cm 1 0 0 RG 2 w 2.5 2 m 2.5 18 l S Q",
        vec![],
    ));
    assert_eq!(at(&image, 6, 10), RED);
    assert_eq!(at(&image, 13, 10), RED);
    assert_eq!(at(&image, 15, 10), CLEAR);
    assert_eq!(at(&image, 4, 10), CLEAR);
}

#[test]
fn transforms_last_until_the_state_is_restored() {
    let image = render(&page(
        "",
        "1 0 0 rg q 1 0 0 1 10 10 cm 0 0 5 5 re f Q 0 0 5 5 re f",
        vec![],
    ));
    assert_eq!(at(&image, 12, 7), RED);
    assert_eq!(at(&image, 2, 17), RED);
    assert_eq!(at(&image, 7, 12), CLEAR);
}

#[test]
fn a_clip_cuts_what_is_painted_after_it_until_restored() {
    let image = render(&page(
        "",
        "q 0 0 10 20 re W n 1 0 0 rg 0 0 20 20 re f Q 0 0 1 rg 15 0 5 5 re f",
        vec![],
    ));
    assert_eq!(at(&image, 5, 10), RED);
    assert_eq!(at(&image, 12, 10), CLEAR);
    assert_eq!(at(&image, 17, 17), BLUE);
}

#[test]
fn constant_alpha_comes_from_the_graphics_state() {
    let image = render(&page(
        "/ExtGState << /G0 << /ca 0.5 >> >>",
        "/G0 gs 1 0 0 rg 0 0 20 20 re f",
        vec![],
    ));
    assert!(near(at(&image, 10, 10), [255, 0, 0, 128], 1));
}

#[test]
fn a_blend_mode_from_the_graphics_state_blends_with_what_is_below() {
    let image = render(&page(
        "/ExtGState << /G0 << /BM /Multiply >> >>",
        "1 1 0 rg 0 0 20 20 re f /G0 gs 0 1 1 rg 0 0 10 20 re f",
        vec![],
    ));
    // Yellow times cyan is green.
    assert_eq!(at(&image, 5, 10), [0, 255, 0, 255]);
    assert_eq!(at(&image, 15, 10), [255, 255, 0, 255]);
}

#[test]
fn an_axial_shading_runs_between_its_colours() {
    let image = render(&page(
        "/Shading << /Sh0 5 0 R >>",
        "/Sh0 sh",
        vec![plain(
            "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 20 0] /Extend [true true] \
             /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>",
        )],
    ));
    assert!(near(at(&image, 0, 10), RED, 16));
    assert!(near(at(&image, 19, 10), BLUE, 16));
    assert!(near(at(&image, 10, 3), [128, 0, 128, 255], 10));
}

#[test]
fn a_radial_shading_pattern_fills_an_outline() {
    let image = render(&page(
        "/Pattern << /P0 5 0 R >>",
        "/Pattern cs /P0 scn 0 0 20 20 re f",
        vec![plain(
            "<< /PatternType 2 /Shading << /ShadingType 3 /ColorSpace /DeviceRGB \
             /Coords [10 10 0 10 10 10] /Extend [true true] \
             /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> >>",
        )],
    ));
    assert!(near(at(&image, 10, 10), RED, 30));
    assert!(near(at(&image, 0, 0), BLUE, 2));
}

#[test]
fn a_shading_that_does_not_extend_is_sampled_and_stops_at_its_ends() {
    let drawing = read(&page(
        "/Shading << /Sh0 5 0 R >>",
        "/Sh0 sh",
        vec![plain(
            "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [5 0 15 0] /Extend [false false] \
             /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>",
        )],
    ));
    let image = crate::sample::drawn(&drawing, 1.0);
    assert_eq!(at(&image, 2, 10), CLEAR);
    assert!(near(at(&image, 6, 10), RED, 40));
    assert_eq!(at(&image, 17, 10), CLEAR);
}

#[test]
fn a_tiling_pattern_repeats_its_cell_as_vectors() {
    let image = render(&page(
        "/Pattern << /P0 5 0 R >>",
        "/Pattern cs /P0 scn 0 0 20 20 re f",
        vec![stream(
            "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 \
             /Resources << >> >>",
            b"1 0 0 rg 0 0 5 5 re f".to_vec(),
        )],
    ));
    for (x, y) in [(2, 17), (12, 17), (2, 7), (12, 7)] {
        assert_eq!(at(&image, x, y), RED, "a cell at {x}, {y}");
    }
    assert_eq!(at(&image, 7, 12), CLEAR);
}

#[test]
fn a_form_object_draws_through_its_matrix() {
    let image = render(&page(
        "/XObject << /X0 5 0 R >>",
        "/X0 Do",
        vec![stream(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 5 5] /Matrix [2 0 0 2 10 0] >>",
            b"1 0 0 rg 0 0 5 5 re f".to_vec(),
        )],
    ));
    assert_eq!(at(&image, 15, 15), RED);
    assert_eq!(at(&image, 5, 15), CLEAR);
}

#[test]
fn a_transparency_group_is_composited_before_its_alpha() {
    // Two overlapping opaque squares in a group at half alpha: the overlap
    // is no darker than the rest, as it would be painted one by one.
    let image = render(&page(
        "/XObject << /X0 5 0 R >> /ExtGState << /G0 << /ca 0.5 >> >>",
        "/G0 gs /X0 Do",
        vec![stream(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] \
             /Group << /S /Transparency >> >>",
            b"1 0 0 rg 0 0 12 20 re f 8 0 12 20 re f".to_vec(),
        )],
    ));
    assert!(near(at(&image, 4, 10), [255, 0, 0, 128], 1));
    assert!(near(at(&image, 10, 10), [255, 0, 0, 128], 1));
}

#[test]
fn a_luminosity_soft_mask_shows_where_its_group_is_bright() {
    let image = render(&page(
        "/ExtGState << /G0 << /SMask << /S /Luminosity /G 5 0 R >> >> >>",
        "/G0 gs 1 0 0 rg 0 0 20 20 re f",
        vec![stream(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] \
             /Group << /S /Transparency /CS /DeviceGray >> >>",
            b"1 g 0 0 10 20 re f".to_vec(),
        )],
    ));
    assert_eq!(at(&image, 5, 10), RED);
    assert_eq!(at(&image, 15, 10), CLEAR);
}

#[test]
fn an_alpha_soft_mask_shows_where_its_group_paints() {
    let image = render(&page(
        "/ExtGState << /G0 << /SMask << /S /Alpha /G 5 0 R >> >> >>",
        "/G0 gs 1 0 0 rg 0 0 20 20 re f",
        vec![stream(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Group << /S /Transparency >> >>",
            b"0 g 10 0 10 20 re f".to_vec(),
        )],
    ));
    assert_eq!(at(&image, 5, 10), CLEAR);
    assert_eq!(at(&image, 15, 10), RED);
}

#[test]
fn a_flate_image_is_drawn_over_the_unit_square() {
    // Two pixels, red then blue, in a row.
    let pixels = zlib_stored(&[255, 0, 0, 0, 0, 255]);
    let image = render(&page(
        "/XObject << /Im0 5 0 R >>",
        "q 20 0 0 20 0 0 cm /Im0 Do Q",
        vec![stream(
            "<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /FlateDecode >>",
            pixels,
        )],
    ));
    assert!(near(at(&image, 2, 10), RED, 2));
    assert!(near(at(&image, 17, 10), BLUE, 2));
}

#[test]
fn a_jpeg_image_is_decoded() {
    const HALVES: &[u8] = include_bytes!("../../../motion-raster/tests/fixtures/halves.jpg");
    let image = render(&page(
        "/XObject << /Im0 5 0 R >>",
        "q 20 0 0 20 0 0 cm /Im0 Do Q",
        vec![stream(
            "<< /Type /XObject /Subtype /Image /Width 16 /Height 8 /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /DCTDecode >>",
            HALVES.to_vec(),
        )],
    ));
    assert!(near(at(&image, 2, 10), [220, 30, 30, 255], 12));
    assert!(near(at(&image, 17, 10), [30, 60, 220, 255], 12));
}

#[test]
fn an_image_mask_paints_the_fill_colour_where_it_is_set() {
    // One byte, two pixels: the first painted (sample 0), the second not.
    let image = render(&page(
        "/XObject << /Im0 5 0 R >>",
        "0 0 1 rg q 20 0 0 20 0 0 cm /Im0 Do Q",
        vec![stream(
            "<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /ImageMask true \
             /BitsPerComponent 1 >>",
            vec![0b0100_0000],
        )],
    ));
    assert_eq!(at(&image, 2, 10), BLUE);
    assert_eq!(at(&image, 17, 10), CLEAR);
}

/// A TrueType font, embedded, for drawing `H` at 16 points.
fn font_objects(embedded: bool) -> Vec<crate::sample::Object> {
    const FONT: &[u8] =
        include_bytes!("../../../motion-raster/tests/fixtures/FixtureSans-Black.ttf");
    let file = match embedded {
        true => " /FontFile2 7 0 R",
        false => "",
    };
    vec![
        plain(
            "<< /Type /Font /Subtype /TrueType /BaseFont /FixtureSans-Black \
             /FirstChar 72 /LastChar 72 /Widths [700] /Encoding /WinAnsiEncoding \
             /FontDescriptor 6 0 R >>",
        ),
        plain(&format!(
            "<< /Type /FontDescriptor /FontName /FixtureSans-Black /Flags 32 \
             /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 900 /Descent -200 \
             /CapHeight 700 /StemV 80{file} >>"
        )),
        stream(&format!("<< /Length1 {} >>", FONT.len()), FONT.to_vec()),
    ]
}

#[test]
fn text_in_an_embedded_font_is_drawn_from_its_outlines() {
    let drawing = read(&page(
        "/Font << /F0 5 0 R >>",
        "1 0 0 rg BT /F0 16 Tf 2 4 Td (H) Tj ET",
        font_objects(true),
    ));
    assert!(
        drawing.unsupported().is_empty(),
        "{:?}",
        drawing.unsupported()
    );
    let image = crate::sample::drawn(&drawing, 4.0);
    let red = (0..80)
        .flat_map(|y| (0..80).map(move |x| (x, y)))
        .filter(|&(x, y)| at(&image, x, y) == RED)
        .count();
    assert!(red > 200, "{red} red pixels");
}

#[test]
fn text_in_a_font_the_file_does_not_embed_is_reported() {
    let drawing = read(&page(
        "/Font << /F0 5 0 R >>",
        "1 0 0 rg BT /F0 16 Tf 2 4 Td (H) Tj ET",
        font_objects(false),
    ));
    assert_eq!(
        drawing.unsupported(),
        ["text in a font the file does not embed or that cannot be read (not drawn)"]
    );
}

#[rstest]
#[case::overprint("<< /OP true >>", "overprinting (drawn as if off)")]
#[case::transfer("<< /TR 6 0 R >>", "transfer functions (drawn without them)")]
fn what_the_interpreter_leaves_out_is_reported(#[case] state: &str, #[case] reported: &str) {
    let drawing = read(&page(
        "/ExtGState << /G0 5 0 R >>",
        "/G0 gs 0 0 5 5 re f",
        vec![
            plain(state),
            plain("<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>"),
        ],
    ));
    assert_eq!(drawing.unsupported(), [reported]);
}

#[test]
fn a_knockout_group_is_reported() {
    let drawing = read(&page(
        "/XObject << /X0 5 0 R >>",
        "/X0 Do",
        vec![stream(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] \
             /Group << /S /Transparency /K true >> >>",
            b"0 0 5 5 re f".to_vec(),
        )],
    ));
    assert_eq!(
        drawing.unsupported(),
        ["knockout groups (drawn as plain groups)"]
    );
}

#[test]
fn a_line_width_of_zero_is_one_point_wide() {
    let style = stroke_style(
        &StrokeProps {
            line_width: 0.0,
            ..StrokeProps::default()
        },
        Affine::scale(4.0),
    );
    assert_eq!(style.width, 0.25);
}

#[test]
fn what_is_left_out_is_found_in_dictionaries_written_inside_others() {
    let drawing = read(&page(
        "/ExtGState << /G0 << /op true >> >>",
        "/G0 gs 0 0 5 5 re f",
        vec![],
    ));
    assert_eq!(drawing.unsupported(), ["overprinting (drawn as if off)"]);
}

#[test]
fn a_soft_mask_transfer_function_is_reported_where_it_is_drawn() {
    let drawing = read(&page(
        "/ExtGState << /G0 << /SMask << /S /Luminosity /G 5 0 R /TR 6 0 R >> >> >>",
        "/G0 gs 1 0 0 rg 0 0 20 20 re f",
        vec![
            stream(
                "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] \
                 /Group << /S /Transparency /CS /DeviceGray >> >>",
                b"1 g 0 0 10 20 re f".to_vec(),
            ),
            plain("<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>"),
        ],
    ));
    assert_eq!(
        drawing.unsupported(),
        ["soft masks with a transfer function (drawn without it)"]
    );
}
