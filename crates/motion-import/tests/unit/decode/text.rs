//! Unit tests for `src/decode/text.rs`, on hand-built text-document bodies.

use proptest::prelude::*;

use super::*;
use crate::model::{Caps, Justification, Kerning, LineJoin, Value};
use crate::test_support::{text_body, text_document};

/// Where test bodies are said to be.
const AT: &str = "LIST:btdk";

/// The documents of a body built from `fonts`, `frames` and `documents`.
fn read_body(fonts: &[&str], frames: &[&str], documents: &[String]) -> Result<Vec<TextDocument>> {
    documents_of(text_body(fonts, frames, documents).as_bytes())
}

/// The documents of `body`.
fn documents_of(body: &[u8]) -> Result<Vec<TextDocument>> {
    super::documents(body, AT)
}

/// A segment of a [`composed`] line: characters, x, glyph ids and width.
type Segment<'a> = (usize, f64, &'a str, f64);

/// A composed-line cache of `lines`, each its origin's y and its segments.
fn composed(lines: &[(f64, &[Segment])]) -> String {
    let lines: Vec<String> = lines
        .iter()
        .map(|(y, segments)| {
            let segments: Vec<String> = segments
                .iter()
                .map(|(count, x, glyphs, width)| {
                    format!(
                        "<< /99 /S /15 << /0 {count} /2 0 /5 false >> /0 << /0 [ {x:?} 0.0 ] >> \
                         /6 [ << /99 /G /1 [ 0.0 -30.0 {width:?} 10.0 ] /5 [ {glyphs} ] >> ] >>"
                    )
                })
                .collect();
            format!(
                "<< /99 /L /10 {y:?} /0 << /0 [ 0.0 {y:?} ] >> /6 [ {} ] >>",
                segments.join(" ")
            )
        })
        .collect();
    format!(
        "<< /99 /PC /5 0 /6 [ << /99 /F /6 [ << /99 /R /6 [ << /99 /R /6 [ {} ] >> ] >> ] >> ] >>",
        lines.join(" ")
    )
}

#[test]
fn reads_text_runs_styles_and_the_stored_layout() {
    let cache = composed(&[
        (0.0, &[(6, -50.5, "20 11 2", 101.0)]),
        (52.8, &[(5, -40.0, "43 27", 80.0)]),
    ]);
    let document = text_document(
        "Hello\rworld\r",
        &[(6, ""), (6, "/0 0")],
        &[(2, "/1 60.0 /12 2 /57 true /63 3.0 /62 1"), (10, "/0 1")],
        &cache,
    );
    let [document] = &read_body(&["InvisibleFont", "Inter-Bold"], &[], &[document]).unwrap()[..]
    else {
        panic!("one document")
    };
    assert_eq!(document.text, "Hello\nworld");
    let paragraphs: Vec<_> = document
        .paragraphs
        .iter()
        .map(|run| (run.range.clone(), run.style.justification))
        .collect();
    assert_eq!(
        paragraphs,
        [(0..6, Justification::Center), (6..11, Justification::Left)]
    );
    let [first, second] = &document.characters[..] else {
        panic!("two runs")
    };
    assert_eq!((first.range.clone(), second.range.clone()), (0..2, 2..11));
    assert_eq!(
        (
            first.style.font.as_str(),
            first.style.size,
            first.style.caps
        ),
        ("InvisibleFont", 60.0, Caps::All)
    );
    assert_eq!(
        (
            second.style.font.as_str(),
            second.style.size,
            second.style.caps
        ),
        ("Inter-Bold", 44.0, Caps::Normal)
    );
    let red = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    assert_eq!(
        (
            first.style.stroke,
            first.style.stroke_width,
            first.style.line_join
        ),
        (Some(red), 3.0, LineJoin::Round)
    );
    assert_eq!(
        (second.style.stroke, second.style.kerning),
        (None, Kerning::Metrics)
    );
    assert_eq!(document.frame, TextFrame::Point);
    let lines: Vec<_> = document
        .lines
        .iter()
        .map(|line| (line.origin, line.baseline, line.segments.clone()))
        .collect();
    assert_eq!(
        lines,
        [
            (
                [0.0, 0.0],
                0.0,
                vec![ComposedSegment {
                    characters: 6,
                    origin: [-50.5, 0.0],
                    advances: vec![],
                    width: 101.0,
                    glyphs: vec![20, 11, 2],
                }]
            ),
            (
                [0.0, 52.8],
                52.8,
                vec![ComposedSegment {
                    characters: 5,
                    origin: [-40.0, 0.0],
                    advances: vec![],
                    width: 80.0,
                    glyphs: vec![43, 27],
                }]
            ),
        ]
    );
}

