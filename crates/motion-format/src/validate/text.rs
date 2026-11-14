//! The rules of text layers: the base style from `font`, `fill` and `style`,
//! runs over character ranges, a style per paragraph, and box text.

use serde_json::Value as Json;

use crate::doc::{LayerDoc, List, Number, Object, StyleDoc, ValueDoc};
use crate::parse::key::{BOX, FILL, FONT, PARAGRAPHS, RUNS, STYLE};
use crate::shorthand::Font;
use crate::text::{
    CharacterStyle, Level, ParagraphStyle, STYLE_KEYS, Setter, StyleKey, TextContent, TextRun,
    color, style_key,
};
use crate::{Diagnostic, Kind, Result};

/// The key of a run that says which characters it styles.
const RANGE: &str = "range";
const RANGE_FORM: &str = "`[start, end]`, character indices with start before end, e.g. `[0, 4]`";

/// The text content of a text layer whose words are `text`, with paths
/// relative to the layer.
pub(super) fn text(
    doc: &LayerDoc,
    text: &str,
    default_font: Font,
    default_fill: [f64; 4],
) -> Result<TextContent> {
    let font = match &*doc.font {
        Some(written) => written.parse_at(FONT)?,
        None => default_font,
    };
    let fill = match &*doc.fill {
        Some(written) => color(written).map_err(|e| e.under(FILL))?,
        None => default_fill,
    };
    let mut base = CharacterStyle::new(font, fill);
    let mut paragraph = ParagraphStyle::default();
    let Object(style) = &doc.style;
    for (name, written) in style {
        let key = known(name, None).map_err(|e| e.at(name.as_str()).under(STYLE))?;
        if matches!(key.name, "font" | "fill") {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!("`{name}` is set on the layer, not in `{STYLE}`"),
            )
            .at(name.as_str())
            .under(STYLE)
            .expected(format!("`{name}` as a layer key")));
        }
        apply(key, written, &mut base, &mut paragraph)
            .map_err(|e| e.under(name.as_str()).under(STYLE))?;
    }
    let characters = text.chars().count();
    let mut styles = vec![(base.clone(), None); characters];
    if let Some(List(runs)) = &*doc.runs {
        for (i, Object(run)) in runs.iter().enumerate() {
            let place = |e: Diagnostic| e.under(format!("[{i}]")).under(RUNS);
            run_onto(run, i, base.clone(), &mut styles).map_err(place)?;
        }
    }
    let count = text.split('\n').count();
    let mut paragraphs = vec![paragraph; count];
    if let Some(List(written)) = &*doc.paragraphs {
        if written.len() > count {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("{} paragraph styles for {count} paragraphs", written.len()),
            )
            .at(PARAGRAPHS)
            .expected("at most one style per paragraph; `\\n` ends a paragraph"));
        }
        for (i, Object(style)) in written.iter().enumerate() {
            let place = |e: Diagnostic| e.under(format!("[{i}]")).under(PARAGRAPHS);
            for (name, value) in style {
                let key =
                    known(name, Some(Level::Paragraph)).map_err(|e| place(e.at(name.as_str())))?;
                let mut unused = base.clone();
                apply(key, value, &mut unused, &mut paragraphs[i])
                    .map_err(|e| place(e.under(name.as_str())))?;
            }
        }
    }
    Ok(TextContent {
        text: text.to_owned(),
        runs: runs_of(styles, &base),
        base,
        paragraphs,
        frame: doc
            .frame
            .as_ref()
            .map(frame)
            .transpose()
            .map_err(|e| e.under(BOX))?,
    })
}

/// The style key called `name`, which must be of `level` when one is given.
fn known(name: &str, level: Option<Level>) -> Result<&'static StyleKey> {
    let allowed = || {
        STYLE_KEYS
            .iter()
            .filter(move |k| level.is_none_or(|l| k.level() == l))
            .map(|k| k.name)
    };
    let key = style_key(name).ok_or_else(|| {
        let what = match level {
            Some(Level::Character) => "run key",
            Some(Level::Paragraph) => "paragraph key",
            None => "style key",
        };
        let mut names: Vec<&str> = allowed().collect();
        if level == Some(Level::Character) {
            names.insert(0, RANGE);
        }
        Diagnostic::unknown(what, name, names)
    })?;
    match level {
        Some(level) if key.level() != level => {
            let (here, there) = match level {
                Level::Character => ("a run", "`style` or `paragraphs`"),
                Level::Paragraph => ("a paragraph", "`style` or `runs`"),
            };
            Err(
                Diagnostic::new(Kind::Invalid, format!("`{name}` cannot be set on {here}"))
                    .expected(format!("`{name}` in {there}")),
            )
        }
        _ => Ok(key),
    }
}

