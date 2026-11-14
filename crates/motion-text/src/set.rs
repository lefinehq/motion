//! [`FontSet`]: the fonts a render may use, from files the caller names and
//! the built-in Inter. Never the system's fonts and never the network, so a
//! render depends only on its inputs (D12, D14).

use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use motion_format::{Diagnostic, Result};

use crate::font::{Bytes, Face, Slant, faces};

/// The file extensions read as fonts when a folder is added.
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

/// The fonts that ship with motion, as `(name, bytes)`; both are Inter's
/// variable fonts, which hold every weight from 100 to 900.
pub(crate) const BUILTIN: &[(&str, &[u8])] = &[
    (
        "InterVariable.ttf",
        include_bytes!("../fonts/InterVariable.ttf"),
    ),
    (
        "InterVariable-Italic.ttf",
        include_bytes!("../fonts/InterVariable-Italic.ttf"),
    ),
];

/// How a text run asks for its font.
#[derive(Debug, Clone, PartialEq)]
pub enum FontQuery {
    /// The face with this PostScript name, such as `Lato-Black`.
    PostScript(String),
    /// The face of `family` with exactly this weight and slant.
    Family {
        /// The family, such as `Inter Variable`.
        family: String,
        /// The weight, 1 to 1000.
        weight: f32,
        /// The slant.
        slant: Slant,
    },
}

impl fmt::Display for FontQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontQuery::PostScript(name) => f.write_str(name),
            FontQuery::Family {
                family,
                weight,
                slant,
            } => write!(f, "{family} {weight} {slant:?}"),
        }
    }
}

/// The faces a render may use, in the order they were added.
#[derive(Debug, Clone, Default)]
pub struct FontSet {
    faces: Vec<Face>,
}

impl FontSet {
    /// A set without any font.
    pub fn new() -> FontSet {
        FontSet::default()
    }

    /// The fonts that ship with motion: Inter, upright and italic, every
    /// named weight. The family is `Inter Variable`; the PostScript names
    /// run from `InterVariable-Thin` through `InterVariable` (400) to
    /// `InterVariable-Black`, and `InterVariableItalic-…` for the italics.
    pub fn builtin() -> FontSet {
        static SET: OnceLock<FontSet> = OnceLock::new();
        SET.get_or_init(|| {
            let mut set = FontSet::new();
            for (name, bytes) in BUILTIN {
                let bytes: Bytes = Arc::new(*bytes);
                set.add(bytes, name)
                    .expect("the built-in fonts are valid fonts");
            }
            set
        })
        .clone()
    }

    /// Adds every face in `bytes`, a font or a collection read from
    /// `source` (a name for messages), and returns how many it added.
    ///
    /// # Errors
    /// An invalid [`Diagnostic`] when the bytes are not a font.
    pub fn add_bytes(&mut self, bytes: Vec<u8>, source: &str) -> Result<usize> {
        self.add(Arc::new(bytes), source)
    }

    /// Adds every face in the font file at `path`.
    ///
    /// # Errors
    /// An io [`Diagnostic`] when the file cannot be read, an invalid one when
    /// it is not a font.
    pub fn add_file(&mut self, path: &Path) -> Result<usize> {
        let bytes = std::fs::read(path).map_err(|e| Diagnostic::io("read", path, &e))?;
        self.add_bytes(bytes, &path.display().to_string())
    }

    /// Adds every font file in `folder` and the folders below it, in path
    /// order so the result never depends on the file system's listing order.
    /// Files without a font extension are passed over.
    ///
    /// # Errors
    /// An io [`Diagnostic`] for a folder or file that cannot be read, an
    /// invalid one for a font file that is not a font.
    pub fn add_folder(&mut self, folder: &Path) -> Result<usize> {
        let mut files = Vec::new();
        collect_fonts(folder, &mut files)?;
        files.sort();
        files
            .iter()
            .map(|file| self.add_file(file))
            .sum::<Result<usize>>()
    }

    /// Every face, in the order added.
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// The face `query` asks for: by PostScript name the first face added
    /// under it, by family the face with exactly that weight and slant.
    ///
    /// # Errors
    /// An unknown-name [`Diagnostic`] listing the faces there are; a missing
    /// font is never replaced by another.
    pub fn find(&self, query: &FontQuery) -> Result<&Face> {
        self.position(query).map(|i| &self.faces[i])
    }

    /// The index in [`FontSet::faces`] of the face `query` asks for.
    ///
    /// # Errors
    /// As [`FontSet::find`].
    pub fn position(&self, query: &FontQuery) -> Result<usize> {
        let found = match query {
            FontQuery::PostScript(name) => {
                self.faces.iter().position(|f| &f.postscript_name == name)
            }
            FontQuery::Family {
                family,
                weight,
                slant,
            } => self.faces.iter().position(|f| {
                f.family.eq_ignore_ascii_case(family) && f.weight == *weight && f.slant == *slant
            }),
        };
        found.ok_or_else(|| {
            let names = self.faces.iter().map(|f| f.postscript_name.as_str());
            Diagnostic::unknown("font", query.to_string(), names)
        })
    }

    fn add(&mut self, bytes: Bytes, source: &str) -> Result<usize> {
        let added = faces(bytes, source)?;
        let count = added.len();
        self.faces.extend(added);
        Ok(count)
    }
}

/// Every file below `folder` whose extension is in [`FONT_EXTENSIONS`].
fn collect_fonts(folder: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(folder).map_err(|e| Diagnostic::io("read", folder, &e))?;
    for entry in entries {
        let path = entry
            .map_err(|e| Diagnostic::io("read", folder, &e))?
            .path();
        if path.is_dir() {
            collect_fonts(&path, files)?;
        } else if is_font_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

/// Whether `path` has a font extension, in any case.
fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| FONT_EXTENSIONS.iter().any(|f| e.eq_ignore_ascii_case(f)))
}

#[cfg(test)]
#[path = "../tests/unit/set.rs"]
mod tests;
