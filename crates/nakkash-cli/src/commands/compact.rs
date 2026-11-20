//! `nakkash compact`: a scene in its compact form ([D91]), proven the same
//! scene before it is written.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use nakkash_engine::compact::{Compacted, Parts, compact_keeping, default_rows};
use nakkash_engine::{Diagnostic, Document, Kind, Result, write_document};
use serde_json::{Value as Json, json};

/// The parts tried in turn: all of them, then each left out in the order
/// least sure to read back the same, until the result is the same scene.
const TRIES: [Parts; 4] = [
    Parts::ALL,
    Parts {
        defaults: false,
        styles: true,
        eases: true,
    },
    Parts {
        defaults: false,
        styles: true,
        eases: false,
    },
    Parts {
        defaults: false,
        styles: false,
        eases: false,
    },
];

/// Writes the scene at `path` in compact form to `out`, once it reads as
/// the same scene; prints what was written, as data with `json`.
///
/// # Errors
/// A diagnostic when the scene does not read or `out` cannot be written.
pub fn run(path: &Path, out: &Path, json: bool) -> Result<ExitCode> {
    let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
    let raw: Json = serde_json::from_str(&text)
        .map_err(|e| Diagnostic::syntax(format!("invalid JSON: {e}")))?;
    let original = Document::from_value(raw.clone())?;
    let same = |compacted: &Json| {
        Document::from_value(compacted.clone()).is_ok_and(|doc| doc.scene() == original.scene())
    };
    // The rows whose default, left out alone, reads as another scene: a value
    // the scene treats differently written than absent stays written.
    let mut kept: Vec<String> = Vec::new();
    let (all, _) = compact_keeping(&raw, Parts::ALL, &|_| false)?;
    if !same(&all) {
        let only = Parts {
            defaults: true,
            styles: false,
            eases: false,
        };
        for row in default_rows(&raw)? {
            let (alone, _) = compact_keeping(&raw, only, &|r| r != row)?;
            if !same(&alone) {
                kept.push(row);
            }
        }
    }
    let keep = |row: &str| kept.iter().any(|k| k == row);
    let mut chosen = None;
    for parts in TRIES {
        let (compacted, done) = compact_keeping(&raw, parts, &keep)?;
        if same(&compacted) {
            chosen = Some((compacted, done, parts));
            break;
        }
    }
    let Some((compacted, done, parts)) = chosen else {
        return Err(Diagnostic::new(
            Kind::Invalid,
            "no compact form of the scene reads as the same scene",
        ));
    };
    let written = write_document(&compacted);
    fs::write(out, &written).map_err(|e| Diagnostic::io("write", out, &e))?;
    let before = serde_json::to_string(&raw).map_or(0, |s| s.len());
    let after = serde_json::to_string(&compacted).map_or(0, |s| s.len());
    let report = Report {
        before,
        after,
        done,
        parts,
        kept,
    };
    match json {
        true => println!("{}", report.data()),
        false => println!("{report}"),
    }
    Ok(ExitCode::SUCCESS)
}

/// What a compaction did.
struct Report {
    before: usize,
    after: usize,
    done: Compacted,
    parts: Parts,
    /// The rows whose defaults stay written, since leaving them out reads
    /// as another scene.
    kept: Vec<String>,
}

impl Report {
    fn data(&self) -> Json {
        json!({
            "bytes": {"before": self.before, "after": self.after},
            "defaults": self.done.defaults,
            "styles": self.done.styles,
            "inheriting": self.done.inheriting,
            "eases": self.done.eases,
            "eased": self.done.eased,
            "left_out": left_out(self.parts),
            "defaults_kept": self.kept,
        })
    }
}

/// The parts that were left out to keep the scene the same.
fn left_out(parts: Parts) -> Vec<&'static str> {
    [
        (!parts.defaults).then_some("defaults"),
        (!parts.styles).then_some("styles"),
        (!parts.eases).then_some("eases"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let saved = self.before.saturating_sub(self.after);
        let share = 100.0 * saved as f64 / self.before.max(1) as f64;
        writeln!(
            f,
            "{} -> {} bytes compact ({share:.1} % less)",
            self.before, self.after
        )?;
        write!(
            f,
            "  {} defaults left out, {} styles inherited by {} layers, {} eases named by {} keys",
            self.done.defaults,
            self.done.styles,
            self.done.inheriting,
            self.done.eases,
            self.done.eased
        )?;
        if !self.kept.is_empty() {
            write!(
                f,
                "\n  defaults kept, since leaving them out reads as another scene: {}",
                self.kept.join(", ")
            )?;
        }
        let left = left_out(self.parts);
        if !left.is_empty() {
            write!(
                f,
                "\n  left as written, to read as the same scene: {}",
                left.join(", ")
            )?;
        }
        Ok(())
    }
}
