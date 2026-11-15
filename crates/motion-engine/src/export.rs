//! Writing an edited scene back into the `.aep` project it was imported
//! from (steps `plan`, `patch` and `verify`, [D45]).
//!
//! [D45]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d45-export-patches-the-project-it-came-from

use std::fs;
use std::path::{Path, PathBuf};

use motion_format::{Diagnostic, Kind, Result};
use motion_import::export::{self, Plan};
use motion_import::map::Options;
use serde_json::Value as Json;

use crate::Session;
use crate::import::{import_scene, substitute_fonts};
use crate::stage::stage;

/// Writes the scene at `scene`, imported from the project at `project` and
/// since edited, back into that project, saved at `out` (which may be
/// `project`). `comp` is the composition the scene was imported from, by
/// name, when it was given then.
///
/// The original is imported again as `motion import` did, with the fonts
/// the scene lists, and compared with the scene. Each changed value the
/// project stores is patched into a copy of the project; the copy is read
/// back, imported, and saved at `out` only when every written value comes
/// back as edited.
///
/// # Errors
/// The diagnostics of reading and importing either file, or an invalid
/// diagnostic when the patched project does not read back as edited, in
/// which case nothing is saved.
pub fn export_project(
    scene: &Path,
    project: &Path,
    out: &Path,
    comp: Option<String>,
) -> Result<Plan> {
    if crate::import::is_template(project) {
        return Err(Diagnostic::new(
            Kind::Invalid,
            "a scene cannot be written back into a `.mogrt` template yet",
        )
        .expected("an `.aep` project (D53 leaves writing a template back for later)"));
    }
    let text = fs::read_to_string(scene).map_err(|e| Diagnostic::io("read", scene, &e))?;
    let edited: Json = serde_json::from_str(&text).map_err(|e| {
        Diagnostic::new(Kind::Syntax, format!("the scene is not JSON: {e}"))
            .at(scene.display().to_string())
    })?;
    let fonts = edited
        .get("fonts")
        .and_then(Json::as_array)
        .map(|fonts| {
            fonts
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let options = Options {
        comp,
        fonts,
        ..Options::default()
    };
    let mut original = import_scene(project, &options, Some(scene))?;
    substitute_fonts(&mut original, Session::open(scene)?.fonts()?)?;
    let plan = stage("plan", || {
        Ok(export::plan(&original.json, &original.sources, &edited))
    })?;
    let bytes = fs::read(project).map_err(|e| Diagnostic::io("read", project, &e))?;
    let patched = stage("patch", || export::apply(&bytes, &plan))?;
    let part = partial(out);
    fs::write(&part, &patched).map_err(|e| Diagnostic::io("write", &part, &e))?;
    let checked = stage("verify", || {
        let back = import_scene(&part, &options, Some(scene))?;
        match export::verify(&plan, &back.json).as_slice() {
            [] => Ok(()),
            wrong => Err(export::mismatch(wrong)),
        }
    });
    if let Err(error) = checked {
        let _ = fs::remove_file(&part);
        return Err(error);
    }
    fs::rename(&part, out).map_err(|e| Diagnostic::io("write", out, &e))?;
    Ok(plan)
}

/// Where the export writes before the check passes: beside `out`.
fn partial(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_owned();
    name.push(".part");
    out.with_file_name(name)
}
