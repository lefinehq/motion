//! Fonts the import finds under another name or was not given.
//!
//! - A font the project names by another name of a face the import has,
//!   such as an earlier build's PostScript name ([D61]), is renamed to the
//!   face's own name. It is the same face, so it is listed, not counted.
//! - A font the import was not given ([D40]) is set in the face chosen to
//!   stand in for it, and every use is reported with the fonts it swaps, so
//!   the gap stays counted.
//!
//! [D40]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d40-missing-fonts-are-substituted-at-import
//! [D61]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d61-a-font-is-found-by-the-names-its-face-gives-itself

use std::collections::BTreeMap;

use motion_format::{Result, key};
use serde_json::Value as Json;

use super::{Import, Mapping, read_back};

impl Import {
    /// Names the faces the project's fonts are by their own names: each
    /// font named in `renames`, by the name the project stored, becomes the
    /// PostScript name of the face it is, the same face under the name its
    /// font gives it. Each is listed in [`Mapping::renamed_fonts`], and not
    /// as unsupported: nothing is lost.
    ///
    /// # Errors
    /// An invalid diagnostic, as a bug in the import, when the scene with
    /// the new names is not a valid scene.
    pub fn rename_fonts(&mut self, renames: &BTreeMap<String, String>) -> Result<()> {
        self.swap_fonts(renames, |mapping, _, from, to| {
            mapping.renamed_fonts.insert(from.to_owned(), to.to_owned());
        })
    }

    /// Sets the scene's text in other faces: each font named in
    /// `substitutes`, by PostScript name, becomes the one it maps to. Every
    /// place it was used is reported as unsupported, as
    /// ``missing fonts, substituted (`<font> → <face>`)``, and
    /// [`Import::fonts`] lists the places under the new face.
    ///
    /// # Errors
    /// An invalid diagnostic, as a bug in the import, when the scene with
    /// the new faces is not a valid scene.
    pub fn substitute_fonts(&mut self, substitutes: &BTreeMap<String, String>) -> Result<()> {
        self.swap_fonts(substitutes, |mapping, at, from, to| {
            mapping.unsupported(at, format!("missing fonts, substituted (`{from} → {to}`)"));
        })
    }

    /// Writes each font of `swaps` the scene uses as the one it maps to,
    /// moves its places in [`Import::fonts`] to that one, and tells `report`
    /// of each place, with the two names.
    fn swap_fonts(
        &mut self,
        swaps: &BTreeMap<String, String>,
        mut report: impl FnMut(&mut Mapping, &str, &str, &str),
    ) -> Result<()> {
        let swapped: BTreeMap<&str, &str> = swaps
            .iter()
            .filter(|(from, to)| from != to && self.fonts.contains_key(*from))
            .map(|(from, to)| (from.as_str(), to.as_str()))
            .collect();
        if swapped.is_empty() {
            return Ok(());
        }
        for (from, to) in &swapped {
            let places = self.fonts.remove(*from).unwrap_or_default();
            let uses = self.fonts.entry((*to).to_owned()).or_default();
            for at in places {
                report(&mut self.mapping, &at, from, to);
                if !uses.contains(&at) {
                    uses.push(at);
                }
            }
        }
        let scene = self.json.as_object_mut().into_iter().flatten();
        let lists = scene.flat_map(|(name, value)| match name.as_str() {
            key::LAYERS => vec![value],
            key::COMPS => value
                .as_object_mut()
                .into_iter()
                .flat_map(|comps| comps.values_mut())
                .filter_map(|comp| comp.get_mut(key::LAYERS))
                .collect(),
            _ => Vec::new(),
        });
        lists
            .filter_map(Json::as_array_mut)
            .flatten()
            .for_each(|layer| rename_in_layer(layer, &swapped));
        self.document = read_back(&self.json)?;
        Ok(())
    }
}

/// Renames the fonts of one layer: its `font` and those of its `runs`.
fn rename_in_layer(layer: &mut Json, swapped: &BTreeMap<&str, &str>) {
    let keys = layer.as_object_mut().into_iter().flatten();
    let fonts = keys.flat_map(|(name, value)| match name.as_str() {
        key::FONT => vec![value],
        key::RUNS => value
            .as_array_mut()
            .into_iter()
            .flatten()
            .filter_map(|run| run.get_mut(key::FONT))
            .collect(),
        _ => Vec::new(),
    });
    for font in fonts {
        let Json::String(text) = font else { continue };
        // The import writes a font as `<PostScript name> <size>`.
        if let Some((name, size)) = text.split_once(' ')
            && let Some(to) = swapped.get(name)
        {
            *text = format!("{to} {size}");
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/map/fonts.rs"]
mod tests;
