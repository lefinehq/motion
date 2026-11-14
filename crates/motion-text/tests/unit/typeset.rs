//! Unit tests for `src/typeset.rs`.

use kurbo::Size;
use motion_format::Kind;
use proptest::prelude::*;
use rstest::rstest;
use skrifa::instance::{LocationRef, Size as FontSize};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use super::*;
use crate::set::FontQuery;
use crate::style::{AUTO_LEADING, Caps, CharacterStyle, StyledRun};

const REGULAR: &str = "InterVariable";
const BOLD: &str = "InterVariable-Bold";

fn style(name: &str, size: f64) -> CharacterStyle {
    CharacterStyle::new(FontQuery::PostScript(name.into()), size)
}

fn setter() -> Typesetter {
    Typesetter::new(FontSet::builtin())
}

fn lay(text: &StyledText) -> TextLayout {
    setter().layout(text).expect("the text lays out")
}

fn plain(text: &str, name: &str, size: f64) -> TextLayout {
    lay(&StyledText::plain(text, style(name, size)))
}

fn boxed(text: &str, name: &str, size: f64, justification: Justification) -> StyledText {
    let mut styled = StyledText::plain(text, style(name, size));
    styled.frame = TextFrame::Box {
        left: 10.0,
        top: 20.0,
        width: 300.0,
        height: 400.0,
    };
    styled
        .paragraphs
        .iter_mut()
        .for_each(|p| p.justification = justification);
    styled
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

/// The face `name` of the built-in set, its font and the location of its instance.
fn instance(name: &str) -> (Face, Vec<i16>) {
    let face = FontSet::builtin()
        .find(&FontQuery::PostScript(name.into()))
        .unwrap()
        .clone();
    let font = FontRef::from_index(face.data.data.as_ref(), face.data.index).unwrap();
    let location = font.axes().location(face.variations.iter().copied());
    let coords = location.coords().iter().map(|c| c.to_bits()).collect();
    (face, coords)
}

#[rstest]
#[case::regular(REGULAR)]
#[case::bold(BOLD)]
#[case::black_italic("InterVariableItalic-Black")]
fn advances_are_the_fonts_own(#[case] name: &str) {
    let (face, coords) = instance(name);
    let font = FontRef::from_index(face.data.data.as_ref(), face.data.index).unwrap();
    let skrifa_coords: Vec<_> = coords
        .iter()
        .map(|&c| skrifa::instance::NormalizedCoord::from_bits(c))
        .collect();
    let metrics = font.glyph_metrics(FontSize::new(100.0), LocationRef::new(&skrifa_coords));
    let charmap = font.charmap();
    for c in "Hamburgefonstiv".chars() {
        let layout = plain(&c.to_string(), name, 100.0);
        let run = &layout.runs[0];
        let glyph = &run.glyphs[0];
        let id = charmap.map(c).unwrap();
        assert_eq!(glyph.id, id.to_u32(), "{c}");
        let expected = f64::from(metrics.advance_width(GlyphId::new(glyph.id)).unwrap());
        assert!(
            close(glyph.advance, expected),
            "{c}: {} vs {expected}",
            glyph.advance
        );
        assert!(close(layout.lines[0].advances[0], expected));
        // Zero coordinates are the font's default instance, which Parley leaves out.
        let trimmed: Vec<i16> = match coords.iter().all(|&c| c == 0) {
            true => Vec::new(),
            false => coords.clone(),
        };
        assert_eq!(run.coords, trimmed);
    }
}

#[test]
fn a_heavier_instance_sets_wider() {
    let regular = plain("Hello", REGULAR, 100.0).lines[0].width;
    let bold = plain("Hello", BOLD, 100.0).lines[0].width;
    assert!(bold > regular + 1.0, "{bold} vs {regular}");
}

#[test]
fn point_text_starts_at_the_first_baseline() {
    let layout = plain("Hello world", REGULAR, 100.0);
    let line = &layout.lines[0];
    assert_eq!(
        (line.x, line.baseline, line.range.clone()),
        (0.0, 0.0, 0..11)
    );
    assert_eq!(line.characters, 11);
    assert_eq!(line.advances.len(), 11);
    assert!(close(*line.advances.last().unwrap(), line.width));
    // Glyphs follow on from one another, from the origin.
    let glyphs = &layout.runs[0].glyphs;
    assert_eq!(glyphs[0].x, 0.0);
    for pair in glyphs.windows(2) {
        assert!(close(pair[0].x + pair[0].advance, pair[1].x));
    }
    for (glyph, i) in glyphs.iter().zip(0..) {
        assert_eq!(glyph.cluster, i..i + 1);
        assert_eq!(glyph.y, 0.0);
    }
}

#[rstest]
#[case::none(0.0)]
#[case::loose(100.0)]
#[case::tight(-50.0)]
fn tracking_follows_every_character(#[case] tracking: f64) {
    let text = "Track";
    let base = plain(text, REGULAR, 80.0);
    let mut styled = StyledText::plain(text, style(REGULAR, 80.0));
    styled.runs[0].style.tracking = tracking;
    let tracked = lay(&styled);
    let per = tracking / 1000.0 * 80.0;
    for (a, b) in base.runs[0].glyphs.iter().zip(&tracked.runs[0].glyphs) {
        assert!(close(b.advance - a.advance, per));
    }
    let gained = tracked.lines[0].width - base.lines[0].width;
    assert!(close(gained, per * text.len() as f64), "{gained}");
}

#[test]
fn box_text_breaks_between_words_that_do_not_fit() {
    let text = "The quick brown fox jumps over the lazy dog";
    let layout = lay(&boxed(text, BOLD, 40.0, Justification::Left));
    let ranges: Vec<_> = layout.lines.iter().map(|l| l.range.clone()).collect();
    assert_eq!(ranges, [0..10, 10..20, 20..35, 35..43]);
    for (i, line) in layout.lines.iter().enumerate() {
        assert!(line.width <= 300.0);
        assert_eq!(line.x, 10.0);
        // A line that wraps has no room for the next word.
        if let Some(next) = layout.lines.get(i + 1) {
            let word = text[next.range.clone()].split(' ').next().unwrap();
            let longer = format!("{}{word}", &text[line.range.clone()]);
            assert!(plain(&longer, BOLD, 40.0).lines[0].width > 300.0);
        }
    }
    let first = &layout.lines[0];
    assert!(close(first.baseline, 20.0 + first.ascent));
    for pair in layout.lines.windows(2) {
        assert!(close(
            pair[1].baseline - pair[0].baseline,
            40.0 * AUTO_LEADING
        ));
    }
}

#[rstest]
#[case::left(Justification::Left, 0.0)]
#[case::right(Justification::Right, 1.0)]
#[case::center(Justification::Center, 0.5)]
fn lines_align_in_their_box_and_about_the_origin(
    #[case] justification: Justification,
    #[case] anchor: f64,
) {
    let in_box = lay(&boxed("Hello", REGULAR, 50.0, justification));
    let line = &in_box.lines[0];
    assert!(
        close(line.x, 10.0 + anchor * (300.0 - line.width)),
        "{}",
        line.x
    );
    assert!(close(in_box.runs[0].glyphs[0].x, line.x));
    let mut point = StyledText::plain("Hello", style(REGULAR, 50.0));
    point.paragraphs[0].justification = justification;
    let point = lay(&point);
    let line = &point.lines[0];
    assert!(close(line.x, -anchor * line.width));
    assert!(close(point.runs[0].glyphs[0].x, line.x));
}

#[rstest]
#[case::last_left(Justification::JustifyLastLeft, Some(0.0))]
#[case::last_right(Justification::JustifyLastRight, Some(1.0))]
#[case::last_center(Justification::JustifyLastCenter, Some(0.5))]
#[case::all(Justification::JustifyAll, None)]
fn justified_lines_fill_the_box(#[case] justification: Justification, #[case] last: Option<f64>) {
    let text = "The quick brown fox jumps over the lazy dog";
    let layout = lay(&boxed(text, BOLD, 40.0, justification));
    let (end, rest) = layout.lines.split_last().unwrap();
    for line in rest {
        assert!(close(line.width, 300.0), "{}", line.width);
        assert!(close(
            *line.advances.last().unwrap() - line.advances[line.advances.len() - 2],
            line.advances.last().unwrap() - line.advances[line.advances.len() - 2]
        ));
        assert_eq!(line.x, 10.0);
    }
    match last {
        Some(anchor) => assert!(close(end.x, 10.0 + anchor * (300.0 - end.width))),
        None => assert!(close(end.width, 300.0) && end.x == 10.0),
    }
    // Justification widens the spaces: the line's last glyph ends at its width.
    let line_runs: Vec<_> = layout.runs.iter().filter(|r| r.line == 0).collect();
    let last_glyph = line_runs
        .last()
        .unwrap()
        .glyphs
        .iter()
        .rev()
        .nth(1)
        .unwrap();
    assert!(close(last_glyph.x + last_glyph.advance, 10.0 + 300.0));
}

#[test]
fn indents_narrow_and_move_box_lines() {
    let text = "The quick brown fox jumps over the lazy dog";
    let mut styled = boxed(text, REGULAR, 30.0, Justification::Left);
    let paragraph = &mut styled.paragraphs[0];
    paragraph.start_indent = 20.0;
    paragraph.end_indent = 30.0;
    paragraph.first_line_indent = 40.0;
    let layout = lay(&styled);
    assert!(close(layout.lines[0].x, 10.0 + 20.0 + 40.0));
    assert!(layout.lines[0].width <= 300.0 - 20.0 - 30.0 - 40.0);
    for line in &layout.lines[1..] {
        assert!(close(line.x, 30.0));
        assert!(line.width <= 250.0);
    }
}

#[rstest]
#[case::auto(Leading::Auto, [0.0, 60.0, 120.0])]
#[case::fixed(Leading::Px(80.0), [0.0, 80.0, 160.0])]
fn baselines_step_by_the_leading(#[case] leading: Leading, #[case] baselines: [f64; 3]) {
    let mut styled = StyledText::plain("A\nB\nC", style(REGULAR, 50.0));
    styled.runs[0].style.leading = leading;
    let layout = lay(&styled);
    let found: Vec<f64> = layout.lines.iter().map(|l| l.baseline).collect();
    assert!(
        found.iter().zip(baselines).all(|(&a, b)| close(a, b)),
        "{found:?}"
    );
    let paragraphs: Vec<usize> = layout.lines.iter().map(|l| l.paragraph).collect();
    assert_eq!(paragraphs, [0, 1, 2]);
}

#[test]
fn paragraph_space_comes_between_paragraphs() {
    let mut styled = StyledText::plain("A\nB\nC", style(REGULAR, 50.0));
    styled.paragraphs[0].space_before = 1000.0;
    styled.paragraphs[1].space_before = 10.0;
    styled.paragraphs[1].space_after = 5.0;
    let layout = lay(&styled);
    let found: Vec<f64> = layout.lines.iter().map(|l| l.baseline).collect();
    assert!(
        close(found[0], 0.0) && close(found[1], 70.0) && close(found[2], 135.0),
        "{found:?}"
    );
}

#[test]
fn baseline_shift_raises_glyphs() {
    let mut styled = StyledText::plain("x2", style(REGULAR, 40.0));
    styled.runs = vec![
        StyledRun {
            range: 0..1,
            style: style(REGULAR, 40.0),
        },
        StyledRun {
            range: 1..2,
            style: CharacterStyle {
                baseline_shift: 12.0,
                ..style(REGULAR, 40.0)
            },
        },
    ];
    let layout = lay(&styled);
    let ys: Vec<f64> = layout.runs.iter().map(|r| r.glyphs[0].y).collect();
    assert_eq!(ys, [0.0, -12.0]);
    assert_eq!(
        layout.runs.iter().map(|r| r.style_run).collect::<Vec<_>>(),
        [0, 1]
    );
}

#[test]
fn runs_keep_their_own_faces() {
    let mut styled = StyledText::plain("ab", style(REGULAR, 40.0));
    styled.runs = vec![
        StyledRun {
            range: 0..1,
            style: style(REGULAR, 40.0),
        },
        StyledRun {
            range: 1..2,
            style: style(BOLD, 40.0),
        },
    ];
    let layout = lay(&styled);
    let set = FontSet::builtin();
    let faces: Vec<&str> = layout
        .runs
        .iter()
        .map(|r| set.faces()[r.face].postscript_name.as_str())
        .collect();
    assert_eq!(faces, [REGULAR, BOLD]);
    assert_eq!(layout.runs[1].coords, instance(BOLD).1);
}

#[test]
fn all_caps_upper_cases_before_shaping() {
    let mut styled = StyledText::plain("Straße", style(REGULAR, 40.0));
    styled.runs[0].style.caps = Caps::All;
    let layout = lay(&styled);
    let upper = plain("STRASSE", REGULAR, 40.0);
    let ids = |l: &TextLayout| l.runs[0].glyphs.iter().map(|g| g.id).collect::<Vec<_>>();
    assert_eq!(ids(&layout), ids(&upper));
    // `ß` is two bytes of the source and sets as two glyphs; each shows it.
    let clusters: Vec<_> = layout.runs[0]
        .glyphs
        .iter()
        .map(|g| g.cluster.clone())
        .collect();
    assert_eq!(clusters[4..], [4..6, 4..6, 6..7]);
    let line = &layout.lines[0];
    assert_eq!((line.range.clone(), line.advances.len()), (0..7, 6));
    assert!(close(line.advances[5], upper.lines[0].width));
}

#[test]
fn small_caps_set_lower_case_smaller() {
    let mut styled = StyledText::plain("Ab", style(REGULAR, 100.0));
    styled.runs[0].style.caps = Caps::Small;
    let layout = lay(&styled);
    let sizes: Vec<f64> = layout.runs.iter().map(|r| r.size).collect();
    assert!(close(sizes[0], 100.0) && close(sizes[1], 70.0), "{sizes:?}");
    let b = plain("B", REGULAR, 70.0).runs[0].glyphs[0].id;
    assert_eq!(layout.runs[1].glyphs[0].id, b);
}

#[test]
fn empty_paragraphs_still_take_a_line() {
    let layout = plain("A\n\nB", REGULAR, 50.0);
    let lines: Vec<_> = layout
        .lines
        .iter()
        .map(|l| (l.range.clone(), l.advances.len()))
        .collect();
    assert_eq!(lines, [(0..1, 1), (2..2, 0), (3..4, 1)]);
    assert!(close(layout.lines[2].baseline, 120.0));
    let trailing = plain("A\n", REGULAR, 50.0);
    assert_eq!(trailing.lines.len(), 2);
}

#[test]
fn an_empty_text_has_no_lines() {
    let empty = StyledText {
        runs: Vec::new(),
        ..StyledText::plain("", style(REGULAR, 10.0))
    };
    let mut setter = setter();
    assert_eq!(setter.layout(&empty).unwrap().lines, []);
    assert_eq!(setter.measure(&empty).unwrap(), Size::ZERO);
}

#[test]
fn measure_is_the_size_of_the_bounds() {
    let mut setter = setter();
    let text = boxed(
        "The quick brown fox jumps",
        REGULAR,
        30.0,
        Justification::Left,
    );
    let layout = setter.layout(&text).unwrap();
    assert_eq!(setter.measure(&text).unwrap(), layout.size());
    let height = layout.lines.last().unwrap().baseline + layout.lines[0].descent - 20.0;
    assert!(close(layout.size().height, height));
}

#[test]
fn control_characters_show_nothing() {
    let layout = plain("a\tb", REGULAR, 40.0);
    let clusters: Vec<_> = layout.runs[0]
        .glyphs
        .iter()
        .map(|g| g.cluster.clone())
        .collect();
    assert_eq!(clusters, [0..1, 2..3]);
    assert_eq!(layout.lines[0].advances.len(), 3);
}

#[test]
fn a_line_separator_breaks_the_line_inside_its_paragraph() {
    let layout = plain("AB\u{2028}C", REGULAR, 50.0);
    let lines: Vec<_> = layout
        .lines
        .iter()
        .map(|l| (l.paragraph, l.range.clone()))
        .collect();
    assert_eq!(lines, [(0, 0..5), (0, 5..6)]);
    assert!(close(layout.lines[1].baseline, 60.0));
    let clusters: Vec<_> = layout
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| g.cluster.clone()))
        .collect();
    assert_eq!(clusters, [0..1, 1..2, 5..6]);
}

