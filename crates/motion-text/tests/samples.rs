//! The local comparison: lays out every text document of the sample projects
//! under `$MOTION_AEP_SAMPLES` in the fonts of `$MOTION_AEP_SAMPLES/fonts/`
//! and prints our lines next to the ones the project stored. Sample projects
//! and their fonts live there, never in the repository.
//!
//! ```text
//! MOTION_AEP_SAMPLES=~/aep-samples cargo test -p motion-text --test samples -- --ignored --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use motion_import::{ItemKind, Project, Stream, TextDocument, Value};
use motion_text::{
    CharacterStyle, FontQuery, FontSet, Justification, Leading, ParagraphStyle, StyledRun,
    StyledText, TextFrame, TextLayout, Typesetter,
};

/// Every `.aep` file under `dir`, sorted, at any depth.
fn projects(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read the samples folder")
        .map(|entry| entry.expect("read a folder entry").path())
        .flat_map(|path| match path.is_dir() {
            true => projects(&path),
            false => vec![path],
        })
        .filter(|path| path.extension().is_some_and(|e| e == "aep"))
        .collect();
    found.sort();
    found
}

/// Every text document of a project's layers, as `(layer name, document)`.
fn documents(project: &Project) -> Vec<(String, TextDocument)> {
    let mut found = Vec::new();
    for item in &project.items {
        let ItemKind::Composition(composition) = &item.kind else {
            continue;
        };
        for layer in &composition.layers {
            for property in layer.properties.properties() {
                let values: Vec<&Value> = match &property.value {
                    Stream::Static(value) => vec![value],
                    Stream::Animated(keys) => keys.iter().map(|k| &k.value).collect(),
                };
                found.extend(values.into_iter().filter_map(|value| match value {
                    Value::Text(document) => Some((layer.name.clone(), (**document).clone())),
                    _ => None,
                }));
            }
        }
    }
    found
}

/// The project's justifications and ours.
const JUSTIFICATIONS: &[(motion_import::Justification, Justification)] = &[
    (motion_import::Justification::Left, Justification::Left),
    (motion_import::Justification::Right, Justification::Right),
    (motion_import::Justification::Center, Justification::Center),
    (
        motion_import::Justification::JustifyLastLeft,
        Justification::JustifyLastLeft,
    ),
    (
        motion_import::Justification::JustifyLastRight,
        Justification::JustifyLastRight,
    ),
    (
        motion_import::Justification::JustifyLastCenter,
        Justification::JustifyLastCenter,
    ),
    (
        motion_import::Justification::JustifyAll,
        Justification::JustifyAll,
    ),
];

/// The project's capitals and ours.
const CAPS: &[(motion_import::Caps, motion_text::Caps)] = &[
    (motion_import::Caps::Normal, motion_text::Caps::Normal),
    (motion_import::Caps::Small, motion_text::Caps::Small),
    (motion_import::Caps::All, motion_text::Caps::All),
    (motion_import::Caps::AllSmall, motion_text::Caps::AllSmall),
];

fn lookup<A: PartialEq, B: Copy>(table: &[(A, B)], key: &A, default: B) -> B {
    table
        .iter()
        .find(|(a, _)| a == key)
        .map_or(default, |&(_, b)| b)
}

/// The document as the typesetter's input.
fn styled(document: &TextDocument) -> StyledText {
    let runs = document
        .characters
        .iter()
        .map(|run| {
            let style = &run.style;
            StyledRun {
                range: run.range.clone(),
                style: CharacterStyle {
                    tracking: style.tracking,
                    leading: match style.auto_leading {
                        true => Leading::Auto,
                        false => Leading::Px(style.leading),
                    },
                    baseline_shift: style.baseline_shift,
                    caps: lookup(CAPS, &style.caps, motion_text::Caps::Normal),
                    faux_bold: style.faux_bold,
                    faux_italic: style.faux_italic,
                    ..CharacterStyle::new(FontQuery::PostScript(style.font.clone()), style.size)
                },
            }
        })
        .collect();
    let mut text = StyledText::plain(
        document.text.clone(),
        CharacterStyle::new(FontQuery::PostScript(String::new()), 1.0),
    );
    // One style per paragraph of ours, from the document's run where it starts.
    text.paragraphs = text
        .paragraph_ranges()
        .iter()
        .map(|range| {
            let style = &document
                .paragraphs
                .iter()
                .find(|run| run.range.contains(&range.start))
                .or(document.paragraphs.last())
                .expect("a document has a paragraph")
                .style;
            ParagraphStyle {
                justification: lookup(JUSTIFICATIONS, &style.justification, Justification::Left),
                first_line_indent: style.first_line_indent,
                start_indent: style.start_indent,
                end_indent: style.end_indent,
                space_before: style.space_before,
                space_after: style.space_after,
                ..ParagraphStyle::default()
            }
        })
        .collect();
    text.runs = runs;
    text.frame = match document.frame {
        motion_import::TextFrame::Point => TextFrame::Point,
        motion_import::TextFrame::Box {
            left,
            top,
            width,
            height,
        } => TextFrame::Box {
            left,
            top,
            width,
            height,
        },
    };
    text
}

/// Prints our lines next to the stored ones.
fn compare(label: &str, document: &TextDocument, layout: &TextLayout) {
    let fonts: Vec<String> = document
        .characters
        .iter()
        .map(|c| {
            let bold = if c.style.faux_bold { " faux bold" } else { "" };
            format!("{} {}px{bold}", c.style.font, c.style.size)
        })
        .collect();
    println!("{label}: {:?} in {}", document.text, fonts.join(", "));
    for (i, ours) in layout.lines.iter().enumerate() {
        let Some(stored) = document.lines.get(i) else {
            println!("  line {i}: not stored");
            continue;
        };
        let width: f64 = stored.segments.iter().map(|s| s.width).sum();
        let ids: Vec<u32> = layout
            .runs
            .iter()
            .filter(|r| r.line == i)
            .flat_map(|r| r.glyphs.iter().map(|g| g.id))
            .collect();
        let stored_ids: Vec<u32> = stored
            .segments
            .iter()
            .flat_map(|s| s.glyphs.iter().copied())
            .collect();
        // The stored glyphs close with one for the line's break, which we do not draw.
        let same = stored_ids.starts_with(&ids) && stored_ids.len() <= ids.len() + 1;
        println!(
            "  line {i}: baseline {:.2} / {:.2}  width {:.2} / {:.2} ({:+.2})  glyphs {}",
            ours.baseline,
            stored.baseline,
            ours.width,
            width,
            ours.width - width,
            if same { "same" } else { "differ" },
        );
    }
}

#[test]
#[ignore = "needs $MOTION_AEP_SAMPLES"]
fn sample_text_lays_out_as_stored() {
    let dir = PathBuf::from(std::env::var("MOTION_AEP_SAMPLES").expect("set $MOTION_AEP_SAMPLES"));
    let mut fonts = FontSet::builtin();
    fonts
        .add_folder(&dir.join("fonts"))
        .expect("read the sample fonts");
    let mut setter = Typesetter::new(fonts);
    let mut compared = 0;
    for file in projects(&dir) {
        let bytes = fs::read(&file).expect("read a sample project");
        let Ok(project) = motion_import::decode(&bytes) else {
            continue;
        };
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        for (layer, document) in documents(&project) {
            let label = format!("{name} / {layer}");
            match setter.layout(&styled(&document)) {
                Ok(layout) => {
                    compared += 1;
                    compare(&label, &document, &layout);
                }
                Err(error) => println!("{label}: {error}"),
            }
        }
    }
    assert!(compared > 0, "no text document laid out");
}
