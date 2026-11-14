//! A text layer: its document becomes `text`, `font`, `fill`, `style`,
//! `runs`, `paragraphs` and `box`, and its animators the `animate` list. The
//! first character run sets the base style; later runs write only what
//! differs from it.

use motion_format::text::style_key;
use serde_json::{Map, Value as Json, json};

use super::Walk;
use super::items::{Out, Place};
use super::keys::{color_json, num};
use super::report::Fate;
use crate::model::{
    BaselineOption, Caps, CharacterStyle, Child, Group, Justification, Kerning, LeadingType,
    ParagraphStyle, Property, Stream, TextDocument, TextFrame, Value,
};

/// The match names of a text layer's document and its animators.
const DOCUMENT: &str = "ADBE Text Document";
const ANIMATORS: &str = "ADBE Text Animators";

impl Walk<'_> {
    /// The text in `group`, a layer's `ADBE Text Properties`, into `out`.
    pub(super) fn text(&mut self, group: &Group, out: &mut Out, place: &Place<'_>) {
        self.mapping.count(&group.match_name, Fate::Mapped);
        group.children.iter().for_each(|child| match child {
            Child::Property(property) if property.match_name == DOCUMENT => {
                self.document(property, out, &place.inside(DOCUMENT));
            }
            Child::Property(property) => {
                self.unknown_property(property, &place.inside(&property.match_name));
            }
            Child::Group(animators) if animators.match_name == ANIMATORS => {
                self.mapping.count(ANIMATORS, Fate::Mapped);
                let list = self.items(animators, &place.inside(ANIMATORS));
                if !list.is_empty() {
                    out.keys.insert("animate".into(), Json::Array(list));
                }
            }
            Child::Group(other) => self.group(other, &place.inside(&other.match_name)),
        });
    }

    /// The document of a Source Text property.
    fn document(&mut self, property: &Property, out: &mut Out, place: &Place<'_>) {
        let baked = self.evaluated(property, place);
        let document = match &property.value {
            Stream::Static(Value::Text(document)) => Some(document),
            Stream::Animated(keys) => {
                if keys.len() > 1 && baked.is_none() {
                    self.mapping
                        .unsupported(&place.at, "keyframed source text (its first text is kept)");
                }
                keys.first().and_then(|key| match &key.value {
                    Value::Text(document) => Some(document),
                    _ => None,
                })
            }
            Stream::Static(_) => None,
        };
        let Some(document) = document else {
            self.mapping.count(DOCUMENT, Fate::Unsupported);
            return self
                .mapping
                .unsupported(&place.at, "a text document it could not read");
        };
        self.mapping.count(DOCUMENT, Fate::Mapped);
        let set = baked.and_then(|bake| self.baked_document(bake, document, property, place));
        self.write_document(set.as_ref().unwrap_or(document), out, place);
    }

    /// Writes `document` into `out`.
    fn write_document(&mut self, document: &TextDocument, out: &mut Out, place: &Place<'_>) {
        out.keys.insert("text".into(), json!(document.text));
        out.lines = Some(document.lines.clone());
        let Some(base) = document.characters.first().map(|run| &run.style) else {
            return;
        };
        out.keys.insert("font".into(), json!(font(base)));
        self.uses_font(&base.font, &place.at);
        out.keys.insert("fill".into(), fill(base));
        let base_keys = character(base);
        let mut uses = Vec::new();
        let mut style: Map<String, Json> = base_keys
            .iter()
            .filter(|(name, value)| !is_default(name, value))
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect();
        let runs: Vec<Json> = document
            .characters
            .iter()
            .skip(1)
            .filter_map(|run| {
                let mut differs: Map<String, Json> = character(&run.style)
                    .into_iter()
                    .filter(|(name, value)| base_keys.iter().all(|(n, v)| n != name || v != value))
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect();
                if run.style.font != base.font || run.style.size != base.size {
                    differs.insert("font".into(), json!(font(&run.style)));
                    uses.push(&run.style.font);
                }
                if fill(&run.style) != fill(base) {
                    differs.insert("fill".into(), fill(&run.style));
                }
                if differs.is_empty() {
                    return None;
                }
                let chars =
                    |byte: usize| document.text.get(..byte).map_or(0, |s| s.chars().count());
                differs.insert(
                    "range".into(),
                    json!([chars(run.range.start), chars(run.range.end)]),
                );
                Some(Json::Object(differs))
            })
            .collect();
        uses.into_iter()
            .for_each(|font| self.uses_font(font, &place.at));
        if !runs.is_empty() {
            out.keys.insert("runs".into(), Json::Array(runs));
        }
        let paragraphs: Vec<Map<String, Json>> = document
            .paragraphs
            .iter()
            .map(|run| paragraph(&run.style))
            .collect();
        match paragraphs.split_first() {
            Some((first, rest)) if rest.iter().all(|p| p == first) => style.extend(first.clone()),
            Some(_) => {
                out.keys.insert(
                    "paragraphs".into(),
                    json!(paragraphs.into_iter().map(Json::Object).collect::<Vec<_>>()),
                );
            }
            None => {}
        }
        if !style.is_empty() {
            out.keys.insert("style".into(), Json::Object(style));
        }
        if let TextFrame::Box {
            left,
            top,
            width,
            height,
        } = document.frame
        {
            let frame = match left == 0.0 && top == 0.0 {
                true => json!([num(width), num(height)]),
                false => json!([num(left), num(top), num(width), num(height)]),
            };
            out.keys.insert("box".into(), frame);
        }
        self.unsupported_style(document, place);
    }

    /// Reports the character and paragraph features the scene has no key for.
    fn unsupported_style(&mut self, document: &TextDocument, place: &Place<'_>) {
        let mut found: Vec<&str> = Vec::new();
        document
            .characters
            .iter()
            .map(|run| &run.style)
            .for_each(|style| {
                if style.horizontal_scale != 1.0 || style.vertical_scale != 1.0 {
                    found.push("scaled characters");
                }
                if style.baseline_option != BaselineOption::Normal {
                    found.push("superscript and subscript");
                }
                if style.tsume != 0.0 {
                    found.push("tsume");
                }
                if style.kerning != Kerning::Metrics {
                    found.push("kerning other than the font's");
                }
            });
        document.paragraphs.iter().for_each(|run| {
            if run.style.leading_type != LeadingType::Roman {
                found.push("leading measured top to top");
            }
        });
        found.sort_unstable();
        found.dedup();
        found
            .into_iter()
            .for_each(|what| self.mapping.unsupported(&place.at, what));
    }
}