#[test]
fn utf16_lengths_count_characters() {
    let layout = plain("Ωé", REGULAR, 40.0);
    assert_eq!(
        (layout.lines[0].range.clone(), layout.lines[0].characters),
        (0..4, 2)
    );
}

#[test]
fn an_unknown_font_is_an_error() {
    let text = StyledText::plain("a", style("Nope-Bold", 10.0));
    let error = setter().layout(&text).unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::UnknownName, Some("runs[0].style.font"))
    );
    assert!(error.expected.unwrap().contains(REGULAR));
}

#[rstest]
#[case::cjk("a日")]
#[case::emoji("ok 🙂")]
fn a_character_without_a_glyph_is_an_error(#[case] text: &str) {
    let error = setter()
        .layout(&StyledText::plain(text, style(REGULAR, 10.0)))
        .unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref(), error.expected.as_deref()),
        (
            Kind::Missing,
            Some("runs[0]"),
            Some("a font that covers every character of its run")
        )
    );
}

#[test]
fn an_invalid_text_is_refused_before_layout() {
    let mut text = StyledText::plain("ab", style(REGULAR, 10.0));
    text.runs[0].range = 0..1;
    let error = setter().layout(&text).unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::Invalid, Some("runs"))
    );
}

#[test]
fn error_messages() {
    let mut setter = setter();
    let mut report = Vec::new();
    let mut push = |what: &str, text: StyledText| {
        let error = setter.layout(&text).unwrap_err();
        report.push(format!("{what}: {error}"));
    };
    push(
        "unknown font",
        StyledText::plain("a", style("Nope-Bold", 10.0)),
    );
    push(
        "missing glyph",
        StyledText::plain("a日", style(REGULAR, 10.0)),
    );
    let mut bad = StyledText::plain("a", style(REGULAR, 0.0));
    push("zero size", bad.clone());
    bad.runs[0].style.size = 10.0;
    bad.paragraphs.clear();
    push("paragraph count", bad);
    let mut font = FontSet::new();
    report.push(format!(
        "not a font: {}",
        font.add_bytes(b"nope".to_vec(), "nope.ttf").unwrap_err()
    ));
    insta::assert_snapshot!(report.join("\n"));
}

