//! A text layer: its document becomes `text`, `font`, `fill`, `style`,
//! `runs`, `paragraphs` and `box`, and its animators the `animate` list. The
//! first character run sets the base style; later runs write only what
//! differs from it.

use nakkash_format::text::style_key;
use serde_json::{Map, Value as Json, json};

use super::Walk;
use super::expressions::BakedText;
use super::items::{Out, Place};
use super::keys::{color_json, num};
use super::report::Fate;
use super::slots::{Held, address};
use crate::bake::{Sampled, Samples};
use crate::model::{
    BaselineOption, Caps, CharacterStyle, Child, Group, Justification, Kerning, Keyframe,
    LeadingType, ParagraphStyle, Property, Stream, TextDocument, TextFrame, Value,
};

/// The match names of a text layer's document and its animators.
const DOCUMENT: &str = "ADBE Text Document";
const ANIMATORS: &str = "ADBE Text Animators";
/// The match name of a text layer's options for the whole text.
const MORE: &str = "ADBE Text More Options";

/// The codes of the project's anchor grouping, by the scene's `anchor-by`.
const ANCHORS_BY: [(i64, &str); 4] = [(1, "char"), (2, "word"), (3, "line"), (4, "all")];
/// The codes of the project's fill and stroke order across characters, by
/// the scene's `paint-across`.
const PAINTS_ACROSS: [(i64, &str); 3] = [(1, "each"), (2, "fills-over"), (3, "strokes-over")];
/// The code of blending between characters that blends nothing: each
/// character drawn over the ones before it.
const NORMAL_BLEND: i64 = 1;