impl Walk<'_> {
    /// Notes that the text at `at` is set in `font`, a PostScript name.
    fn uses_font(&mut self, font: &str, at: &str) {
        let places = self.fonts.entry(font.to_owned()).or_default();
        if !places.iter().any(|place| place == at) {
            places.push(at.to_owned());
        }
    }
}

/// The base font of a run: its PostScript name and size.
fn font(style: &CharacterStyle) -> String {
    format!("{} {}", style.font, num(style.size))
}

/// A run's fill, or a clear colour when it has none.
fn fill(style: &CharacterStyle) -> Json {
    style
        .fill
        .as_ref()
        .map_or_else(|| json!([0, 0, 0, 0]), color_json)
}

/// Whether the scene's style leaves `name` at `value` when it is not
/// written, by the format's own defaults.
fn is_default(name: &str, value: &Json) -> bool {
    style_key(name).is_some_and(|key| key.is_default(value))
}

/// Every character key of `style` but its font and fill, with its value.
fn character(style: &CharacterStyle) -> Vec<(&'static str, Json)> {
    let mut keys = Vec::new();
    // No stroke is a clear one, which draws nothing whatever its width.
    match &style.stroke {
        Some(stroke) => {
            keys.push(("stroke", color_json(stroke)));
            keys.push(("stroke-width", num(style.stroke_width)));
        }
        None => keys.push(("stroke", json!([0, 0, 0, 0]))),
    }
    let paint = if style.stroke_over_fill {
        "stroke-over"
    } else {
        "fill-over"
    };
    keys.push(("paint", json!(paint)));
    keys.push(("tracking", num(style.tracking)));
    let leading = if style.auto_leading {
        json!("auto")
    } else {
        num(style.leading)
    };
    keys.push(("leading", leading));
    keys.push(("baseline", num(style.baseline_shift)));
    let caps = match style.caps {
        Caps::Small => "small",
        Caps::All => "all",
        Caps::AllSmall => "all-small",
        Caps::Normal | Caps::Other(_) => "normal",
    };
    keys.push(("caps", json!(caps)));
    keys.push(("faux-bold", json!(style.faux_bold)));
    keys.push(("faux-italic", json!(style.faux_italic)));
    keys
}

/// The paragraph keys of `style` that differ from the scene's defaults.
fn paragraph(style: &ParagraphStyle) -> Map<String, Json> {
    let align = match style.justification {
        Justification::Right => "right",
        Justification::Center => "center",
        Justification::JustifyLastLeft => "justify-left",
        Justification::JustifyLastRight => "justify-right",
        Justification::JustifyLastCenter => "justify-center",
        Justification::JustifyAll => "justify-all",
        Justification::Left | Justification::Other(_) => "left",
    };
    [
        ("align", json!(align)),
        ("indent-first", num(style.first_line_indent)),
        ("indent-start", num(style.start_indent)),
        ("indent-end", num(style.end_indent)),
        ("space-before", num(style.space_before)),
        ("space-after", num(style.space_after)),
    ]
    .into_iter()
    .filter(|(name, value)| !is_default(name, value))
    .map(|(name, value)| (name.to_owned(), value))
    .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/map/text.rs"]
mod tests;