#[test]
fn glyph_runs_draw_with_the_renderer() {
    use vello_cpu::peniko::color::palette::css::BLACK;
    use vello_cpu::{Pixmap, RenderContext, Resources};

    let (left, top) = (10.0, 80.0);
    let mut styled = StyledText::plain("Hello", style(BOLD, 64.0));
    styled.frame = TextFrame::Box {
        left,
        top: top - 60.0,
        width: 300.0,
        height: 100.0,
    };
    let layout = lay(&styled);
    let (width, height) = (320u16, 120u16);
    let mut ctx = RenderContext::new(width, height);
    let mut resources = Resources::new();
    ctx.set_paint(BLACK);
    for run in &layout.runs {
        // The font data and the coordinates are the renderer's own types.
        let font: &vello_cpu::peniko::FontData = &run.font;
        ctx.glyph_run(&mut resources, font)
            .font_size(run.size as f32)
            .normalized_coords(&run.coords)
            .fill_glyphs(run.glyphs.iter().map(|g| vello_cpu::Glyph {
                id: g.id,
                x: g.x as f32,
                y: g.y as f32,
            }))
            .unwrap();
    }
    ctx.flush();
    let mut pixmap = Pixmap::new(width, height);
    ctx.render(&mut pixmap, &mut resources);
    let inked: Vec<(usize, usize)> = pixmap
        .data()
        .iter()
        .enumerate()
        .filter(|(_, p)| p.a > 128)
        .map(|(i, _)| (i % usize::from(width), i / usize::from(width)))
        .collect();
    assert!(inked.len() > 500, "{} pixels", inked.len());
    let line = &layout.lines[0];
    let (min_x, max_x) = inked
        .iter()
        .fold((usize::MAX, 0), |(a, b), &(x, _)| (a.min(x), b.max(x)));
    let (min_y, max_y) = inked
        .iter()
        .fold((usize::MAX, 0), |(a, b), &(_, y)| (a.min(y), b.max(y)));
    // The ink sits on the line: from its start to its width, between its ascent and baseline.
    assert!(
        (min_x as f64) >= line.x && (min_x as f64) < line.x + 10.0,
        "{min_x}"
    );
    assert!((max_x as f64) <= line.x + line.width && (max_x as f64) > line.x + line.width - 10.0);
    assert!((min_y as f64) >= line.baseline - line.ascent);
    assert!((max_y as f64) <= line.baseline + 1.0 && (max_y as f64) > line.baseline - 2.0);
}

