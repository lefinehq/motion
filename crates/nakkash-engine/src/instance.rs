//! Opening an instance ([D91]): the instance file read, its template found
//! by the path it pins relative to the instance's folder, the template's
//! canonical hash taken, and the instance merged into it, every problem of
//! the first stage that has errors returned at once. A session on the
//! result reads the template's own files from the template's folder; the
//! files an instance gives are relative to the instance's folder and are
//! moved to the template's on the way in.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::fs;
use std::path::{Path, PathBuf};

use nakkash_format::{Diagnostic, Document, Kind, Result, canon, parse_document};
use serde_json::Value as Json;

pub use nakkash_edit::instance::{
    HIDE, Instance, Level, Merged, Pin, Problem, RawSet, SCHEMA, STORY_ROLES, UNSET, elide,
    has_errors, is_instance, sections_of,
};

use crate::Session;

/// An instance as opened.
#[derive(Debug)]
pub struct Opened {
    /// The instance's file.
    pub path: PathBuf,
    /// The template's file, as the pin names it from the instance's folder.
    pub template_path: PathBuf,
    /// The instance as read, when its schema reads.
    pub instance: Option<Instance>,
    /// The template's canonical hash now.
    pub hash: String,
    /// Every problem the stages found, in stage order.
    pub problems: Vec<Problem>,
    /// The instance with relinks and pins as they are now: what `migrate`
    /// writes. `None` when the schema did not read.
    pub relinked: Option<Instance>,
    /// A session on the template with the instance's values in it, when no
    /// stage found an error.
    pub session: Option<Session>,
}

/// Opens the instance at `path`.
///
/// # Errors
/// A [`Kind::Io`] diagnostic when the instance or its template cannot be
/// read, or the template's own first problem when it is not a valid
/// scene. Problems of the instance are not errors here: they are in
/// [`Opened::problems`].
pub fn open(path: &Path) -> Result<Opened> {
    let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
    let json: Json = serde_json::from_str(&text)
        .map_err(|e| Diagnostic::syntax(format!("invalid JSON in `{}`: {e}", path.display())))?;
    open_json(path, &json)
}

/// Opens the instance `json`, read from `path`.
///
/// # Errors
/// As [`open`].
pub fn open_json(path: &Path, json: &Json) -> Result<Opened> {
    let folder = path.parent().unwrap_or(Path::new(""));
    let pinned_path = json
        .get("template")
        .and_then(|t| t.get("path"))
        .and_then(Json::as_str)
        .map(|p| folder.join(p));
    let instance = match Instance::read(json) {
        Ok(instance) => instance,
        Err(problems) => {
            return Ok(Opened {
                path: path.to_owned(),
                template_path: pinned_path.unwrap_or_default(),
                instance: None,
                hash: String::new(),
                problems,
                relinked: None,
                session: None,
            });
        }
    };
    let template_path = folder.join(&instance.template.path);
    let template = read_template(&template_path)?;
    let hash = canon::hash(template.raw());
    let template_folder = template_path.parent().unwrap_or(Path::new("")).to_owned();
    let moved = |file: &str| moved_file(file, folder, &template_folder);
    let Merged {
        raw,
        mut problems,
        relinked,
        given,
    } = nakkash_edit::instance::merge(&template, &hash, &instance, &moved);
    let session = match raw {
        Some(raw) => Some(Session::on_instance(&template_path, path, raw)?),
        None => None,
    };
    // Stage 4, once the stages before it found no error.
    if let Some(session) = &session
        && !has_errors(&problems)
    {
        problems.extend(crate::brief::measured(session, &given));
    }
    Ok(Opened {
        path: path.to_owned(),
        template_path,
        instance: Some(instance),
        hash,
        problems,
        relinked: Some(relinked),
        session,
    })
}

/// The template at `path`, which must be a scene and not another instance.
fn read_template(path: &Path) -> Result<Document> {
    let text = fs::read_to_string(path).map_err(|e| {
        Diagnostic::io("read", path, &e).expected(
            "the template the instance's `template.path` names, relative to the instance's folder",
        )
    })?;
    // A split template is read flat, so its hash is the one scene's (D91).
    let text = crate::includes::flattened(path, text)?;
    let doc = parse_document(&text)?;
    if is_instance(doc.raw()) {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("`{}` is an instance, not a template", path.display()),
        )
        .expected("an instance of a scene; instances of instances are not made"));
    }
    Ok(doc)
}

/// `file`, written relative to `from`, as written relative to `to`; an
/// absolute file is kept.
fn moved_file(file: &str, from: &Path, to: &Path) -> String {
    match Path::new(file).is_relative() {
        true => {
            crate::relocate::relative_to(&from.join(file), to).unwrap_or_else(|| file.to_owned())
        }
        false => file.to_owned(),
    }
}

/// The layers of `raw` that follow slot `id` of composition `comp` (none
/// for the scene's own), by composition and index.
pub fn followers_of(raw: &Json, comp: Option<&str>, id: &str) -> Vec<(Option<String>, usize)> {
    nakkash_edit::instance::followers(raw, &(comp, id))
}

/// Every problem as one diagnostic, one per line, for commands that only
/// need to know the instance cannot be used.
pub fn diagnostic(opened: &Opened) -> Diagnostic {
    let lines: Vec<String> = opened
        .problems
        .iter()
        .filter(|p| p.level == Level::Error)
        .map(ToString::to_string)
        .collect();
    Diagnostic::new(
        Kind::Invalid,
        format!(
            "the instance `{}` has {} error{}:\n{}",
            opened.path.display(),
            lines.len(),
            if lines.len() == 1 { "" } else { "s" },
            lines.join("\n")
        ),
    )
    .expected("`nakkash check <instance>` lists them as objects with hints")
}

/// A template's own values measured against its slots' contracts, as an
/// instance's are at stage 4: each text in its fit, each colour pair's
/// contrast, each file against its slot. Warnings, since they are the
/// template's: an instance that keeps such a value is told about it again,
/// as an error where the stage refuses it.
pub fn own_problems(session: &Session) -> Vec<Problem> {
    let given: Vec<(String, Json)> = session
        .slots()
        .iter()
        .map(|l| (crate::brief::key_of(l), l.slot.value.clone()))
        .collect();
    crate::brief::measured(session, &given)
        .into_iter()
        .map(|mut problem| {
            problem.level = Level::Warning;
            problem
        })
        .collect()
}