#[test]
fn reads_stored_advances_and_lines_without_an_origin() {
    let cache = "<< /99 /PC /6 [ << /99 /L /6 [ << /99 /S /15 << /0 2 /7 << /7 [ 10.0 22.5 ] >> >> \
                 /6 [ << /99 /X >> ] >> << /99 /Q >> ] >> << /99 /Y >> ] >>";
    let document = text_document("ab\r", &[(3, "")], &[(3, "")], cache);
    let documents = read_body(&["Inter-Bold"], &[], &[document]).unwrap();
    let line = &documents[0].lines[0];
    assert_eq!((line.origin, line.baseline), ([0.0, 0.0], 0.0));
    let segment = &line.segments[0];
    assert_eq!(
        (
            segment.advances.clone(),
            segment.width,
            segment.origin,
            segment.glyphs.len()
        ),
        (vec![10.0, 22.5], 0.0, [0.0, 0.0], 0)
    );
}

#[test]
fn box_text_takes_the_bounds_of_its_outline() {
    let outline = "-110.0 -70.0 -110.0 -70.0 110.0 -70.0 110.0 -70.0 110.0 -70.0 110.0 -70.0 \
                   110.0 70.0 110.0 70.0 110.0 70.0 110.0 70.0 -110.0 70.0 -110.0 70.0 \
                   -110.0 70.0 -110.0 70.0 -110.0 -70.0 -110.0 -70.0";
    let frame = format!("<< /0 << /1 << /0 [ {outline} ] >> /2 << /0 1 >> >> >>");
    let document = text_document("x\r", &[(2, "")], &[(2, "")], "");
    let documents = read_body(&["Inter-Bold"], &[&frame], &[document]).unwrap();
    assert_eq!(
        documents[0].frame,
        TextFrame::Box {
            left: -110.0,
            top: -70.0,
            width: 220.0,
            height: 140.0
        }
    );
    let empty = "<< /0 << /1 << /0 [ ] >> >> >>";
    let document = text_document("x\r", &[(2, "")], &[(2, "")], "");
    let documents = read_body(&["Inter-Bold"], &[empty], &[document]).unwrap();
    assert_eq!(documents[0].frame, TextFrame::Point);
}

#[test]
fn keyframed_text_holds_one_document_per_keyframe() {
    let documents: Vec<String> = ["One\r", "Two\r"]
        .iter()
        .map(|text| text_document(text, &[(4, "")], &[(4, "")], ""))
        .collect();
    let texts: Vec<String> = read_body(&["Inter-Bold"], &[], &documents)
        .unwrap()
        .into_iter()
        .map(|document| document.text)
        .collect();
    assert_eq!(texts, ["One", "Two"]);
}

#[test]
fn utf16_text_and_counts_map_to_byte_ranges() {
    // "é😀\r" in UTF-16: é is one unit, the emoji two, the break one.
    let text: Vec<u8> = [0xFE, 0xFF]
        .into_iter()
        .chain("é😀\r".encode_utf16().flat_map(u16::to_be_bytes))
        .collect();
    let escaped: Vec<u8> = text
        .iter()
        .flat_map(|&byte| match byte {
            b'(' | b')' | b'\\' => vec![b'\\', byte],
            byte => vec![byte],
        })
        .collect();
    let placeholder = text_document("TEXT", &[(4, "")], &[(1, ""), (3, "")], "");
    let body = text_body(&["Inter-Bold"], &[], &[placeholder]).into_bytes();
    let at = body
        .windows(4)
        .position(|window| window == b"TEXT")
        .expect("the placeholder");
    let body = [&body[..at], &escaped, &body[at + 4..]].concat();
    let documents = documents_of(&body).unwrap();
    let document = &documents[0];
    assert_eq!(document.text, "é😀");
    let ranges: Vec<_> = document
        .characters
        .iter()
        .map(|run| run.range.clone())
        .collect();
    assert_eq!(ranges, [0..2, 2..6]);
}

#[test]
fn byte_ranges_clip_and_keep_split_characters_whole() {
    let ranges = byte_ranges("a😀b", [1, 1, 1, 9, 0].into_iter());
    // The run that ends inside the emoji's pair gets none of it; the next
    // run, where the emoji ends, takes it whole.
    assert_eq!(ranges, [0..1, 1..1, 1..5, 5..6, 6..6]);
    assert_eq!(byte_ranges("", [3].into_iter()), vec![0..0; 1]);
}