/// Sets `key` from `written` on the character or paragraph style.
fn apply(
    key: &StyleKey,
    written: &Json,
    character: &mut CharacterStyle,
    paragraph: &mut ParagraphStyle,
) -> Result<()> {
    let result = match key.set {
        Setter::Character(set) => set(character, written),
        Setter::Paragraph(set) => set(paragraph, written),
    };
    result.map_err(|e| match e.expected {
        Some(_) => e,
        None => e.expected(key.form),
    })
}

/// One run: its range, then character keys over the base style, laid onto
/// the characters it covers. A later run wins where runs overlap.
fn run_onto(
    run: &StyleDoc,
    index: usize,
    base: CharacterStyle,
    styles: &mut [(CharacterStyle, Option<usize>)],
) -> Result<()> {
    let written = run.get(RANGE).ok_or_else(|| {
        Diagnostic::new(Kind::Missing, format!("missing `{RANGE}`")).expected(RANGE_FORM)
    })?;
    let range = range(written, styles.len()).map_err(|e| e.at(RANGE))?;
    let mut style = base;
    let mut unused = ParagraphStyle::default();
    for (name, value) in run.iter().filter(|(name, _)| *name != RANGE) {
        let key = known(name, Some(Level::Character)).map_err(|e| e.at(name.as_str()))?;
        apply(key, value, &mut style, &mut unused).map_err(|e| e.under(name.as_str()))?;
    }
    for at in range {
        styles[at] = (style.clone(), Some(index));
    }
    Ok(())
}

/// A run's range: `[start, end]` with `start < end <= characters`.
fn range(written: &Json, characters: usize) -> Result<std::ops::Range<usize>> {
    let bounds: [Number; 2] = crate::de::from_value(written).map_err(|e| e.expected(RANGE_FORM))?;
    let [start, end] = bounds.map(|Number(n)| n);
    let whole = |n: f64| n.fract() == 0.0 && n >= 0.0;
    if !(whole(start) && whole(end) && start < end) {
        return Err(Diagnostic::new(
            Kind::Invalid,
            "the range must run forward over whole indices",
        )
        .expected(RANGE_FORM));
    }
    // Both are whole and not negative, and the text is far shorter than 2^53.
    let (start, end) = (start as usize, end as usize);
    if end > characters {
        return Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("the range ends at {end}, past the text's {characters} characters"),
        )
        .expected(RANGE_FORM));
    }
    Ok(start..end)
}

/// Neighbouring characters of the same style from the same run, as runs;
/// one base run for empty text.
fn runs_of(styles: Vec<(CharacterStyle, Option<usize>)>, base: &CharacterStyle) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    for (at, (style, source)) in styles.into_iter().enumerate() {
        match runs.last_mut() {
            Some(run) if run.style == style && run.source == source => run.range.end = at + 1,
            _ => runs.push(TextRun {
                range: at..at + 1,
                style,
                source,
            }),
        }
    }
    if runs.is_empty() {
        runs.push(TextRun {
            range: 0..0,
            style: base.clone(),
            source: None,
        });
    }
    runs
}

/// Box text's `[width, height]`, both above zero.
fn frame(written: &ValueDoc) -> Result<[f64; 2]> {
    const FORM: &str = "`[width, height]` in px, both above zero, e.g. `[600, 300]`";
    match written {
        ValueDoc::Numbers(n) if n.len() == 2 && n.iter().all(|v| v.is_finite() && *v > 0.0) => {
            Ok([n[0], n[1]])
        }
        _ => Err(
            Diagnostic::new(Kind::Invalid, "a box is a width and a height above zero")
                .expected(FORM),
        ),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/validate/text.rs"]
mod tests;
