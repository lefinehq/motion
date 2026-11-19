//! `nakkash vary`: generated variants of one master (D78), along palette,
//! type, content, timing and format, and improved by the check's repairs.
//! Each is measured against the master and, unless `--dry-run`, rendered
//! with a sheet of its frames.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use nakkash_engine::vary::{
    ContentSpec, FormatSpec, Made, Pace, PaletteSpec, Recipe, Sheet, Spec, TimingSpec, TypeSpec,
    gate, make, plan, repair, sheet,
};
use nakkash_engine::{
    Code, Diagnostic, Kind, Quality, RenderOptions, Report, Result, Session, Severity,
};
use serde_json::{Value as Json, json};

use super::render::{Encoding, video};

/// What the command line asks for, besides the spec file.
#[derive(Debug, Default)]
pub struct Asked {
    /// The spec file, if any.
    pub spec: Option<PathBuf>,
    /// The seed, over the spec's.
    pub seed: Option<u64>,
    /// How many variants, over the spec's.
    pub count: Option<usize>,
    /// `--axis` values: `palette`, `palette=invert`, `type`,
    /// `type=contrast`, `copy=long`, `format=9:16`, `timing=15s`,
    /// `bpm=120`, `music=track.wav`, `pace=snappy`, `rows=rows.csv`.
    pub axes: Vec<String>,
    /// `--improve`.
    pub improve: bool,
    /// `--apart`: each variant takes one axis.
    pub apart: bool,
    /// `--dry-run`: check only.
    pub dry_run: bool,
    /// `--json`.
    pub json: bool,
    /// The output folder.
    pub out: PathBuf,
}