#[test]
fn sparse_runs_inherit_their_sheet_chain_and_the_default() {
    // Sheet 1 (parent 0) sets size 30; sheet 0 sets tracking 50; the
    // document default gives every other key; the run sets caps.
    let document = text_document("Hi\r", &[(3, "")], &[(3, "/12 2")], "");
    let body = text_body(&["Inter-Bold"], &[], &[document])
        .replace(
            "/5 << /0 [ << /0 << /0 (Normal RGB) /6 << >> >> >> ] >>",
            "/5 << /0 [ << /0 << /0 (Normal RGB) /6 << /8 50 >> >> >> \
             << /0 << /0 (Body) /5 0 /6 << /1 30.0 >> >> >> ] >>",
        )
        .replace("/0 () /5 0 /6 << /12 2 >>", "/0 () /5 1 /6 << /12 2 >>");
    let documents = documents_of(body.as_bytes()).unwrap();
    let style = &documents[0].characters[0].style;
    assert_eq!(
        (style.size, style.tracking, style.caps, style.fill.is_some()),
        (30.0, 50.0, Caps::All, true)
    );
}

#[test]
fn sheet_cycles_and_missing_sheets_fall_back() {
    let document = text_document("Hi\r", &[(3, "")], &[(3, "/1 12.0")], "");
    let body = text_body(&["Inter-Bold"], &[], &[document])
        .replace(
            "/5 << /0 [ << /0 << /0 (Normal RGB) /6 << >> >> >> ] >>",
            "/5 << /0 [ << /0 << /0 (A) /5 1 /6 << /8 1 >> >> >> \
             << /0 << /0 (B) /5 0 /6 << /8 2 >> >> >> << /0 (no payload) >> ] >>",
        )
        .replace("/0 () /5 0 /6 << /1 12.0 >>", "/0 () /5 9 /6 << /1 12.0 >>");
    let documents = documents_of(body.as_bytes()).unwrap();
    let style = &documents[0].characters[0].style;
    // Sheet 9 does not exist, so the run sits on the default.
    assert_eq!((style.size, style.tracking), (12.0, 0.0));
}

#[test]
fn refuses_a_font_past_the_table() {
    let document = text_document("Hi\r", &[(3, "")], &[(3, "/0 4")], "");
    let error = read_body(&["Inter-Bold"], &[], &[document]).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert_eq!(
        error.path.as_deref(),
        Some("LIST:btdk/1/1/0/0/6/0/0/0/0/6/0")
    );
    assert!(error.message.contains("font 4"), "{error}");
}

#[test]
fn refuses_bodies_without_the_records_it_needs() {
    let error = documents_of(b" /0 << >> /1 << >>").unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert_eq!(error.path.as_deref(), Some("LIST:btdk/0"));
    let error = documents_of(b" /0 (").unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::Syntax, Some(AT))
    );
}

#[test]
fn the_tree_prints_a_document_compactly() {
    let document = crate::test_support::sample_document();
    assert_eq!(
        Value::Text(Box::new(document)).to_string(),
        "text \"Hello\\nworld\" (0 lines), box 220x140, Inter-Bold 44, fill [1, 1, 1, 1], \
         stroke [1, 0, 0, 1], center"
    );
}

/// A body for the never-panic tests: two documents, a frame, a cache.
fn realistic_body() -> Vec<u8> {
    let cache = composed(&[(0.0, &[(3, -5.0, "1 2", 10.0)])]);
    let documents = [
        text_document("Hi\r", &[(3, "")], &[(1, "/12 2"), (2, "")], &cache),
        text_document("Yo\r", &[(3, "/0 1")], &[(3, "")], ""),
    ];
    let frame = "<< /0 << /1 << /0 [ 0.0 0.0 4.0 2.0 ] >> >> >>";
    text_body(&["Inter-Bold"], &[frame], &documents).into_bytes()
}

proptest! {
    #[test]
    fn never_panics_on_a_truncated_body(cut in 0usize..4096) {
        let body = realistic_body();
        let _ = documents_of(&body[..cut.min(body.len())]);
    }

    #[test]
    fn never_panics_on_a_corrupted_body(at in 0usize..4096, byte in proptest::prelude::any::<u8>()) {
        let mut body = realistic_body();
        let at = at % body.len();
        body[at] = byte;
        let _ = documents_of(&body);
    }

    #[test]
    fn never_panics_on_corrupted_digits(at in 0usize..4096, digit in 0u8..10) {
        // Digits keep the syntax valid, so the typed read and the model see
        // other counts, indices and sizes.
        let mut body = realistic_body();
        let digits: Vec<usize> = (0..body.len()).filter(|&i| body[i].is_ascii_digit()).collect();
        let at = digits[at % digits.len()];
        body[at] = b'0' + digit;
        let _ = documents_of(&body);
    }

    #[test]
    fn byte_ranges_always_slice_the_text(
        text in "\\PC{0,12}",
        counts in proptest::collection::vec(0usize..6, 0..8),
    ) {
        let ranges = byte_ranges(&text, counts.into_iter());
        let mut start = 0;
        for range in ranges {
            prop_assert_eq!(range.start, start);
            prop_assert!(text.get(range.clone()).is_some());
            start = range.end;
        }
    }
}
