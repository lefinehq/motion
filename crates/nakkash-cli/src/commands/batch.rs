//! `nakkash batch`: every variant of one master (D52). Each variant is the
//! master with its edits applied in order, through the locks, then
//! checked against the master (D78): it passes when its report, rules and
//! pixel gates, has no error the master's lacks. Those that pass are
//! rendered, one at a time, beside their scene.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use nakkash_engine::{Diagnostic, Id, Kind, Quality, RenderOptions, Report, Result, Session};
use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::render::{Encoding, Frames, Written, video};

/// A batch as its file writes it: the master, where the variants go, and
/// the variants.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    /// The master scene, relative to the manifest.
    master: PathBuf,
    /// The folder the variants are written to, relative to the manifest.
    out: PathBuf,
    /// The variants, in the order they are made.
    variants: Vec<Variant>,
}

/// One variant: its id, its edits, and how it renders.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    /// Its id, which names its files.
    id: String,
    /// Edit lines, applied in order.
    #[serde(default)]
    edits: Vec<String>,
    /// How it renders; the defaults of `nakkash render` when left out.
    #[serde(default)]
    render: Render,
}

/// A variant's render settings, as `nakkash render` takes them.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Render {
    codec: Option<String>,
    quality: Option<String>,
    frames: Option<String>,
}

/// What became of a variant.
enum Outcome {
    /// Its edits were made and it rendered; `known` counts the findings
    /// its master already has.
    Rendered {
        written: Written,
        scene: PathBuf,
        seconds: f64,
        known: usize,
    },
    /// Its edits were made and its check passed; it was not rendered.
    Checked { known: usize },
    /// An edit was refused: which, and why.
    Refused { edit: usize, why: Diagnostic },
    /// Its check has an error its master's does not.
    Failed { why: String },
}

/// Makes every variant `path`'s manifest lists; `check` stops before
/// rendering, and `json` prints the outcomes as data. Exits 1 when a
/// variant is refused or fails its check.
///
/// # Errors
/// A diagnostic when the manifest or the master cannot be read, an id is
/// not one or repeats, or a render or a write fails.
pub fn run(path: &Path, check: bool, json: bool) -> Result<ExitCode> {
    let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| {
        Diagnostic::new(Kind::Invalid, format!("`{}` is not a batch: {e}", path.display()))
            .expected(r#"{"master":"scene.json","out":"out","variants":[{"id":"sale","edits":["@headline text=\"Sale\""]}]}"#)
    })?;
    let base = path.parent().unwrap_or(Path::new(""));
    let mut ids = BTreeSet::new();
    for variant in &manifest.variants {
        variant.id.parse::<Id>()?;
        if !ids.insert(variant.id.as_str()) {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!("variant `{}` is listed twice", variant.id),
            )
            .expected("one id per variant"));
        }
    }
    let master = Session::open(base.join(&manifest.master))?;
    // What the master already has is no variant's doing: a variant is
    // measured against it, once for all of them.
    let baseline = master.full_check()?;
    let out = base.join(&manifest.out);
    let mut outcomes = Vec::with_capacity(manifest.variants.len());
    for variant in &manifest.variants {
        let outcome = make(&master, &baseline, variant, &out, check)?;
        if !json {
            println!("{}", line(variant, &outcome, &ids));
        }
        outcomes.push(outcome);
    }
    let made = |o: &Outcome| matches!(o, Outcome::Rendered { .. } | Outcome::Checked { .. });
    let good = outcomes.iter().filter(|o| made(o)).count();
    if json {
        let data: Vec<Json> = manifest
            .variants
            .iter()
            .zip(&outcomes)
            .map(|(v, o)| data(v, o))
            .collect();
        let text = serde_json::to_string_pretty(&data).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the batch as JSON: {e}"),
            )
        })?;
        println!("{text}");
    } else {
        let done = if check { "checked" } else { "rendered" };
        let refused = outcomes
            .iter()
            .filter(|o| matches!(o, Outcome::Refused { .. }))
            .count();
        let failed = outcomes.len() - good - refused;
        println!("{good} {done}, {refused} refused, {failed} failed their check");
    }
    Ok(match good == outcomes.len() {
        true => ExitCode::SUCCESS,
        false => ExitCode::from(1),
    })
}