impl Walk<'_> {
    /// The text in `group`, a layer's `ADBE Text Properties`, into `out`.
    pub(super) fn text(&mut self, group: &Group, out: &mut Out, place: &Place<'_>) {
        self.mapping.count(&group.match_name, Fate::Mapped);
        self.count_parts(group);
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
            Child::Group(more) if more.match_name == MORE => {
                self.whole_text(more, out, &place.inside(MORE));
            }
            Child::Group(other) => self.group(other, &place.inside(&other.match_name)),
        });
    }

    /// The options of the whole text in `group`, `ADBE Text More Options`,
    /// as `style` keys of the text level: whose anchor its characters take,
    /// how far the anchors move, and how fills and strokes stack across
    /// characters. Blending between characters is supported only where it
    /// blends nothing.
    fn whole_text(&mut self, group: &Group, out: &mut Out, place: &Place<'_>) {
        if !group.properties().any(|property| !property.hidden) {
            return self.mapping.ignore(MORE, "empty");
        }
        self.mapping.count(MORE, Fate::Mapped);
        let mut keys = Map::new();
        for child in &group.children {
            let Child::Property(property) = child else {
                if let Child::Group(other) = child {
                    self.group(other, &place.inside(&other.match_name));
                }
                continue;
            };
            if property.hidden {
                continue;
            }
            let at = place.inside(&property.match_name);
            let name = property.match_name.as_str();
            let value = self.still(property, &at);
            let code = |table: &[(i64, &'static str)]| match value {
                Some(Value::Number(n)) => table
                    .iter()
                    .find(|(c, _)| *c == n.round() as i64)
                    .map(|(_, name)| *name),
                _ => None,
            };
            let (key, json) = match name {
                "ADBE Text Anchor Point Option" => {
                    ("anchor-by", code(&ANCHORS_BY).map(|c| json!(c)))
                }
                "ADBE Text Render Order" => {
                    ("paint-across", code(&PAINTS_ACROSS).map(|c| json!(c)))
                }
                // Percent of the part's width and of its size, as fractions.
                "ADBE Text Anchor Point Align" => match value {
                    Some(Value::Vector2([x, y]) | Value::Vector3([x, y, _])) => (
                        "anchor-align",
                        Some(json!([num(x / 100.0), num(y / 100.0)])),
                    ),
                    _ => ("anchor-align", None),
                },
                "ADBE Text Character Blend Mode" => match value {
                    Some(Value::Number(n)) if n.round() as i64 == NORMAL_BLEND => {
                        self.mapping.count(name, Fate::Defaulted);
                        continue;
                    }
                    _ => {
                        self.mapping.count(name, Fate::Unsupported);
                        self.mapping
                            .unsupported(&at.at, "blending between the characters of a text");
                        continue;
                    }
                },
                _ => {
                    self.unknown_property(property, &at);
                    continue;
                }
            };
            match json {
                Some(json) => {
                    self.mapping.count(name, Fate::Mapped);
                    if !is_default(key, &json) {
                        keys.insert(key.to_owned(), json);
                    }
                }
                None => {
                    self.mapping.count(name, Fate::Unsupported);
                    self.mapping
                        .unsupported(&at.at, format!("an `{key}` the scene does not have"));
                }
            }
        }
        merge_style(out, keys);
    }

    /// The one value `property` holds for the whole text: its own, or what
    /// its expression gives when that is the same at every frame. A value
    /// that changes is reported, and its first is kept.
    fn still(&mut self, property: &Property, place: &Place<'_>) -> Option<Value> {
        if let Some(bake) = self.evaluated(property, place) {
            let first = match &bake.outcome {
                Ok(Samples {
                    values: Sampled::Numbers { channels, .. },
                    ..
                }) if channels
                    .iter()
                    .all(|c| c.iter().all(|v| c.first() == Some(v))) =>
                {
                    channels
                        .iter()
                        .map(|c| c.first().copied())
                        .collect::<Option<Vec<f64>>>()
                }
                _ => None,
            };
            return match (first.as_deref(), &property.value) {
                (Some(&[n]), _) => Some(Value::Number(n)),
                (Some(&[x, y]), _) => Some(Value::Vector2([x, y])),
                (_, stream) => {
                    self.mapping.unsupported(
                        &place.at,
                        "an expression on a property the scene does not animate (its value is kept)",
                    );
                    first_value(stream)
                }
            };
        }
        if let Stream::Animated(keys) = &property.value
            && keys.len() > 1
        {
            self.mapping
                .unsupported(&place.at, "an animated setting (its first value is kept)");
        }
        first_value(&property.value)
    }

    /// The document of a Source Text property. Words that change over
    /// time, keyed or from an expression, become held keys of the layer's
    /// words over the document they start from ([D92]).
    ///
    /// [D92]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d92-a-text-layers-words-may-be-keyed
    fn document(&mut self, property: &Property, out: &mut Out, place: &Place<'_>) {
        let baked = self.evaluated(property, place);
        let document = match &property.value {
            Stream::Static(Value::Text(document)) => Some(document),
            Stream::Animated(keys) => keys.first().and_then(|key| match &key.value {
                Value::Text(document) => Some(document),
                _ => None,
            }),
            Stream::Static(_) => None,
        };
        let Some(document) = document else {
            self.mapping.count(DOCUMENT, Fate::Unsupported);
            return self
                .mapping
                .unsupported(&place.at, "a text document it could not read");
        };
        self.mapping.count(DOCUMENT, Fate::Mapped);
        let still = baked.is_none() && matches!(property.value, Stream::Static(_));
        match baked.and_then(|bake| self.baked_document(bake, document, property, place)) {
            Some(BakedText::Still(set)) => self.write_document(&set, out, place),
            Some(BakedText::Keyed(keys)) => {
                self.write_document(document, out, place);
                out.anim.insert("text".into(), keys);
            }
            None => {
                self.write_document(document, out, place);
                if let Stream::Animated(keys) = &property.value
                    && keys.len() > 1
                {
                    self.keyed_words(keys, document, out, place);
                }
            }
        }
        // A still text a panel control drives follows its slot.
        if let Some(text) = out.keys.get_mut("text").filter(|_| still)
            && let Some(marked) = self.follow_panel(&Held::Stored(address(property)), text, None)
        {
            *text = marked;
        }
    }

    /// The keys of a keyed Source Text as held keys of the layer's words,
    /// at the times the project keeps them ([D92]). Each key's words are set
    /// in the style of the first key's first characters and paragraph; a
    /// key whose own style differs is reported, its words kept.
    ///
    /// [D92]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d92-a-text-layers-words-may-be-keyed
    fn keyed_words(
        &mut self,
        keys: &[Keyframe],
        first: &TextDocument,
        out: &mut Out,
        place: &Place<'_>,
    ) {
        let base = first.characters.first().map(|run| &run.style);
        let paragraph = first.paragraphs.first().map(|run| &run.style);
        let mut restyled = 0;
        let mut written = Vec::with_capacity(keys.len());
        for key in keys {
            let Value::Text(document) = &key.value else {
                return self
                    .mapping
                    .unsupported(&place.at, "a text document it could not read");
            };
            let same = document
                .characters
                .iter()
                .all(|run| Some(&run.style) == base)
                && document
                    .paragraphs
                    .iter()
                    .all(|run| Some(&run.style) == paragraph);
            restyled += usize::from(!same);
            written.push(json!([
                place.clock.time(key.time),
                document.text.replace('\r', "\n")
            ]));
        }
        if restyled > 0 {
            self.mapping.unsupported(
                &place.at,
                format!(
                    "keyed source text whose keys change its style: {} (their words are kept, in the first key's style)",
                    super::items::plural(restyled, "key")
                ),
            );
        }
        out.anim.insert("text".into(), Json::Array(written));
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
        merge_style(out, style);
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
                if style.baseline_option != BaselineOption::Normal {
                    found.push("superscript and subscript");
                }
                if style.tsume != 0.0 {
                    found.push("tsume");
                }
                match style.kerning {
                    Kerning::Optical => {
                        found.push("optical kerning (the font's kerning pairs are kept)");
                    }
                    Kerning::Other(_) => {
                        found.push("a kerning the scene does not have (the font's is kept)");
                    }
                    Kerning::Metrics | Kerning::None => {}
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

/// Adds `keys` to the layer's `style`, which is written only when it holds a key.
fn merge_style(out: &mut Out, keys: Map<String, Json>) {
    if keys.is_empty() {
        return;
    }
    if let Json::Object(style) = out
        .keys
        .entry("style")
        .or_insert_with(|| Json::Object(Map::new()))
    {
        style.extend(keys);
    }
}

/// The value `stream` holds, or its first key's.
fn first_value(stream: &Stream) -> Option<Value> {
    match stream {
        Stream::Static(value) => Some(value.clone()),
        Stream::Animated(keys) => keys.first().map(|key| key.value.clone()),
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
    keys.push(("horizontal-scale", num(style.horizontal_scale)));
    keys.push(("vertical-scale", num(style.vertical_scale)));
    let kerning = match style.kerning {
        Kerning::None => "none",
        Kerning::Metrics | Kerning::Optical | Kerning::Other(_) => "font",
    };
    keys.push(("kerning", json!(kerning)));
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
