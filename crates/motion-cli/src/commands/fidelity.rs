//! `motion fidelity`: measure every sample of a local manifest, print one
//! line of numbers per sample, and compare them with a ledger. The score is
//! given over each frame's codec floor, then strictly (D69). A sample
//! measured over another background than its project's says so.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use motion_engine::{
    Change, Diagnostic, Ledger, Manifest, Measure, Result, Rule, Sample, Scoreboard, Verdict,
    Window,
};

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

/// Measures the samples, prints their numbers, what moved since the
/// ledger and the score, and writes the ledger when asked. Exits 1 when a sample cannot
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
        print_backgrounds(&before, &after);
    }
    let chosen: Vec<&Sample> = manifest
        .samples
        .iter()
        .filter(|s| args.only.is_empty() || args.only.contains(&s.id))
        .collect();
    let floored = chosen
        .iter()
        .filter(|s| s.excluded.is_none())
        .filter(|s| {
            after
                .samples
                .get(&s.id)
                .is_some_and(|m| m.all.floor.is_some())
        })
        .count();
    print_score(
        &Scoreboard::new(chosen.iter().copied(), &after, Rule::Floor),
        &Scoreboard::new(chosen.iter().copied(), &after, Rule::Strict),
        floored,
    );
    let over: Vec<String> = chosen
        .iter()
        .filter_map(|s| {
            Some(format!(
                "{} {}",
                s.id,
                after.samples.get(&s.id)?.bg.as_ref()?
            ))
        })
        .collect();
    if !over.is_empty() {
        println!(
            "  measured over another background than the project's ({}): {}",
            over.len(),
            over.join(", ")
        );
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
    if let Some(bg) = &measure.bg {
        println!("  measured over the background {bg}, not the project's");
    }
    println!("  {}", window("all frames", &measure.all));
    if let Some(settled) = &measure.settled {
        println!("  {}", window(&format!("from {}s", settled.from), settled));
    }
}

/// A window's errors, the content's first; with a codec floor, the frames
/// passing over it, then strictly.
fn window(name: &str, w: &Window) -> String {
    let empty = match w.frames_empty {
        Some(empty) if empty > 0 => format!(" ({empty} empty)"),
        _ => String::new(),
    };
    let passing = match (w.passing(Rule::Floor), w.frames_passing, w.floor) {
        (Some(n), Some(strict), Some(floor)) => {
            let error = w
                .error_floor
                .map_or(String::new(), |error| format!(" and {error:.4}"));
            format!(
                ", {n} of {} frames pass over a floor of {:.2}%{error}{empty}, {strict} strictly",
                w.frames,
                floor * 100.0
            )
        }
        (Some(n), _, _) => format!(", {n} of {} frames pass{empty}", w.frames),
        (None, _, _) => String::new(),
    };
    format!(
        "{name} ({}): content {:.4}, {:.2}% failing{passing}; where drawn {:.4}, {:.2}% failing; over all drawn {:.4}, {:.2}% failing",
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

/// The samples measured over another background than the ledger's: their
/// numbers moved with it, not with how motion draws.
fn print_backgrounds(before: &Ledger, after: &Ledger) {
    let name = |bg: Option<&String>| bg.map_or("the project's".to_owned(), String::clone);
    for (id, measure) in &after.samples {
        let Some(was) = before.samples.get(id) else {
            continue;
        };
        if was.bg != measure.bg {
            println!(
                "background: {id} measured over {}, before over {}",
                name(measure.bg.as_ref()),
                name(was.bg.as_ref())
            );
        }
    }
}

/// The score: how many samples pass over their codec floors, then how many
/// pass strictly, and the failing ones nearest to passing over their
/// floors, those with nothing named unsupported and those one gap away.
/// `floored` scored samples give a floor.
fn print_score(board: &Scoreboard, strict: &Scoreboard, floored: usize) {
    let passes = board.ids(|v| *v == Verdict::Passes);
    let settles = board.ids(|v| *v == Verdict::Settles);
    let excluded = board.ids(|v| matches!(v, Verdict::Excluded(_)));
    let unscored = board.ids(|v| *v == Verdict::Unscored);
    println!(
        "score: {} of {} pass every frame, {} more once settled, {} of {} frames pass, over the codec floor ({floored} with a floor, {} excluded, {} not yet scored)",
        passes.len(),
        board.scored(),
        settles.len(),
        board.frames_passing,
        board.frames,
        excluded.len(),
        unscored.len()
    );
    println!(
        "strict: {} of {} pass every frame, {} more once settled, {} of {} frames pass",
        strict.ids(|v| *v == Verdict::Passes).len(),
        strict.scored(),
        strict.ids(|v| *v == Verdict::Settles).len(),
        strict.frames_passing,
        strict.frames
    );
    let list = |title: &str, ids: &[&str]| {
        if !ids.is_empty() {
            println!("  {title} ({}): {}", ids.len(), ids.join(" "));
        }
    };
    list("pass", &passes);
    list("pass once settled", &settles);
    list(
        "fail with nothing unsupported",
        &board.ids(|v| *v == Verdict::Unexplained),
    );
    let single = board.single_gaps();
    let one: usize = single.iter().map(|(_, ids)| ids.len()).sum();
    if one > 0 {
        println!("  fail with one gap ({one}):");
        for (gap, ids) in &single {
            println!("    {gap} ({}): {}", ids.len(), ids.join(" "));
        }
    }
    let more = board.ids(|v| matches!(v, Verdict::Blocked(gaps) if gaps.len() > 1));
    if !more.is_empty() {
        println!("  fail with more gaps: {}", more.len());
    }
    for (id, verdict) in &board.samples {
        if let Verdict::Excluded(reason) = verdict {
            println!("  excluded {id}: {reason}");
        }
    }
}
