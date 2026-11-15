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

use motion_format::{Address, Diagnostic, Kind, Result};
use motion_import::map::Options;
use serde::{Deserialize, Serialize};

use crate::diff::FrameDiff;
use crate::{DiffOptions, Session, import_scene, substitute_fonts, write_document};

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
    /// Layers left out of the drawing and the measure, by address, such as
    /// a placeholder the reference fills with footage: `plate`, or
    /// `card/photo` inside a composition.
    #[serde(default)]
    pub ignore: Vec<Address>,
    /// From when, in seconds, the sample has settled: its numbers from then
    /// on are kept apart from the entrance's.
    #[serde(default)]
    pub settled: Option<f64>,
    /// Why the sample cannot pass however well motion draws it, such as a
    /// reference rendered from other footage than the project names. It is
    /// still measured, so a change to it shows, but kept out of the score.
    #[serde(default)]
    pub excluded: Option<String>,
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
    /// Colour error where the scene draws or the reference differs from its
    /// background, 0 to 1; none in a ledger written before it was measured.
    #[serde(default)]
    pub content_error: Option<f64>,
    /// Failing fraction where the scene draws or the reference differs from
    /// its background: the headline number.
    #[serde(default)]
    pub content_failing: Option<f64>,
    /// How many frames pass over the content: no more than
    /// [`DiffOptions::PASSING`] of its pixels past the tolerance. None in a
    /// ledger written before it was counted.
    #[serde(default)]
    pub frames_passing: Option<usize>,
    /// How many frames have no content at all: the scene draws nothing and
    /// the reference is its background. They pass without showing anything,
    /// so a window of only these proves nothing. None in a ledger written
    /// before it was counted.
    #[serde(default)]
    pub frames_empty: Option<usize>,
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
/// A font the sample does not give is set in the closest face it has
/// ([`substitute_fonts`]), and every use counts as unsupported, so the
/// sample is measured without its gap being hidden.
///
/// # Errors
/// The import's, the session's or the diff's diagnostics, under the
/// sample's id; text that cannot be laid out (a glyph no face has) is an
/// error too, since a measure without it would flatter the sample.
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
        ..Options::default()
    };
    // Footage is named by absolute paths, as fonts are, so the scene finds
    // it from the work folder without a copy.
    let mut import = import_scene(&sample.project, &options, None).map_err(under)?;
    // The sample's fonts are absolute, so the scene finds them from anywhere.
    let unset: Session = write_document(&import.json).parse().map_err(under)?;
    substitute_fonts(&mut import, unset.fonts().map_err(under)?).map_err(under)?;
    let scene = dir.join("scene.json");
    fs::write(&scene, write_document(&import.json))
        .map_err(|e| under(Diagnostic::io("write", &scene, &e)))?;
    let session = Session::open(&scene).map_err(under)?;
    let geometry = session.text_geometry(&import.texts).map_err(under)?;
    let diff = session
        .diff(&DiffOptions {
            frames: DiffOptions::frames_in(&sample.reference).map_err(under)?,
            fps: sample.fps,
            start: sample.start,
            tolerance: DiffOptions::TOLERANCE,
            images: images.then(|| dir.join("errors")),
            ignore: sample.ignore.clone(),
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
        content_error: Some(mean(|f| f.content.mean)),
        content_failing: Some(mean(|f| f.content.failing)),
        frames_passing: Some(frames.iter().filter(|f| f.content.passes()).count()),
        frames_empty: Some(frames.iter().filter(|f| f.content.pixels == 0).count()),
        drawn_error: mean(|f| f.drawn.mean),
        drawn_failing: mean(|f| f.drawn.failing),
        union_error: mean(|f| f.union.mean),
        union_failing: mean(|f| f.union.failing),
    }
}

