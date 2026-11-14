//! Reading an `.aep` project (steps `read` and `decode`), finding its
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
use motion_import::Project;
use motion_import::map::{self, Import, Options};
use motion_text::FontSet;

use crate::fonts::has_face;
use crate::footage;
use crate::stage::{stage, stage_ok};

/// Reads the `.aep` project at `path` into its typed model (steps `read`
/// and `decode`).
///
/// # Errors
/// A [`Kind::Io`](motion_format::Kind::Io) diagnostic when the file cannot be
/// read, or the decoder's diagnostic, with its chunk path, when the file is
/// not a project it can follow.
pub fn import(path: impl AsRef<Path>) -> Result<Project> {
    let path = path.as_ref();
    let bytes = stage("read", || {
        fs::read(path).map_err(|e| Diagnostic::io("read", path, &e))
    })?;
    stage("decode", || motion_import::decode(&bytes))
}

/// Reads the `.aep` project at `path` and imports it as a scene (steps
/// `read`, `decode`, `footage` and `map`), with what the import mapped,
/// defaulted, ignored or could not support.
///
/// Footage files are looked for where the project stores them, then by name
/// under the project's folder, and named relative to the folder of `scene`,
/// the file the scene will be saved to, or absolute without one. Still
/// pictures found become image layers. Entries of `options.footage` win
/// over what is found.
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
    let project = import(path)?;
    let found = stage_ok("footage", || footage::find(&project, path, scene));
    let options = Options {
        footage: found.into_iter().chain(options.footage.clone()).collect(),
        ..options.clone()
    };
    stage("map", || map::import(&project, &options))
}

/// Sets every font of `import`'s text that `fonts` has no face for in the
/// face [`FontSet::fallback`] chooses (step `substitute`): one of the same
/// family, else the built-in Inter, at the same slant and the nearest
/// weight. Each use is in the import's report. `fonts` are the fonts the
/// scene will have: the built-in ones, those the import was given, and any
/// fetched for it.
///
/// # Errors
/// An invalid diagnostic, as a bug in the import, when the scene with the
/// new faces is not a valid scene.
pub fn substitute_fonts(import: &mut Import, fonts: &FontSet) -> Result<()> {
    stage("substitute", || {
        let substitutes: BTreeMap<String, String> = import
            .fonts
            .keys()
            .filter(|name| !has_face(fonts, name))
            .filter_map(|name| Some((name.clone(), fonts.fallback(name)?.postscript_name.clone())))
            .collect();
        import.substitute_fonts(&substitutes)
    })
}

#[cfg(test)]
#[path = "../tests/unit/import.rs"]
mod tests;
