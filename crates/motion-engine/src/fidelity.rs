//! The fidelity ledger ([D35], hardening plan H2): every local sample
//! imported, its text held against the stored layout, and its frames
//! compared with its reference render, reduced to one line of numbers per
//! sample, and compared with the numbers before.
//!
//! The manifest names local files (projects, references, fonts) and stays
//! out of the repository; the ledger holds only sample ids and numbers, so
//! it can be committed and a pull request shows what it changed as numbers.
//!
//! [D35]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d35-fidelity-is-measured-against-reference-renders

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use motion_format::{Diagnostic, Kind, Result};
use motion_import::map::Options;
use serde::{Deserialize, Serialize};

use crate::diff::FrameDiff;
use crate::{DiffOptions, Session, import_scene, write_document};

/// The samples to measure, as a manifest file lists them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The samples, in the order they are measured.
    pub samples: Vec<Sample>,
}

/// One sample: a project, the composition to import, and its reference.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    /// The id the ledger knows it by; never a product or template name.
    pub id: String,
    /// The `.aep` project.
    pub project: PathBuf,
    /// The composition to import, by name; by default the one no other draws.
    #[serde(default)]
    pub comp: Option<String>,
    /// Font files and folders its text is set in.
    #[serde(default)]
    pub fonts: Vec<PathBuf>,
    /// The folder of reference frames, PNG files in order.
    pub reference: PathBuf,
    /// The reference's frame rate.
    pub fps: f64,
    /// The scene time of the first reference frame, in seconds.
    #[serde(default)]
    pub start: f64,
    /// Layers left out of the drawing and the measure, by scene id, such as
    /// a placeholder the reference fills with footage.
    #[serde(default)]
    pub ignore: Vec<String>,
    /// From when, in seconds, the sample has settled: its numbers from then
    /// on are kept apart from the entrance's.
    #[serde(default)]
    pub settled: Option<f64>,
}

impl Manifest {
    /// Reads the manifest at `path`; relative paths in it are relative to
    /// its folder.
    ///
    /// # Errors
    /// An I/O diagnostic when the file cannot be read, or an invalid one
    /// when it is not a manifest, with where.
    pub fn read(path: &Path) -> Result<Manifest> {
        let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
        let mut manifest: Manifest = serde_json::from_str(&text).map_err(|e| {
            Diagnostic::new(Kind::Invalid, format!("{}: {e}", path.display())).expected(
                r#"{"samples":[{"id":"s1","project":"a.aep","reference":"frames/","fps":25}]}"#,
            )
        })?;
        let base = path.parent().unwrap_or(Path::new("."));
        for sample in &mut manifest.samples {
            for path in [&mut sample.project, &mut sample.reference]
                .into_iter()
                .chain(&mut sample.fonts)
            {
                *path = base.join(&*path);
            }
        }
        Ok(manifest)
    }
}

/// One sample's numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    /// What the import could not support, by what, with how many places.
    pub unsupported: BTreeMap<String, usize>,
    /// How many text layers were checked.
    pub texts: usize,
    /// How many of them lie within the tolerance of the stored layout.
    pub texts_within: usize,
    /// How far the worst line lies from the stored one, in pixels; none
    /// when a line is missing or breaks elsewhere.
    pub text_worst: Option<f64>,
    /// Every compared frame.
    pub all: Window,
    /// The frames from when the sample has settled, if it says when.
    pub settled: Option<Window>,
}

/// The errors over a run of frames, each the mean of the frames' own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// From when, in seconds.
    pub from: f64,
    /// How many frames.
    pub frames: usize,
    /// Colour error where the scene draws, 0 to 1.
    pub drawn_error: f64,
    /// Failing fraction where the scene draws.
    pub drawn_failing: f64,
    /// Colour error over everything the scene draws in any frame.
    pub union_error: f64,
    /// Failing fraction over everything the scene draws in any frame.
    pub union_failing: f64,
}

/// Every sample's numbers, by id, as the ledger file holds them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    /// The samples.
    pub samples: BTreeMap<String, Measure>,
}

impl Ledger {
    /// Reads the ledger at `path`, or an empty one when there is none yet.
    ///
    /// # Errors
    /// An I/O diagnostic when the file exists but cannot be read, or an
    /// invalid one when it is not a ledger.
    pub fn read(path: &Path) -> Result<Ledger> {
        if !path.exists() {
            return Ok(Ledger::default());
        }
        let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
        serde_json::from_str(&text)
            .map_err(|e| Diagnostic::new(Kind::Invalid, format!("{}: {e}", path.display())))
    }

    /// The ledger as its file holds it: pretty JSON, numbers rounded so a
    /// change below what the measure can tell does not show.
    pub fn to_json(&self) -> String {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        round(&mut value);
        let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
        text.push('\n');
        text
    }
}

/// Rounds every number under `value` to six decimals.
fn round(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Number(n) if n.is_f64() => {
            let rounded = (n.as_f64().unwrap_or_default() * 1e6).round() / 1e6;
            *value = serde_json::json!(rounded);
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(round),
        serde_json::Value::Object(map) => map.values_mut().for_each(round),
        _ => {}
    }
}

