//! Reading an `.aep` project, or the project inside a `.mogrt` template
//! (steps `read` and `decode`), finding its
//! footage files (step `footage`), importing it as a scene (step `map`), and
//! setting the text of fonts the import was not given in the closest faces
//! it has (step `substitute`, [D40]): the entry points for tools that
//! import one.
//!
//! [D40]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d40-missing-fonts-are-substituted-at-import

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use motion_format::{Diagnostic, Result};
use motion_import::map::{self, Import, Options};
use motion_import::{Project, template};
use motion_text::FontSet;

use crate::fonts::has_face;
use crate::footage;
use crate::stage::{stage, stage_ok};

/// Reads the `.aep` project at `path`, or the project inside the `.mogrt`
/// template at `path`, into its typed model (steps `read` and `decode`).
///
/// # Errors
/// A [`Kind::Io`](motion_format::Kind::Io) diagnostic when the file cannot be
/// read, the template's diagnostic when it holds no project to read, or the
/// decoder's diagnostic, with its chunk path, when the file is not a project
/// it can follow.
pub fn import(path: impl AsRef<Path>) -> Result<Project> {
    read(path.as_ref()).map(|(project, _)| project)
}

/// Whether `path` names a `.mogrt` template.
#[must_use]
pub fn is_template(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mogrt"))
}

/// The project at `path`, with the panel of the template it came from: none
/// for an `.aep` project.
fn read(path: &Path) -> Result<(Project, Vec<template::Control>)> {
    let bytes = stage("read", || {
        fs::read(path).map_err(|e| Diagnostic::io("read", path, &e))
    })?;
    stage("decode", || match is_template(path) {
        true => {
            let template = template::open(&bytes)?;
            Ok((motion_import::decode(&template.project)?, template.panel))
        }
        false => Ok((motion_import::decode(&bytes)?, Vec::new())),
    })
}

/// Reads the `.aep` project, or `.mogrt` template, at `path` and imports
/// it as a scene (steps `read`, `decode`, `footage` and `map`), with what
/// the import mapped, defaulted, ignored or could not support. A
/// template's panel becomes the scene's slots, unless `options` gives one.
///
/// Footage files are looked for where the project stores them, then by name
/// under the project's folder, and named relative to the folder of `scene`,
/// the file the scene will be saved to, or absolute without one. Still
/// pictures found become image layers, and clips found in a codec with a
/// decoder video layers. Entries of `options.footage` and `options.videos`
/// win over what is found.
///
/// # Errors
/// The diagnostics of [`import`], or a diagnostic when the composition to
/// import cannot be chosen.
pub fn import_scene(
    path: impl AsRef<Path>,
    options: &Options,
    scene: Option<&Path>,
) -> Result<Import> {
    let path = path.as_ref();
    let (project, panel) = read(path)?;
    let found = stage_ok("footage", || footage::find(&project, path, scene));
    let options = Options {
        footage: found
            .files
            .into_iter()
            .chain(options.footage.clone())
            .collect(),
        videos: found
            .videos
            .into_iter()
            .chain(options.videos.clone())
            .collect(),
        sounds: found
            .sounds
            .into_iter()
            .chain(options.sounds.clone())
            .collect(),
        // A template's panel becomes the scene's slots (D53).
        panel: match options.panel.is_empty() {
            true => panel,
            false => options.panel.clone(),
        },
        ..options.clone()
    };
    stage("map", || map::import(&project, &options))
}

/// Gives every font of `import`'s text that `fonts` has no face for by
/// that name a face (step `substitute`).
///
/// - When a face [answers to](motion_text::Face::answers_to) the name, by
///   the names its font gives it ([D61]), the scene names that face by its
///   own PostScript name. It is the same face, so it is listed under
///   [`Mapping::renamed_fonts`](motion_import::map::Mapping::renamed_fonts)
///   and not counted as unsupported.
/// - Otherwise the face [`FontSet::fallback`] chooses stands in for it: one
///   of the same family, else the built-in Inter, at the same slant and the
///   nearest weight ([D40]). Each use is in the import's report.
///
/// `fonts` are the fonts the scene will have: the built-in ones, those the
/// import was given, and any fetched for it.
///
/// # Errors
/// An invalid diagnostic, as a bug in the import, when the scene with the
/// new faces is not a valid scene.
///
/// [D40]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d40-missing-fonts-are-substituted-at-import
/// [D61]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d61-a-font-is-found-by-the-names-its-face-gives-itself
pub fn substitute_fonts(import: &mut Import, fonts: &FontSet) -> Result<()> {
    stage("substitute", || {
        let mut renames = BTreeMap::new();
        let mut substitutes = BTreeMap::new();
        for name in import.fonts.keys().filter(|name| !has_face(fonts, name)) {
            if let Some(face) = fonts.named(name) {
                renames.insert(name.clone(), face.postscript_name.clone());
            } else if let Some(face) = fonts.fallback(name) {
                substitutes.insert(name.clone(), face.postscript_name.clone());
            }
        }
        import.rename_fonts(&renames)?;
        import.substitute_fonts(&substitutes)
    })
}

#[cfg(test)]
#[path = "../tests/unit/import.rs"]
mod tests;