#[test]
fn debug_names_the_face_count() {
    let setter = setter();
    assert_eq!(setter.fonts().faces().len(), 18);
    assert_eq!(format!("{setter:?}"), "Typesetter { faces: 18, .. }");
}

/// A styled text of `chars`, cut into runs at `cuts` (taken as fractions).
fn arbitrary_text(chars: Vec<char>, cuts: Vec<f64>, sizes: Vec<f64>) -> StyledText {
    let text: String = chars.into_iter().collect();
    let boundaries: Vec<usize> = text.char_indices().map(|(i, _)| i).skip(1).collect();
    let mut ends: Vec<usize> = cuts
        .iter()
        .filter_map(|f| {
            boundaries
                .get((f * boundaries.len() as f64) as usize)
                .copied()
        })
        .collect();
    ends.push(text.len());
    ends.sort_unstable();
    ends.dedup();
    let faces = [REGULAR, BOLD, "InterVariableItalic-Light"];
    let mut start = 0;
    let runs = ends
        .into_iter()
        .filter(|&end| end > 0)
        .enumerate()
        .map(|(i, end)| {
            let run = StyledRun {
                range: start..end,
                style: CharacterStyle {
                    caps: [Caps::Normal, Caps::All, Caps::Small, Caps::AllSmall][i % 4],
                    tracking: sizes[i % sizes.len()] * 3.0 - 200.0,
                    baseline_shift: sizes[i % sizes.len()] / 10.0,
                    ..style(faces[i % faces.len()], sizes[i % sizes.len()])
                },
            };
            start = end;
            run
        })
        .collect();
    StyledText {
        runs,
        ..StyledText::plain(text, style(REGULAR, 10.0))
    }
}