/// Where one sample stands against the fidelity limit: every compared
/// frame within [`DiffOptions::TOLERANCE`] on all but
/// [`DiffOptions::PASSING`] of its content, and at least one frame with
/// content to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every compared frame passes.
    Passes,
    /// Every frame from when it has settled passes, and one of them has
    /// content, but not every frame of its entrance passes.
    Settles,
    /// It fails, and the import named what it could not support: the gaps,
    /// by what.
    Blocked(Vec<String>),
    /// It fails with nothing named unsupported: something is drawn wrong or
    /// lost without a word, which is the first thing to look at.
    Unexplained,
    /// Its manifest says it cannot pass, and why.
    Excluded(String),
    /// Its numbers were written before frames were counted.
    Unscored,
}

impl Measure {
    /// The sample's verdict; `excluded` is its manifest's reason, if any.
    pub fn verdict(&self, excluded: Option<&str>) -> Verdict {
        if let Some(reason) = excluded {
            return Verdict::Excluded(reason.to_owned());
        }
        // A window passes when all its frames do and one has content: no
        // frame, or only empty ones, shows nothing either way.
        let passes = |w: &Window| {
            let (passing, empty) = (w.frames_passing?, w.frames_empty?);
            Some(passing == w.frames && empty < w.frames)
        };
        match (passes(&self.all), self.settled.as_ref().and_then(passes)) {
            (None, _) => Verdict::Unscored,
            (Some(true), _) => Verdict::Passes,
            (Some(false), Some(true)) => Verdict::Settles,
            (Some(false), _) if self.unsupported.is_empty() => Verdict::Unexplained,
            (Some(false), _) => Verdict::Blocked(self.unsupported.keys().cloned().collect()),
        }
    }
}

/// Every measured sample's verdict, in the manifest's order: the score
/// the fidelity work is steered by, one sample at a time rather than one
/// feature at a time, since a sample passes only once all its gaps close.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scoreboard {
    /// Each sample's id and verdict.
    pub samples: Vec<(String, Verdict)>,
}

impl Scoreboard {
    /// The verdicts of `samples` that `ledger` has numbers for.
    pub fn new<'a>(samples: impl IntoIterator<Item = &'a Sample>, ledger: &Ledger) -> Scoreboard {
        let samples = samples
            .into_iter()
            .filter_map(|sample| {
                let measure = ledger.samples.get(&sample.id)?;
                let verdict = measure.verdict(sample.excluded.as_deref());
                Some((sample.id.clone(), verdict))
            })
            .collect();
        Scoreboard { samples }
    }

    /// The ids whose verdict `matches`.
    pub fn ids(&self, matches: impl Fn(&Verdict) -> bool) -> Vec<&str> {
        self.samples
            .iter()
            .filter(|(_, verdict)| matches(verdict))
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// How many samples count: all but the excluded and the unscored.
    pub fn scored(&self) -> usize {
        self.ids(|v| !matches!(v, Verdict::Excluded(_) | Verdict::Unscored))
            .len()
    }

    /// The failing samples one gap away from the limit, by that gap, the
    /// gap that blocks the most first: what closing one feature would
    /// bring nearest to passing.
    pub fn single_gaps(&self) -> Vec<(&str, Vec<&str>)> {
        let mut by_gap: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (id, verdict) in &self.samples {
            if let Verdict::Blocked(gaps) = verdict
                && let [gap] = gaps.as_slice()
            {
                by_gap.entry(gap.as_str()).or_default().push(id.as_str());
            }
        }
        let mut gaps: Vec<_> = by_gap.into_iter().collect();
        gaps.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
        gaps
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
    metric == "texts_within" || metric.ends_with("frames_passing")
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
        let content = [
            ("content_error", w.content_error),
            ("content_failing", w.content_failing),
        ];
        for (metric, value) in content {
            if let Some(value) = value {
                out.insert(format!("{name} {metric}"), value);
            }
        }
        if let Some(passing) = w.frames_passing {
            out.insert(format!("{name} frames_passing"), passing as f64);
        }
        if let Some(empty) = w.frames_empty {
            out.insert(format!("{name} frames_empty"), empty as f64);
        }
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
