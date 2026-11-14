//! Fonts and the engine ([D39]): the faces a scene asks for, the entry that
//! names a lock in its `fonts`, and the [`LayoutJudge`] that holds
//! candidate font files against the text a project stored.
//!
//! [D39]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use motion_fonts::{FontFile, Judge, LINE_OFF_CAP, LinesScore, TextScore};
use motion_format::text::TextContent;
use motion_format::{Content, Layer, Result, Scene, key};
use motion_import::map::StoredText;
use motion_text::{FontSet, Typesetter};
use serde_json::Value;

use crate::geometry::{TEXT_TOLERANCE, check_text, content_of};

/// The extension that makes an entry of a scene's `fonts` a lock.
pub const LOCK_EXTENSION: &str = "lock";

/// Where `motion fonts fetch` keeps the lock of the scene at `scene`: beside
/// it, named after it (`story.json` locks in `story.fonts.lock`), so scenes
/// that share a folder never change each other's fonts.
pub fn lock_path(scene: &Path) -> PathBuf {
    let stem = scene
        .file_stem()
        .map_or_else(|| "scene".into(), |s| s.to_string_lossy());
    scene.with_file_name(format!("{stem}.fonts.{LOCK_EXTENSION}"))
}

/// Every font name the scene's text asks for, PostScript names and families
/// alike, in order and once each.
pub fn font_names(scene: &Scene) -> Vec<String> {
    let mut names = BTreeSet::new();
    let layers = scene
        .layers
        .iter()
        .chain(scene.comps.iter().flat_map(|c| &c.layers));
    for content in layers.filter_map(text_of) {
        names.extend(asks(content));
    }
    names.into_iter().collect()
}

/// The names among [`font_names`] that `fonts` has no face for, by
/// PostScript name or family: what a fetch has to find.
pub fn missing_fonts(scene: &Scene, fonts: &FontSet) -> Vec<String> {
    font_names(scene)
        .into_iter()
        .filter(|name| {
            !fonts
                .faces()
                .iter()
                .any(|face| &face.postscript_name == name || &face.family == name)
        })
        .collect()
}

fn text_of(layer: &Layer) -> Option<&TextContent> {
    match &layer.content {
        Content::Text(content) => Some(content),
        _ => None,
    }
}

/// The font names one text asks for.
fn asks(content: &TextContent) -> BTreeSet<String> {
    std::iter::once(&content.base)
        .chain(content.runs.iter().map(|run| &run.style))
        .map(|style| style.font.family.clone())
        .collect()
}

/// Adds `entry` to the `fonts` of a scene written as JSON, unless it is
/// there; a new `fonts` goes before the compositions and layers, where the
/// writer puts it.
pub fn add_fonts_entry(scene: &mut Value, entry: &str) {
    let Some(object) = scene.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(fonts)) = object.get_mut(key::FONTS) {
        if !fonts.iter().any(|f| f.as_str() == Some(entry)) {
            fonts.push(Value::from(entry));
        }
        return;
    }
    let mut rebuilt = serde_json::Map::new();
    let mut placed = false;
    for (name, value) in std::mem::take(object) {
        if !placed && (name == key::COMPS || name == key::LAYERS) {
            rebuilt.insert(key::FONTS.to_owned(), Value::from(vec![entry]));
            placed = true;
        }
        rebuilt.insert(name, value);
    }
    if !placed {
        rebuilt.insert(key::FONTS.to_owned(), Value::from(vec![entry]));
    }
    *object = rebuilt;
}

/// Holds candidate font files against the lines a project stored for its
/// text ([`Judge`]): each stored text is set alone, in the scene's style,
/// with `base` and the candidates, and held line by line and glyph by
/// glyph against the stored lines.
pub struct LayoutJudge<'a> {
    scene: &'a Scene,
    texts: &'a [StoredText],
    base: FontSet,
}

impl<'a> LayoutJudge<'a> {
    /// A judge of `texts` as `scene` styles them, with the faces of `base`
    /// (such as the built-in ones and the scene's other fonts) besides the
    /// candidates.
    pub fn new(scene: &'a Scene, texts: &'a [StoredText], base: FontSet) -> Self {
        Self { scene, texts, base }
    }
}

impl Judge for LayoutJudge<'_> {
    fn judge(&mut self, files: &[FontFile<'_>]) -> Result<Vec<TextScore>> {
        let mut fonts = self.base.clone();
        for file in files {
            fonts.add_bytes(file.bytes.to_vec(), file.name)?;
        }
        let mut typesetter = Typesetter::new(fonts);
        Ok(self
            .texts
            .iter()
            .map(|text| {
                let fonts = content_of(self.scene, text)
                    .map(|content| asks(content).into_iter().collect())
                    .unwrap_or_default();
                // A text the candidates cannot set scores as unset.
                let set = check_text(self.scene, text, &mut typesetter)
                    .ok()
                    .map(|check| {
                        let mut score = LinesScore {
                            lines: check.lines.len(),
                            ..LinesScore::default()
                        };
                        for line in &check.lines {
                            let off = line.off();
                            score.lines_failing += usize::from(off > TEXT_TOLERANCE);
                            score.off += off.min(LINE_OFF_CAP);
                            if let Some((differ, compared)) = line.glyphs_differ() {
                                score.glyphs_differ += differ;
                                score.glyphs += compared;
                            }
                        }
                        score
                    });
                TextScore { fonts, set }
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../tests/unit/fonts.rs"]
mod tests;
