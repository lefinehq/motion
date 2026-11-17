//! Reading an `.aep` project, or the project inside a `.mogrt` template
//! (steps `read` and `decode`), finding its
//! footage files (step `footage`), importing it as a scene (step `map`),
//! setting the text of fonts the import was not given in the closest faces
//! it has (step `substitute`, [D40]), and measuring the content of the
//! layers its expressions read the size of (step `layout`): the entry
//! points for tools that import one.
//!
//! [D40]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d40-missing-fonts-are-substituted-at-import

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use motion_format::{Diagnostic, Result};
use motion_import::map::{self, Import, Options, Unmeasured};
use motion_import::{Project, template};
use motion_text::{FontSet, Typesetter};

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
/// pictures and vector footage found become image layers, and clips found
/// in a codec with a decoder video layers. Entries of `options.footage`,
/// `options.videos`, `options.sounds` and `options.vectors` win over what
/// is found.
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
    let options = prepared(&project, panel, path, options, scene);
    stage("map", || map::import(&project, &options))
}

/// `options` with the footage found for `project`, read from `path`, and
/// the panel of the template it came from, as [`import_scene`] imports it.
fn prepared(
    project: &Project,
    panel: Vec<template::Control>,
    path: &Path,
    options: &Options,
    scene: Option<&Path>,
) -> Options {
    let found = stage_ok("footage", || footage::find(project, path, scene));
    Options {
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
        vectors: found
            .vectors
            .into_iter()
            .chain(options.vectors.clone())
            .collect(),
        // A template's panel becomes the scene's slots (D53).
        panel: match options.panel.is_empty() {
            true => panel,
            false => options.panel.clone(),
        },
        ..options.clone()
    }
}

/// The most times [`measure_layouts`] imports a project again.
const MAX_LAYOUT_PASSES: usize = 8;

/// Measures what the expressions of `import` read of layers' content
/// (`sourceRectAtTime`) in the scene it wrote, set in `fonts`, and imports
/// the project at `path` again with `options` and what was measured, until
/// every rectangle read is measured (step `layout`). `import` must have its
/// fonts substituted ([`substitute_fonts`]); it is replaced by the last
/// import, substituted the same way.
///
/// - **The same code that draws.** A rectangle is what the scene's own
///   layout and drawing give the layer at that time: its glyphs as its fonts
///   shape them and its animators move them, its shapes as they are drawn
///   ([`motion_scene::content_rects`]).
/// - **Order.** What a layer draws can depend on expressions that read
///   other rectangles: a box sized to a title is itself measured by the
///   next. A pass measures only layers whose content has baked
///   ([`Unmeasured::settled`]), so a rectangle is measured from what the
///   layer will draw. When none has, as when a layer's content reads its own
///   rectangle, each is measured as it draws now, with the expressions that
///   wait drawn as the keys under them: the value the original reads for a
///   property while its own expression runs.
/// - **Limits.** After 8 imports again, what is still not
///   measured stays not baked, and is reported.
///
/// # Errors
/// As [`import_scene`] and [`substitute_fonts`]. A scene that does not
/// compile, such as one with a glyph no font has, is not an error here: the
/// rectangles it would give are reported as not measured, with why.
pub fn measure_layouts(
    path: impl AsRef<Path>,
    options: &Options,
    scene: Option<&Path>,
    import: &mut Import,
    fonts: &FontSet,
) -> Result<()> {
    if import.unmeasured.is_empty() {
        return Ok(());
    }
    let path = path.as_ref();
    let (project, panel) = read(path)?;
    let mut options = prepared(&project, panel, path, options, scene);
    for _ in 0..MAX_LAYOUT_PASSES {
        let asks = to_measure(&import.unmeasured);
        if asks.is_empty() {
            break;
        }
        stage_ok("layout", || {
            measure(import, &asks, fonts, &mut options.layouts)
        });
        *import = stage("map", || map::import(&project, &options))?;
        substitute_fonts(import, fonts)?;
        if import.unmeasured.is_empty() {
            break;
        }
    }
    Ok(())
}

/// What a pass measures of `unmeasured`: the rectangles of layers whose
/// content has baked, or all of them when none has.
fn to_measure(unmeasured: &[Unmeasured]) -> Vec<&Unmeasured> {
    let settled: Vec<&Unmeasured> = unmeasured.iter().filter(|u| u.settled).collect();
    match settled.is_empty() {
        true => unmeasured.iter().collect(),
        false => settled,
    }
}

/// Measures `asks` in the scene `import` wrote, set in `fonts`, into
/// `layouts`.
fn measure(
    import: &Import,
    asks: &[&Unmeasured],
    fonts: &FontSet,
    layouts: &mut motion_import::bake::Layouts,
) {
    let mut typesetter = Typesetter::new(fonts.clone());
    let graph = match motion_scene::compile(import.document.scene(), &mut typesetter) {
        Ok(graph) => graph,
        Err(error) => {
            let why = format!("the scene does not compile: {}", error.message);
            asks.iter()
                .for_each(|u| layouts.insert(&u.ask, Err(why.clone())));
            return;
        }
    };
    let placed: Vec<_> = asks
        .iter()
        .filter_map(|u| Some((u.layer.clone()?, u.ask.time, u.ask.extents)))
        .collect();
    let mut rects = motion_scene::content_rects(&graph, &placed).into_iter();
    for u in asks {
        let rect = match u.layer {
            Some(_) => rects.next().flatten().ok_or("the scene has no such layer"),
            None => Err("the scene leaves the layer out"),
        };
        let rect = rect.map(|r| [r.x0, r.y0, r.width(), r.height()]);
        layouts.insert(&u.ask, rect.map_err(str::to_owned));
    }
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