/// Imports `sample`, writes its scene and error images under `work`, and
/// measures it.
///
/// # Errors
/// The import's, the session's or the diff's diagnostics, under the
/// sample's id; text that cannot be laid out (a missing font) is an error
/// too, since a measure without it would flatter the sample.
pub fn measure(sample: &Sample, work: &Path, images: bool) -> Result<Measure> {
    let under = |d: Diagnostic| {
        let message = format!("sample `{}`: {}", sample.id, d.message);
        Diagnostic { message, ..d }
    };
    let dir = work.join(&sample.id);
    fs::create_dir_all(&dir).map_err(|e| under(Diagnostic::io("create", &dir, &e)))?;
    let fonts = sample
        .fonts
        .iter()
        .map(|font| absolute(font).display().to_string())
        .collect();
    let options = Options {
        comp: sample.comp.clone(),
        fonts,
    };
    let import = import_scene(&sample.project, &options).map_err(under)?;
    let scene = dir.join("scene.json");
    fs::write(&scene, write_document(&import.json))
        .map_err(|e| under(Diagnostic::io("write", &scene, &e)))?;
    let mut session = Session::open(&scene).map_err(under)?;
    let geometry = session.text_geometry(&import.texts).map_err(under)?;
    for id in &sample.ignore {
        session.edit(&format!("{id} hidden=true")).map_err(under)?;
    }
    let diff = session
        .diff(&DiffOptions {
            frames: DiffOptions::frames_in(&sample.reference).map_err(under)?,
            fps: sample.fps,
            start: sample.start,
            tolerance: DiffOptions::TOLERANCE,
            images: images.then(|| dir.join("errors")),
        })
        .map_err(under)?;
    let mut unsupported = BTreeMap::new();
    for finding in &import.mapping.unsupported {
        *unsupported.entry(finding.what.clone()).or_insert(0) += 1;
    }
    let worst = geometry.texts.iter().map(|t| t.off()).fold(0.0, f64::max);
    Ok(Measure {
        unsupported,
        texts: geometry.texts.len(),
        texts_within: geometry.texts.len() - geometry.failing().count(),
        text_worst: worst.is_finite().then_some(worst),
        all: window(&diff.frames, sample.start),
        settled: sample.settled.map(|from| {
            let frames: Vec<FrameDiff> = diff
                .frames
                .iter()
                .filter(|f| f.time >= from)
                .cloned()
                .collect();
            window(&frames, from)
        }),
    })
}

/// `path` from the filesystem root, so a scene written elsewhere finds it.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The mean errors of `frames`, from `from`.
fn window(frames: &[FrameDiff], from: f64) -> Window {
    let mean = |get: fn(&FrameDiff) -> f64| match frames.len() {
        0 => 0.0,
        n => frames.iter().map(get).sum::<f64>() / n as f64,
    };
    Window {
        from,
        frames: frames.len(),
        drawn_error: mean(|f| f.drawn.mean),
        drawn_failing: mean(|f| f.drawn.failing),
        union_error: mean(|f| f.union.mean),
        union_failing: mean(|f| f.union.failing),
    }
}

/// How far a number may move before it counts as a change: below what the
/// measure can tell apart on compressed references.
pub const NOISE: f64 = 1e-4;

/// One number that moved between two ledgers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    /// The sample's id.
    pub sample: String,
    /// What moved, such as `settled drawn_failing`.
    pub metric: String,
    /// Before, or none when the sample or the metric is new.
    pub before: Option<f64>,
    /// After, or none when the sample or the metric is gone.
    pub after: Option<f64>,
    /// Whether it moved the right way; none when it appeared or vanished.
    pub better: Option<bool>,
}

/// Every number that moved from `before` to `after`, sample by sample.
pub fn compare(before: &Ledger, after: &Ledger) -> Vec<Change> {
    let ids: std::collections::BTreeSet<&String> =
        before.samples.keys().chain(after.samples.keys()).collect();
    let mut changes = Vec::new();
    for id in ids {
        let old = before.samples.get(id).map(metrics).unwrap_or_default();
        let new = after.samples.get(id).map(metrics).unwrap_or_default();
        let names: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
        for name in names {
            let (b, a) = (old.get(name).copied(), new.get(name).copied());
            let better = match (b, a) {
                (Some(b), Some(a)) if (a - b).abs() <= NOISE => continue,
                (Some(b), Some(a)) => Some(if higher_is_better(name) { a > b } else { a < b }),
                _ => None,
            };
            changes.push(Change {
                sample: id.clone(),
                metric: name.clone(),
                before: b,
                after: a,
                better,
            });
        }
    }
    changes
}

/// Whether more of `metric` is better.
fn higher_is_better(metric: &str) -> bool {
    metric == "texts_within"
}

/// A measure's numbers by name; a text line that is wholly off counts as
/// infinitely far.
fn metrics(measure: &Measure) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for (what, count) in &measure.unsupported {
        out.insert(format!("unsupported: {what}"), *count as f64);
    }
    out.insert("texts_within".into(), measure.texts_within as f64);
    out.insert(
        "text_worst".into(),
        measure.text_worst.unwrap_or(f64::INFINITY),
    );
    let windows = [
        ("all", Some(&measure.all)),
        ("settled", measure.settled.as_ref()),
    ];
    for (name, window) in windows {
        let Some(w) = window else { continue };
        out.insert(format!("{name} drawn_error"), w.drawn_error);
        out.insert(format!("{name} drawn_failing"), w.drawn_failing);
        out.insert(format!("{name} union_error"), w.union_error);
        out.insert(format!("{name} union_failing"), w.union_failing);
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/fidelity.rs"]
mod tests;
