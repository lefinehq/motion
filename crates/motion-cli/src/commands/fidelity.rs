//! `motion fidelity`: measure every sample of a local manifest, print one
//! line of numbers per sample, and compare them with a ledger.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Change, Diagnostic, Ledger, Manifest, Measure, Result, Window};

/// What to measure and where to keep it.
pub struct Args<'a> {
    /// The manifest of local samples.
    pub manifest: &'a Path,
    /// The ledger to compare with, if any.
    pub ledger: Option<&'a Path>,
    /// Write the new numbers into the ledger.
    pub write: bool,
    /// Where each sample's scene and error images go.
    pub work: &'a Path,
    /// Write one error image per frame.
    pub images: bool,
    /// Only these samples, by id; all when empty.
    pub only: &'a [String],
}

/// Measures the samples, prints their numbers and what moved since the
/// ledger, and writes the ledger when asked. Exits 1 when a sample cannot
/// be measured, or when a number got worse and the ledger is not being
/// rewritten to accept it.
pub fn run(args: &Args<'_>) -> Result<ExitCode> {
    let manifest = Manifest::read(args.manifest)?;
    let before = match args.ledger {
        Some(path) => Ledger::read(path)?,
        None => Ledger::default(),
    };
    let mut after = before.clone();
    let mut failed = false;
    for sample in &manifest.samples {
        if !args.only.is_empty() && !args.only.contains(&sample.id) {
            continue;
        }
        match motion_engine::measure(sample, args.work, args.images) {
            Ok(measure) => {
                print_measure(&sample.id, &measure);
                after.samples.insert(sample.id.clone(), measure);
            }
            Err(error) => {
                println!("{}: not measured: {error}", sample.id);
                failed = true;
            }
        }
    }
    let changes = motion_engine::compare(&before, &after);
    let worse = changes.iter().any(|c| c.better == Some(false));
    if args.ledger.is_some() {
        print_changes(&changes);
    }
    if let (Some(path), true) = (args.ledger, args.write) {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| Diagnostic::io("create", dir, &e))?;
        }
        fs::write(path, after.to_json()).map_err(|e| Diagnostic::io("write", path, &e))?;
        println!("wrote {}", path.display());
    }
    Ok(match failed || (worse && !args.write) {
        true => ExitCode::from(1),
        false => ExitCode::SUCCESS,
    })
}

/// One sample's numbers, on a few lines.
fn print_measure(id: &str, measure: &Measure) {
    let unsupported: usize = measure.unsupported.values().sum();
    let kinds: Vec<String> = measure
        .unsupported
        .iter()
        .map(|(what, count)| format!("{what} {count}"))
        .collect();
    let worst = measure
        .text_worst
        .map_or("a whole line".into(), |px| format!("{px:.2} px"));
    println!("{id}");
    println!("  unsupported {unsupported}: {}", kinds.join(", "));
    println!(
        "  text {} of {} within the stored layout, worst {worst}",
        measure.texts_within, measure.texts
    );
    println!("  {}", window("all frames", &measure.all));
    if let Some(settled) = &measure.settled {
        println!("  {}", window(&format!("from {}s", settled.from), settled));
    }
}

/// A window's errors, the content's first.
fn window(name: &str, w: &Window) -> String {
    format!(
        "{name} ({}): content {:.4}, {:.2}% failing; where drawn {:.4}, {:.2}% failing; over all drawn {:.4}, {:.2}% failing",
        w.frames,
        w.content_error.unwrap_or_default(),
        w.content_failing.unwrap_or_default() * 100.0,
        w.drawn_error,
        w.drawn_failing * 100.0,
        w.union_error,
        w.union_failing * 100.0
    )
}

/// What moved since the ledger, worse first.
fn print_changes(changes: &[Change]) {
    if changes.is_empty() {
        return println!("no change from the ledger");
    }
    let mut by_kind: BTreeMap<u8, Vec<&Change>> = BTreeMap::new();
    for change in changes {
        let rank = match change.better {
            Some(false) => 0,
            Some(true) => 1,
            None => 2,
        };
        by_kind.entry(rank).or_default().push(change);
    }
    for (rank, list) in by_kind {
        let title = ["worse", "better", "new or gone"][usize::from(rank)];
        println!("{title}:");
        for c in list {
            let num = |v: Option<f64>| v.map_or("-".into(), |v| format!("{v:.4}"));
            println!(
                "  {} {}: {} -> {}",
                c.sample,
                c.metric,
                num(c.before),
                num(c.after)
            );
        }
    }
}