/// Makes one variant of `master`, whose report is `baseline`, into `out`.
fn make(
    master: &Session,
    baseline: &Report,
    variant: &Variant,
    out: &Path,
    check: bool,
) -> Result<Outcome> {
    let mut session = master.variant();
    for (i, edit) in variant.edits.iter().enumerate() {
        if let Err(why) = session.edit_variant(edit) {
            return Ok(Outcome::Refused { edit: i + 1, why });
        }
    }
    let mut report = session.full_check()?;
    let known = report.since(baseline);
    if report.has_errors() {
        let text = report.to_string();
        let first = text
            .lines()
            .find(|l| l.starts_with("E "))
            .unwrap_or_default();
        return Ok(Outcome::Failed {
            why: first.to_owned(),
        });
    }
    if check {
        return Ok(Outcome::Checked { known });
    }
    let encoding = Encoding::named(
        variant.render.codec.as_deref(),
        variant.render.quality.as_deref(),
    )?;
    let frames = match &variant.render.frames {
        Some(frames) => frames
            .parse::<Frames>()
            .map_err(|why| Diagnostic::new(Kind::Invalid, why))?
            .range(),
        None => session.frames()?,
    };
    // A codec that keeps alpha goes in a QuickTime file.
    let extension = match encoding.codec.unwrap_or_default().keeps_alpha() {
        true => "mov",
        false => "mp4",
    };
    fs::create_dir_all(out).map_err(|e| Diagnostic::io("create", out, &e))?;
    let scene = out.join(format!("{}.json", variant.id));
    session.save_as(&scene)?;
    let started = Instant::now();
    let options = RenderOptions {
        quality: Quality::default(),
        ..RenderOptions::default()
    };
    let file = out.join(format!("{}.{extension}", variant.id));
    let written = video(&session, frames, options, &file, encoding)?;
    Ok(Outcome::Rendered {
        written,
        scene,
        seconds: (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        known,
    })
}

/// One line of the report: the id, padded to the longest, then what
/// became of it.
fn line(variant: &Variant, outcome: &Outcome, ids: &BTreeSet<&str>) -> String {
    let width = ids.iter().map(|id| id.len()).max().unwrap_or(0);
    let edits = match variant.edits.len() {
        1 => "1 edit".to_owned(),
        n => format!("{n} edits"),
    };
    let ok = |known: usize| match known {
        0 => "check OK".to_owned(),
        n => format!("check OK  KNOWN {n}"),
    };
    let what = match outcome {
        Outcome::Rendered {
            written,
            seconds,
            known,
            ..
        } => {
            format!("{}  {}  {seconds} s", ok(*known), written.out.display())
        }
        Outcome::Checked { known } => ok(*known),
        Outcome::Refused { edit, why } => format!("refused: {} (edit {edit})", why.message),
        Outcome::Failed { why } => format!("check failed: {why}"),
    };
    format!("{:width$}  {edits:>8}  {what}", variant.id)
}

fn data(variant: &Variant, outcome: &Outcome) -> Json {
    let mut data = json!({"id": variant.id, "edits": variant.edits});
    let fields = match outcome {
        Outcome::Rendered {
            written,
            scene,
            seconds,
            known,
        } => json!({
            "outcome": "rendered",
            "video": written.out.display().to_string(),
            "scene": scene.display().to_string(),
            "frames": written.frames,
            "seconds": seconds,
            "known": known,
        }),
        Outcome::Checked { known } => json!({"outcome": "checked", "known": known}),
        Outcome::Refused { edit, why } => {
            json!({"outcome": "refused", "edit": edit, "why": why.to_string()})
        }
        Outcome::Failed { why } => json!({"outcome": "failed", "why": why}),
    };
    if let (Json::Object(data), Json::Object(fields)) = (&mut data, fields) {
        data.extend(fields);
    }
    data
}
