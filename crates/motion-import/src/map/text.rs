//! A text layer: its document becomes `text`, `font`, `fill`, `style`,
//! `runs`, `paragraphs` and `box`, and its animators the `animate` list. The
//! first character run sets the base style; later runs write only what
//! differs from it.

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
        let document = match &property.value {
            Stream::Static(Value::Text(document)) => Some(document),
            Stream::Animated(keys) => {
                if keys.len() > 1 {
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
        if property.expression.as_ref().is_some_and(|e| e.enabled) {
            self.mapping
                .unsupported(&place.at, "expressions (the keys under them are kept)");
        }
        let Some(document) = document else {
            self.mapping.count(DOCUMENT, Fate::Unsupported);
            return self
                .mapping
                .unsupported(&place.at, "a text document it could not read");
        };
        self.mapping.count(DOCUMENT, Fate::Mapped);
        self.write_document(document, out, place);
    }

    /// Writes `document` into `out`.
    fn write_document(&mut self, document: &TextDocument, out: &mut Out, place: &Place<'_>) {
        out.keys.insert("text".into(), json!(document.text));
        let Some(base) = document.characters.first().map(|run| &run.style) else {
            return;
        };
        out.keys.insert("font".into(), json!(font(base)));
        out.keys.insert("fill".into(), fill(base));
        let base_keys = character(base);
        let mut style: Map<String, Json> = base_keys
            .iter()
            .filter(|(name, value)| character_default(name).as_ref() != Some(value))
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

/// The value the scene gives a character key the style leaves out; `None`
/// for a key without one.
fn character_default(name: &str) -> Option<Json> {
    Some(match name {
        "stroke-width" | "tracking" | "baseline" => json!(0),
        "paint" => json!("fill-over"),
        "leading" => json!("auto"),
        "caps" => json!("normal"),
        "faux-bold" | "faux-italic" => json!(false),
        _ => return None,
    })
}

/// Every character key of `style` but its font and fill, with its value.
fn character(style: &CharacterStyle) -> Vec<(&'static str, Json)> {
    let mut keys = Vec::new();
    match &style.stroke {
        Some(stroke) => {
            keys.push(("stroke", color_json(stroke)));
            keys.push(("stroke-width", num(style.stroke_width)));
        }
        None => keys.push(("stroke-width", json!(0))),
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
    let mut keys = Map::new();
    if align != "left" {
        keys.insert("align".into(), json!(align));
    }
    [
        ("indent-first", style.first_line_indent),
        ("indent-start", style.start_indent),
        ("indent-end", style.end_indent),
        ("space-before", style.space_before),
        ("space-after", style.space_after),
    ]
    .into_iter()
    .filter(|(_, value)| *value != 0.0)
    .for_each(|(name, value)| {
        keys.insert(name.into(), num(value));
    });
    keys
}