fn justification() -> impl Strategy<Value = Justification> {
    prop::sample::select(vec![
        Justification::Left,
        Justification::Right,
        Justification::Center,
        Justification::JustifyLastLeft,
        Justification::JustifyLastRight,
        Justification::JustifyLastCenter,
        Justification::JustifyAll,
    ])
}

/// Checks what every layout promises about its source text.
fn check_layout(text: &StyledText, layout: &TextLayout) {
    for line in &layout.lines {
        assert!(text.text.is_char_boundary(line.range.start));
        assert!(text.text.is_char_boundary(line.range.end));
        assert_eq!(
            line.advances.len(),
            text.text[line.range.clone()].chars().count()
        );
        assert!(line.width.is_finite() && line.baseline.is_finite());
    }
    for run in &layout.runs {
        for glyph in &run.glyphs {
            assert!(text.text.get(glyph.cluster.clone()).is_some());
            assert!(glyph.x.is_finite() && glyph.y.is_finite());
        }
    }
}

proptest! {
    #[test]
    fn layout_never_panics(
        chars in prop::collection::vec(any::<char>(), 0..24),
        cuts in prop::collection::vec(0.0..1.0f64, 0..4),
        sizes in prop::collection::vec(1.0..200.0f64, 1..4),
        width in 1.0..400.0f64,
        boxed in any::<bool>(),
        justification in justification(),
    ) {
        let mut text = arbitrary_text(chars, cuts, sizes);
        text.paragraphs = vec![ParagraphStyle { justification, ..ParagraphStyle::default() }; text.paragraphs.len()];
        if boxed {
            text.frame = TextFrame::Box { left: 0.0, top: 0.0, width, height: 100.0 };
        }
        if let Ok(layout) = setter().layout(&text) {
            check_layout(&text, &layout);
        }
    }

    #[test]
    fn latin_text_always_lays_out(
        chars in prop::collection::vec(prop::sample::select(" \n\tabcxyzABCßéÆ.,!-0129fiffl".chars().collect::<Vec<_>>()), 0..60),
        cuts in prop::collection::vec(0.0..1.0f64, 0..4),
        sizes in prop::collection::vec(4.0..120.0f64, 1..4),
        width in 1.0..400.0f64,
        justification in justification(),
    ) {
        let mut text = arbitrary_text(chars, cuts, sizes);
        text.paragraphs = vec![ParagraphStyle { justification, first_line_indent: 12.0, start_indent: 5.0, ..ParagraphStyle::default() }; text.paragraphs.len()];
        text.frame = TextFrame::Box { left: 0.0, top: 0.0, width, height: 100.0 };
        let layout = setter().layout(&text).unwrap();
        check_layout(&text, &layout);
        prop_assert_eq!(layout.lines.len() >= text.paragraphs.len(), !text.runs.is_empty());
    }
}
