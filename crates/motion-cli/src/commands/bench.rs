//! `motion bench`: an agent on every brief of a set, with what each run
//! cost and what `motion check` finds in the scene it wrote. It scores
//! nothing: taste is judged by people ([D22]).
//!
//! [D22]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d22-no-ai-judge

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use motion_engine::{Diagnostic, Kind, Report, Result, Session, Severity};
use serde_json::{Value, json};

use super::render::{Encoding, video};

/// The scene an agent writes in its brief's folder.
const SCENE: &str = "scene.json";
/// What the bench writes in the output folder: one JSON object per brief.
const RESULTS: &str = "results.jsonl";

/// One brief of the set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    /// The folder it sits in under the set, its use case; empty at the top.
    pub group: String,
    /// Its file name without `.md`.
    pub name: String,
    /// The file.
    pub path: PathBuf,
}

impl Brief {
    /// `group/name`, or `name` at the top of the set.
    pub fn id(&self) -> String {
        match self.group.is_empty() {
            true => self.name.clone(),
            false => format!("{}/{}", self.group, self.name),
        }
    }
}

/// Runs `agent` on every brief under `briefs`, one at a time, each in a
/// folder of its own under `out`, and writes one line per brief to
/// `out/results.jsonl`. Prints one line per brief as it ends.
///
/// # Errors
/// A diagnostic when the set cannot be read or `out` written. An agent
/// that fails, times out or writes a scene that does not open is recorded,
/// not an error.
pub fn run(briefs: &Path, agent: &str, out: &Path, timeout: Duration) -> Result<ExitCode> {
    let set = list(briefs)?;
    if set.is_empty() {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("no brief in `{}`", briefs.display()))
                .expected("Markdown files, `*.md`, in folders by use case"),
        );
    }
    fs::create_dir_all(out).map_err(|e| Diagnostic::io("create", out, &e))?;
    let path = out.join(RESULTS);
    let mut results = File::create(&path).map_err(|e| Diagnostic::io("create", &path, &e))?;
    for brief in &set {
        let record = bench(brief, agent, &out.join(brief.id()), timeout)?;
        println!("{}", summary(&brief.id(), &record));
        writeln!(results, "{record}").map_err(|e| Diagnostic::io("write", &path, &e))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The briefs under `root`: its `*.md` files and those one folder down, in
/// name order.
///
/// # Errors
/// A diagnostic when a folder cannot be read.
pub fn list(root: &Path) -> Result<Vec<Brief>> {
    let mut found = Vec::new();
    for entry in sorted(root)? {
        if entry.is_dir() {
            let group = file_stem(&entry);
            for file in sorted(&entry)? {
                if is_brief(&file) {
                    found.push(brief(&group, file));
                }
            }
        } else if is_brief(&entry) {
            found.push(brief("", entry));
        }
    }
    Ok(found)
}

fn sorted(dir: &Path) -> Result<Vec<PathBuf>> {
    let read = fs::read_dir(dir).map_err(|e| Diagnostic::io("read", dir, &e))?;
    let mut paths: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    Ok(paths)
}

fn is_brief(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|e| e == "md")
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn brief(group: &str, path: PathBuf) -> Brief {
    Brief {
        group: group.into(),
        name: file_stem(&path),
        path,
    }
}

/// Runs the agent on `brief` in `dir` and measures what it left.
fn bench(brief: &Brief, agent: &str, dir: &Path, timeout: Duration) -> Result<Value> {
    fs::create_dir_all(dir).map_err(|e| Diagnostic::io("create", dir, &e))?;
    let text =
        fs::read_to_string(&brief.path).map_err(|e| Diagnostic::io("read", &brief.path, &e))?;
    let copy = dir.join("brief.md");
    fs::write(&copy, &text).map_err(|e| Diagnostic::io("write", &copy, &e))?;
    let ran = run_agent(agent, dir, &text, timeout)?;
    let log = fs::read_to_string(dir.join("agent.log")).unwrap_or_default();
    let (tokens, steps) = cost(&log);
    let mut record = json!({
        "brief": brief.id(),
        "group": brief.group,
        "agent_seconds": round(ran.seconds),
        "agent_exit": ran.exit,
        "timed_out": ran.timed_out,
        "tokens": tokens,
        "steps": steps,
    });
    let scene = dir.join(SCENE);
    if scene.is_file() {
        let measured = measure(&scene, &dir.join("video.mp4"));
        let fields = record.as_object_mut().expect("a record is an object");
        match measured {
            Ok(m) => fields.extend(m.as_object().expect("an object").clone()),
            Err(d) => {
                fields.insert("scene_error".into(), d.to_string().into());
            }
        }
    }
    let fields = record.as_object_mut().expect("a record is an object");
    fields.insert("scene".into(), scene.is_file().into());
    Ok(record)
}

/// How an agent run ended.
struct Ran {
    seconds: f64,
    /// Its exit code; `None` when it was stopped or killed by a signal.
    exit: Option<i32>,
    timed_out: bool,
}

/// Runs `agent` through the shell in `dir`, the brief on its stdin and in
/// `MOTION_BRIEF`, its output in `agent.log` and `agent.err`; stops it
/// after `timeout`.
fn run_agent(agent: &str, dir: &Path, brief: &str, timeout: Duration) -> Result<Ran> {
    let create = |name: &str| {
        let path = dir.join(name);
        File::create(&path).map_err(|e| Diagnostic::io("create", &path, &e))
    };
    let start = Instant::now();
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(agent)
        .current_dir(dir)
        .env("MOTION_BRIEF", dir.join("brief.md"))
        .stdin(Stdio::piped())
        .stdout(create("agent.log")?)
        .stderr(create("agent.err")?)
        .spawn()
        .map_err(|e| Diagnostic::io("run", Path::new("sh"), &e))?;
    if let Some(mut stdin) = child.stdin.take() {
        // An agent that reads only `MOTION_BRIEF` may close stdin early.
        let _ = stdin.write_all(brief.as_bytes());
    }
    let wait = |e: std::io::Error| Diagnostic::io("wait for", Path::new("sh"), &e);
    loop {
        if let Some(status) = child.try_wait().map_err(wait)? {
            return Ok(Ran {
                seconds: start.elapsed().as_secs_f64(),
                exit: status.code(),
                timed_out: false,
            });
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Ran {
                seconds: start.elapsed().as_secs_f64(),
                exit: None,
                timed_out: true,
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// The tokens and steps an agent reports as the last line of its output:
/// a JSON object with `tokens` and `steps`, or with `usage` and
/// `num_turns`, as agents that print JSON results write them.
pub fn cost(log: &str) -> (Option<u64>, Option<u64>) {
    let last = log.lines().rev().find(|l| !l.trim().is_empty());
    let Some(Ok(Value::Object(line))) = last.map(serde_json::from_str::<Value>) else {
        return (None, None);
    };
    let tokens = line.get("tokens").and_then(Value::as_u64).or_else(|| {
        let usage = line.get("usage")?.as_object()?;
        let counted = [
            "input_tokens",
            "output_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        ];
        let found: Vec<u64> = counted
            .iter()
            .filter_map(|k| usage.get(*k)?.as_u64())
            .collect();
        (!found.is_empty()).then(|| found.iter().sum())
    });
    let steps = ["steps", "num_turns"]
        .iter()
        .find_map(|k| line.get(*k)?.as_u64());
    (tokens, steps)
}

/// What `motion check` finds in `scene`, how long the scene takes to
/// render into `video`, and how long it is.
fn measure(scene: &Path, video_out: &Path) -> Result<Value> {
    let session = Session::open(scene)?;
    let mut report = session.check()?;
    report.join(session.gates()?);
    let start = Instant::now();
    let written = video(
        &session,
        session.frames()?,
        motion_engine::RenderOptions::default(),
        video_out,
        Encoding::default(),
    )?;
    let render = start.elapsed().as_secs_f64();
    let fps = session.scene().root.fps;
    Ok(json!({
        "render_seconds": round(render),
        "video_seconds": round(written.frames as f64 / fps),
        "errors": count(&report, Severity::Error),
        "warnings": count(&report, Severity::Warning),
        "passed": report.passed,
        "findings": findings(&report),
    }))
}

fn count(report: &Report, severity: Severity) -> usize {
    report
        .violations
        .iter()
        .filter(|v| v.severity == severity)
        .count()
}

/// How many report lines each rule has.
pub fn findings(report: &Report) -> BTreeMap<String, usize> {
    let mut lines = BTreeMap::new();
    for v in &report.violations {
        *lines.entry(v.code.to_string()).or_default() += 1;
    }
    lines
}

/// To a hundredth.
fn round(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// One line for the terminal: the brief, the agent's time and cost, and
/// the check's errors and warnings.
pub fn summary(id: &str, record: &Value) -> String {
    let number = |key: &str| record.get(key).and_then(Value::as_f64);
    let mut parts = vec![id.to_string()];
    if let Some(s) = number("agent_seconds") {
        parts.push(format!("agent {s}s"));
    }
    if record.get("timed_out") == Some(&Value::Bool(true)) {
        parts.push("timed out".into());
    }
    if let Some(steps) = number("steps") {
        parts.push(format!("{steps} steps"));
    }
    if let Some(tokens) = number("tokens") {
        parts.push(format!("{tokens} tokens"));
    }
    match (record.get("scene"), record.get("scene_error")) {
        (Some(Value::Bool(false)), _) => parts.push("no scene".into()),
        (_, Some(Value::String(why))) => parts.push(format!("scene error: {why}")),
        _ => {
            let (e, w) = (
                number("errors").unwrap_or(0.0),
                number("warnings").unwrap_or(0.0),
            );
            parts.push(format!("E{e} W{w}"));
            if let Some(r) = number("render_seconds") {
                parts.push(format!("render {r}s"));
            }
        }
    }
    parts.join("  ")
}

#[cfg(test)]
#[path = "../../tests/unit/commands/bench.rs"]
mod tests;
