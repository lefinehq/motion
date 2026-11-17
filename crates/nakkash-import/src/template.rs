//! `.mogrt` templates ([D53]): a zip archive of the template's panel,
//! `definition.json`, a thumbnail, and a nested archive that holds the
//! project. [`open`] reads the panel and the project's bytes, and nothing
//! else; the project then decodes as any other ([`decode`](crate::decode)).
//!
//! [D53]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d53-a-mogrt-imports-as-its-project-with-its-controls-as-slots

use std::io::{Cursor, Read, Seek};

use nakkash_format::{Diagnostic, Kind, Result};
use serde::Deserialize;
use serde_json::Value as Json;
use zip::ZipArchive;

use crate::model::ControlKind;

/// The most bytes one entry may inflate to, checked against the size the
/// archive claims and against what it gives, so a crafted archive cannot
/// fill memory.
pub const MAX_ENTRY: u64 = 1 << 30;

/// The panel's file.
const DEFINITION: &str = "definition.json";
/// The ending of the nested archive that holds a project.
const PROJECT_ARCHIVE: &str = ".aegraphic";
/// The ending of the nested archive of a template made in an editing app.
const EDITOR_ARCHIVE: &str = ".prgraphic";
/// The ending of the project inside the nested archive.
const PROJECT: &str = ".aep";

/// A template: its panel and the project inside it.
#[derive(Debug, Clone)]
pub struct Template {
    /// The panel's controls, in its order.
    pub panel: Vec<Control>,
    /// The project's bytes.
    pub project: Vec<u8>,
}

/// One control of the panel, as `definition.json` gives it. The project
/// gives what it drives ([`PanelControl`](crate::PanelControl)), by the
/// same id.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    /// Its id, a UUID.
    pub id: String,
    /// What kind of value it offers.
    pub kind: ControlKind,
    /// Its name, in the first language the template gives.
    pub name: String,
    /// Its value as the template writes it: a group's is the ids of its
    /// controls.
    pub value: Json,
    /// The smallest value a slider takes.
    pub min: Option<f64>,
    /// The largest value a slider takes.
    pub max: Option<f64>,
}

/// `definition.json`, the fields the import reads.
#[derive(Deserialize)]
struct Definition {
    #[serde(rename = "clientControls")]
    controls: Vec<RawControl>,
}

/// One entry of `clientControls`.
#[derive(Deserialize)]
struct RawControl {
    id: String,
    #[serde(rename = "type")]
    kind: u32,
    #[serde(rename = "uiName")]
    name: Localized,
    #[serde(default)]
    value: Json,
    min: Option<f64>,
    max: Option<f64>,
}

/// A string in each language the template gives.
#[derive(Deserialize)]
struct Localized {
    #[serde(rename = "strDB")]
    strings: Vec<LocalString>,
}

#[derive(Deserialize)]
struct LocalString {
    #[serde(rename = "str")]
    text: String,
}

/// Reads a `.mogrt` archive into its panel and its project's bytes.
///
/// # Errors
/// A diagnostic when the bytes are not a zip archive, it has no
/// `definition.json` or that file does not read, an entry inflates past
/// [`MAX_ENTRY`] or fails its checksum, or the template holds no project.
pub fn open(archive: &[u8]) -> Result<Template> {
    let mut outer =
        ZipArchive::new(Cursor::new(archive)).map_err(|e| not_an_archive("the file", &e))?;
    let definition = entry(&mut outer, DEFINITION, MAX_ENTRY)?.ok_or_else(|| {
        Diagnostic::new(Kind::Missing, "the template has no `definition.json`")
            .expected("a `.mogrt` archive with its panel in `definition.json`")
    })?;
    let panel = panel(&definition)?;
    let entries = names(&outer)?;
    let Some(nested) = entries.iter().find(|n| ends_with(n, PROJECT_ARCHIVE)) else {
        let made_in_editor = entries.iter().any(|n| ends_with(n, EDITOR_ARCHIVE));
        return Err(no_project(match made_in_editor {
            true => "the template holds no project to read: it was made in an editing app",
            false => "the template holds no project to read",
        }));
    };
    let nested = entry(&mut outer, nested, MAX_ENTRY)?.unwrap_or_default();
    let mut inner = ZipArchive::new(Cursor::new(nested))
        .map_err(|e| not_an_archive("its project archive", &e))?;
    let Some(name) = names(&inner)?.into_iter().find(|n| ends_with(n, PROJECT)) else {
        return Err(no_project(
            "the template's project archive holds no project",
        ));
    };
    let project = entry(&mut inner, &name, MAX_ENTRY)?.unwrap_or_default();
    Ok(Template { panel, project })
}

/// The panel of `definition.json`.
fn panel(definition: &[u8]) -> Result<Vec<Control>> {
    let mut reader = serde_json::Deserializer::from_slice(definition);
    let definition: Definition = serde_path_to_error::deserialize(&mut reader).map_err(|e| {
        Diagnostic::new(
            Kind::Invalid,
            format!(
                "the template's `definition.json` does not read at `{}`: {}",
                e.path(),
                e.inner()
            ),
        )
    })?;
    Ok(definition
        .controls
        .into_iter()
        .map(|c| Control {
            id: c.id,
            kind: ControlKind::from_code(c.kind),
            name: c
                .name
                .strings
                .into_iter()
                .next()
                .map(|s| s.text)
                .unwrap_or_default(),
            value: c.value,
            min: c.min,
            max: c.max,
        })
        .collect())
}

/// The bytes of the entry called `name`, at most `limit` of them, or
/// `None` when there is none.
fn entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(unreadable(name, &e)),
    };
    let too_large = || {
        Diagnostic::new(
            Kind::Invalid,
            format!("`{name}` in the template inflates past {limit} bytes"),
        )
        .expected("a smaller template")
    };
    if file.size() > limit {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| unreadable(name, &e))?;
    match u64::try_from(bytes.len()) {
        Ok(len) if len <= limit => Ok(Some(bytes)),
        _ => Err(too_large()),
    }
}

/// The names of an archive's entries.
fn names<R: Read + Seek>(archive: &ZipArchive<R>) -> Result<Vec<String>> {
    archive
        .file_names()
        .map(|name| {
            name.map(|n| n.into_owned())
                .map_err(|e| unreadable("an entry's name", &e))
        })
        .collect()
}

/// Whether `name` ends with `ending`, in any case.
fn ends_with(name: &str, ending: &str) -> bool {
    name.len() >= ending.len()
        && name
            .get(name.len() - ending.len()..)
            .is_some_and(|end| end.eq_ignore_ascii_case(ending))
}

fn not_an_archive(what: &str, error: &zip::result::ZipError) -> Diagnostic {
    Diagnostic::new(
        Kind::Invalid,
        format!("{what} is not a zip archive: {error}"),
    )
    .expected("a `.mogrt` template")
}

fn unreadable(name: &str, error: &dyn std::fmt::Display) -> Diagnostic {
    Diagnostic::new(
        Kind::Invalid,
        format!("`{name}` in the template does not read: {error}"),
    )
}

fn no_project(message: &str) -> Diagnostic {
    Diagnostic::new(Kind::Missing, message)
        .expected("a template with its project inside, in an `.aegraphic` archive")
}

#[cfg(test)]
#[path = "../tests/unit/template.rs"]
mod tests;