/// Makes the variants of `scene` that `asked` describes into its folder;
/// exits 1 when any adds an error to the master's report.
///
/// # Errors
/// A diagnostic when the scene, the spec or a file it names cannot be read,
/// an axis does not read, or a variant cannot be written or rendered.
pub fn run(scene: &Path, asked: &Asked) -> Result<ExitCode> {
    let (mut spec, base) = match &asked.spec {
        Some(path) => {
            let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
            let spec: Spec = serde_json::from_str(&text).map_err(|e| {
                Diagnostic::new(
                    Kind::Invalid,
                    format!("`{}` is not a vary spec: {e}", path.display()),
                )
                .expected(r#"{"seed":7,"count":4,"palette":{},"format":{"sizes":["9:16"]}}"#)
            })?;
            (spec, path.parent().unwrap_or(Path::new("")).to_path_buf())
        }
        None => (Spec::default(), PathBuf::new()),
    };
    for axis in &asked.axes {
        add_axis(&mut spec, axis)?;
    }
    if let Some(seed) = asked.seed {
        spec.seed = seed;
    }
    if asked.count.is_some() {
        spec.count = asked.count;
    }
    spec.improve |= asked.improve;
    spec.apart |= asked.apart;
    let nothing = spec.palette.is_none()
        && spec.typeface.is_none()
        && spec.content.is_none()
        && spec.timing.is_none()
        && spec.format.is_none()
        && !spec.improve;
    if nothing {
        return Err(Diagnostic::new(Kind::Missing, "no axis to vary along")
            .expected("`--axis palette`, `--axis format=9:16`, `--improve` or a spec file"));
    }
    let master = Session::open(scene)?;
    let started = Instant::now();
    let baseline = master.full_check()?;
    let out = &asked.out;
    fs::create_dir_all(out).map_err(|e| Diagnostic::io("create", out, &e))?;
    write(&out.join("master.check.txt"), &format!("{baseline}\n"))?;
    let recipes = plan(&master, &spec, &base)?;
    let mut rows = Vec::with_capacity(recipes.len());
    let mut made_sessions = Vec::new();
    let mut passed = 0;
    for recipe in &recipes {
        let dir = out.join(&recipe.id);
        fs::create_dir_all(&dir).map_err(|e| Diagnostic::io("create", &dir, &e))?;
        write(&dir.join("recipe.json"), &pretty(&json!(recipe))?)?;
        let row = match make(&master, recipe, &base) {
            Ok(mut made) => {
                let report = checked(&mut made, &master, &baseline);
                match report {
                    Ok(report) => {
                        let full = format!("{report}\n");
                        let gated = gate(&baseline, report, &made);
                        let new = gated.new.to_string();
                        write(
                            &dir.join("check.txt"),
                            &format!(
                                "{full}\nnew over the master (KNOWN {}):\n{new}\n",
                                gated.known
                            ),
                        )?;
                        made.session.save_as(&dir.join("scene.json"))?;
                        let ok = gated.passed();
                        passed += usize::from(ok);
                        let mut film = None;
                        if ok && !asked.dry_run {
                            film = Some(render(&made.session, &dir)?);
                        }
                        let first_new =
                            new.lines().find(|l| l.starts_with("E ")).map(str::to_owned);
                        if !asked.dry_run {
                            let image = sheet(&[&made.session], Sheet::default())?;
                            write_png(&dir.join("sheet.png"), &image)?;
                        }
                        let row = json!({
                            "id": recipe.id,
                            "recipe": recipe.to_string(),
                            "passed": ok,
                            "known": gated.known,
                            "new": gated.new.violations.len(),
                            "new_error": first_new,
                            "notes": made.notes,
                            "film": film,
                        });
                        if !asked.dry_run {
                            made_sessions.push(made.session);
                        }
                        row
                    }
                    Err(why) => failed(recipe, &why),
                }
            }
            Err(why) => failed(recipe, &why),
        };
        if !asked.json {
            println!("{}", line(&row));
        }
        rows.push(row);
    }
    if !asked.dry_run && !made_sessions.is_empty() {
        let mut all: Vec<&Session> = vec![&master];
        all.extend(made_sessions.iter());
        let image = sheet(&all, Sheet::default())?;
        write_png(&out.join("sheet.png"), &image)?;
    }
    let seconds = (started.elapsed().as_secs_f64() * 10.0).round() / 10.0;
    let report = json!({
        "master": scene.display().to_string(),
        "seed": spec.seed,
        "variants": rows,
        "passed": passed,
        "seconds": seconds,
    });
    write(&out.join("report.json"), &pretty(&report)?)?;
    if asked.json {
        println!("{}", pretty(&report)?);
    } else {
        let failed = recipes.len() - passed;
        println!(
            "{passed} passed, {failed} failed  ·  {} s  ·  {}",
            seconds,
            out.display()
        );
    }
    Ok(match passed == recipes.len() {
        true => ExitCode::SUCCESS,
        false => ExitCode::from(1),
    })
}

/// `made`'s full report, after repairing the contrast errors it adds to
/// `baseline`, its master's.
fn checked(made: &mut Made, master: &Session, baseline: &Report) -> Result<Report> {
    let report = made.session.full_check()?;
    let faint = gate(baseline, report.clone(), made)
        .new
        .violations
        .iter()
        .any(|v| v.code == Code::Contrast && v.severity == Severity::Error);
    if !faint {
        return Ok(report);
    }
    Ok(repair(made, master, baseline, report)?.1)
}

/// The row of a variant that could not be made or checked.
fn failed(recipe: &Recipe, why: &Diagnostic) -> Json {
    json!({
        "id": recipe.id,
        "recipe": recipe.to_string(),
        "passed": false,
        "error": why.to_string(),
    })
}

/// One line of the report.
fn line(row: &Json) -> String {
    let id = row["id"].as_str().unwrap_or_default();
    let recipe = row["recipe"].as_str().unwrap_or_default();
    let what = if let Some(error) = row["error"].as_str() {
        format!("not made: {error}")
    } else if row["passed"] == json!(true) {
        let known = row["known"].as_u64().unwrap_or(0);
        let film = row["film"]
            .as_str()
            .map(|f| format!("  {f}"))
            .unwrap_or_default();
        // As `batch` says it: what the master already has, when it has any.
        let known = match known {
            0 => String::new(),
            n => format!("  KNOWN {n}"),
        };
        format!("check OK{known}{film}")
    } else {
        format!("new: {}", row["new_error"].as_str().unwrap_or("an error"))
    };
    format!("{id:<4} {recipe:<48}  {what}")
}

/// Renders `session` into `dir/film.mp4`.
fn render(session: &Session, dir: &Path) -> Result<String> {
    let file = dir.join("film.mp4");
    let options = RenderOptions {
        quality: Quality::default(),
        ..RenderOptions::default()
    };
    let written = video(
        session,
        session.frames()?,
        options,
        &file,
        Encoding::named(None, None)?,
    )?;
    Ok(written.out.display().to_string())
}

/// Adds `--axis` value `axis` to `spec`.
fn add_axis(spec: &mut Spec, axis: &str) -> Result<()> {
    let (name, value) = match axis.split_once('=') {
        Some((n, v)) => (n, Some(v.to_owned())),
        None => (axis, None),
    };
    let bad = || {
        Diagnostic::new(Kind::Invalid, format!("`{axis}` is not an axis"))
            .expected("palette[=scheme], brand=#hex, type[=contrast|same], copy=long|<text>, format=9:16, timing=15s, bpm=120, music=<file>, pace=snappy|calm, rows=<file>")
    };
    match name {
        "palette" => {
            let p = spec.palette.get_or_insert_with(PaletteSpec::default);
            p.schemes.extend(value);
        }
        "brand" => {
            let p = spec.palette.get_or_insert_with(PaletteSpec::default);
            p.brand.extend(
                value
                    .into_iter()
                    .flat_map(|v| v.split(',').map(str::to_owned).collect::<Vec<_>>()),
            );
            p.schemes.push("brand".into());
        }
        "type" => {
            let t = spec.typeface.get_or_insert_with(TypeSpec::default);
            t.pairs = value;
        }
        "copy" => {
            let t = spec.typeface.get_or_insert_with(TypeSpec::default);
            t.copy = value;
        }
        "fonts" => {
            let t = spec.typeface.get_or_insert_with(TypeSpec::default);
            t.fonts.extend(value.map(PathBuf::from));
        }
        "format" => {
            let f = spec.format.get_or_insert_with(FormatSpec::default);
            f.sizes.extend(value);
        }
        "timing" => {
            let t = spec.timing.get_or_insert_with(TimingSpec::default);
            t.durations.extend(value);
        }
        "bpm" => {
            let t = spec.timing.get_or_insert_with(TimingSpec::default);
            t.bpm = Some(value.ok_or_else(bad)?.parse().map_err(|_| bad())?);
        }
        "music" => {
            let t = spec.timing.get_or_insert_with(TimingSpec::default);
            t.music = value.map(PathBuf::from);
        }
        "pace" => {
            let t = spec.timing.get_or_insert_with(TimingSpec::default);
            t.pace = Some(match value.as_deref() {
                Some("snappy") => Pace::Snappy,
                Some("calm") => Pace::Calm,
                _ => return Err(bad()),
            });
        }
        "rows" => {
            let c = spec.content.get_or_insert_with(ContentSpec::default);
            c.rows = value.map(PathBuf::from);
        }
        _ => return Err(bad()),
    }
    Ok(())
}

fn pretty(value: &Json) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| Diagnostic::new(Kind::Invalid, format!("cannot write JSON: {e}")))
}

fn write(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).map_err(|e| Diagnostic::io("write", path, &e))
}

fn write_png(path: &Path, image: &nakkash_engine::Image) -> Result<()> {
    fs::write(path, image.to_png()?).map_err(|e| Diagnostic::io("write", path, &e))
}
