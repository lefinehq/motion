//! Instances on the command line ([D91]): `check`, `edit`, `migrate` and
//! `slots --as-instance` read and write the small file an agent fills a
//! template with. Every other command reads an instance through
//! `Session::open`, as the scene it makes.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use nakkash_engine::instance::{self, Instance, Level, Problem, has_errors};
use nakkash_engine::{Diagnostic, Kind, Report, Result, Session};
use serde_json::{Value as Json, json};

use super::print_new;

/// Whether the file at `path` is an instance, by its `schema`.
pub fn is_instance(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .filter(|text| text.contains("\"nakkash.Instance."))
        .and_then(|text| serde_json::from_str::<Json>(&text).ok())
        .is_some_and(|json| instance::is_instance(&json))
}

/// `nakkash check <instance>`: every problem of the stages, then what the
/// scene it makes adds to its template's check. Exits 1 on an error of
/// either, 2 when the check cannot run.
pub fn check(path: &Path, json: bool) -> Result<ExitCode> {
    let opened = instance::open(path)?;
    let (report, known) = match &opened.session {
        Some(session) if !has_errors(&opened.problems) => {
            let mut report = session.full_check()?;
            let template = Session::open(&opened.template_path)?.full_check()?;
            let known = report.since(&template);
            (Some(report), known)
        }
        _ => (None, 0),
    };
    let failed = has_errors(&opened.problems) || report.as_ref().is_some_and(Report::has_errors);
    match json {
        true => {
            let out = json!({
                "instance": opened.path.display().to_string(),
                "template": opened.template_path.display().to_string(),
                "problems": opened.problems.iter().map(Problem::to_json).collect::<Vec<_>>(),
                "known": known,
                "report": report.as_ref().map(|r| r.to_string().lines().map(str::to_owned).collect::<Vec<_>>()),
            });
            println!("{}", serde_json::to_string_pretty(&out).expect("JSON writes"));
        }
        false => {
            print_problems(&opened.problems);
            if let Some(report) = report {
                if known > 0 {
                    println!("KNOWN {known} (the template's own)");
                }
                println!("{report}");
            }
        }
    }
    Ok(if failed { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// Prints `problems`, one line each, errors first.
fn print_problems(problems: &[Problem]) {
    let mut sorted: Vec<&Problem> = problems.iter().collect();
    sorted.sort_by_key(|p| (std::cmp::Reverse(p.level), p.stage));
    for problem in sorted {
        println!("{problem}");
    }
}

/// `nakkash edit <instance> '$id=value' …`: sets each slot (`$id=none` or
/// `$id=$unset` returns it to the template's value, `$id=$hide` hides what
/// follows it) and `look=<name>`, merges the result, and writes the
/// instance only when no stage finds an error. Prints the problems and what
/// the edits added to the check.
pub fn edit(path: &Path, lines: &[String]) -> Result<ExitCode> {
    let before = instance::open(path)?;
    let Some(mut edited) = before.relinked.clone().or_else(|| before.instance.clone()) else {
        print_problems(&before.problems);
        return Ok(ExitCode::from(1));
    };
    for line in lines {
        apply(&mut edited, line)?;
    }
    let text = serde_json::to_string_pretty(&edited.to_json()).expect("JSON writes");
    let json: Json = serde_json::from_str(&text).expect("JSON reads");
    let after = instance::open_json(path, &json)?;
    if has_errors(&after.problems) || after.session.is_none() {
        print_problems(&after.problems);
        println!("nothing written: the instance keeps its {} values", before.instance.as_ref().map_or(0, |i| i.slots.len()));
        return Ok(ExitCode::from(1));
    }
    let mut written = after.relinked.clone().unwrap_or(edited);
    let template = Session::open(&after.template_path)?;
    instance::elide(&mut written, template.document());
    write(path, &written)?;
    print_problems(&after.problems);
    let session = after.session.expect("checked above");
    let base = match &before.session {
        Some(s) if !has_errors(&before.problems) => Arc::clone(s.checked()?),
        _ => Arc::clone(template.checked()?),
    };
    let report = session.full_check()?;
    Ok(print_new(report, base.report()))
}

/// Applies one edit line to `instance`.
fn apply(instance: &mut Instance, line: &str) -> Result<()> {
    let refuse = || {
        Diagnostic::new(Kind::Invalid, format!("`{line}` is not an edit of an instance"))
            .expected("`$id=value`, `$id=none`, `$id=$hide` or `look=<name>`; other changes are the template's")
    };
    let (key, value) = line.split_once('=').ok_or_else(refuse)?;
    let key = key.trim();
    if key == "look" {
        instance.look = match value.trim() {
            "" | "none" => None,
            look => Some(look.to_owned()),
        };
        return Ok(());
    }
    let id = key.strip_prefix('$').filter(|id| !id.is_empty()).ok_or_else(refuse)?;
    let value = match value.trim() {
        "none" | "$unset" => json!({ instance::UNSET: true }),
        "$hide" => json!({ instance::HIDE: true }),
        _ => {
            // The value reads as an edit line's value does: quoted text,
            // JSON, or a bare word.
            let edit = nakkash_engine::parse_edit(&format!("scene x={}", value.trim()))?;
            match edit.changes.into_iter().next().map(|c| c.value) {
                Some(nakkash_engine::EditValue::Set(value)) => value,
                _ => json!({ instance::UNSET: true }),
            }
        }
    };
    instance.slots.insert(id.to_owned(), value);
    Ok(())
}

/// Writes `instance` to `path`, through a renamed temp file.
fn write(path: &Path, instance: &Instance) -> Result<()> {
    let text = serde_json::to_string_pretty(&instance.to_json()).expect("JSON writes") + "\n";
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| Diagnostic::io("write", path, &e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Diagnostic::io("write", path, &e)
    })
}

/// `nakkash slots <template> --as-instance [--all] [--out FILE]`: an
/// instance pinned to the template, empty or with every current value,
/// written to `out` (its pin relative to `out`'s folder) or printed (its
/// pin relative to the working folder).
pub fn as_instance(template: &Path, all: bool, out: Option<&Path>) -> Result<ExitCode> {
    if is_instance(template) {
        return Err(Diagnostic::new(Kind::Invalid, format!("`{}` is already an instance", template.display()))
            .expected("the template's scene file"));
    }
    let session = Session::open(template)?;
    let folder = out.and_then(Path::parent).map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let pinned = pathdiff(template, &folder);
    let hash = nakkash_engine::canon::hash(session.document().raw());
    let mut made = Instance::pinned(&pinned, &hash);
    if all {
        for listed in session.slots() {
            let key = match &listed.comp {
                None => listed.slot.id.to_string(),
                Some(comp) => format!("{comp}/{}", listed.slot.id),
            };
            if let Some(uid) = &listed.slot.uid {
                made.template.uids.insert(key.clone(), uid.clone());
            }
            made.slots.insert(key, listed.slot.value.clone());
        }
    }
    match out {
        Some(out) => {
            write(out, &made)?;
            println!("{} pinned to {pinned} ({hash})", out.display());
        }
        None => println!("{}", serde_json::to_string_pretty(&made.to_json()).expect("JSON writes")),
    }
    Ok(ExitCode::SUCCESS)
}

/// `template` as named from `folder`, both with links resolved, so a
/// folder reached through a link names the template the way the file
/// system finds it.
fn pathdiff(template: &Path, folder: &Path) -> String {
    let absolute = |p: &Path| {
        fs::canonicalize(p).unwrap_or_else(|_| std::path::absolute(p).unwrap_or_else(|_| p.to_owned()))
    };
    let (t, f) = (absolute(template), absolute(folder));
    let t: Vec<_> = t.components().collect();
    let f: Vec<_> = f.components().filter(|c| !matches!(c, std::path::Component::CurDir)).collect();
    let shared = t.iter().zip(&f).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = std::iter::repeat_n("..".to_owned(), f.len() - shared).collect();
    parts.extend(t[shared..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    parts.join("/")
}

/// `nakkash migrate <instance> [--dry-run]`: pins the instance to its
/// template as it is now: each relinked slot under its new id, each stale
/// value moved under `stale`, never dropped, the hash and uids pinned
/// again. Prints what changed; writes only without `--dry-run` and only
/// when what is left has no error.
pub fn migrate(path: &Path, dry_run: bool) -> Result<ExitCode> {
    let opened = instance::open(path)?;
    let Some(mut migrated) = opened.relinked.clone() else {
        print_problems(&opened.problems);
        return Ok(ExitCode::from(1));
    };
    let mut moved = Vec::new();
    for problem in opened.problems.iter().filter(|p| p.code == "target.stale" || p.code == "slot.unknown") {
        let Some(key) = &problem.slot else { continue };
        if let Some(value) = opened.instance.as_ref().and_then(|i| i.slots.get(key)) {
            migrated.stale.insert(key.clone(), value.clone());
            migrated.slots.remove(key);
            migrated.template.uids.remove(key);
            moved.push(key.clone());
        }
    }
    let json = migrated.to_json();
    let again = instance::open_json(path, &json)?;
    let left: Vec<&Problem> = again.problems.iter().filter(|p| p.level == Level::Error).collect();
    for problem in opened.problems.iter().filter(|p| p.code == "target.relinked") {
        println!("relinked {}: {}", problem.path, problem.message);
    }
    for key in &moved {
        println!("stale {key}: kept under `stale`");
    }
    println!("pinned to {}", again.hash);
    if !left.is_empty() {
        print_problems(&again.problems);
        return Ok(ExitCode::from(1));
    }
    if !dry_run {
        write(path, &again.relinked.unwrap_or(migrated))?;
    }
    Ok(ExitCode::SUCCESS)
}
