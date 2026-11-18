//! The check after an edit reads again only what the edit can change
//! ([D79]); this holds it to the check from scratch. Every scene of the
//! compatibility corpus, the examples and the fixtures gets many random
//! edits, one after another, and after each the kept check's report must
//! be the one a new session makes of the edited scene, to the bit and in
//! the same order.
//!
//! `NAKKASH_CHECK_SCENES`, folders of scene files joined as `PATH` joins
//! folders, adds every scene in them at any depth (such as large imported
//! templates, which cannot ship here, or `examples/showcase`, whose long
//! films are slow to check unoptimised), and
//! `NAKKASH_CHECK_EDITS` sets how many edits each scene gets.
//!
//! [D79]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d79-an-edit-checks-only-what-it-can-change

use std::fs;
use std::path::{Path, PathBuf};

use nakkash_engine::Session;

/// Edits a scene gets when `NAKKASH_CHECK_EDITS` does not say: few, so the
/// test stays quick unoptimised; a run with more is a release-mode job.
const EDITS: usize = 6;

/// How many edits in a row build on each other before the next run starts
/// again from the scene as read.
const CHAIN: usize = 4;

/// Every scene file the test reads, sorted.
fn scenes() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut scenes = Vec::new();
    for folder in [
        "crates/nakkash-format/tests/compat/v1",
        "examples",
        "crates/nakkash-engine/tests/fixtures",
    ] {
        scenes_in(&root.join(folder), false, &mut scenes);
    }
    if let Some(extra) = std::env::var_os("NAKKASH_CHECK_SCENES") {
        for folder in std::env::split_paths(&extra) {
            scenes_in(&folder, true, &mut scenes);
        }
    }
    scenes.sort();
    scenes
}

/// Adds the scene files in `folder`, and with `deep` in the folders inside
/// it, to `scenes`; files whose names start with a dot are left out.
fn scenes_in(folder: &Path, deep: bool, scenes: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a folder entry").path();
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if path.is_dir() && deep {
            scenes_in(&path, deep, scenes);
        } else if !hidden && path.extension().is_some_and(|e| e == "json") {
            scenes.push(path);
        }
    }
}

/// A small, seeded generator (SplitMix64), so every run makes the same
/// edits and a failure can be replayed.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A whole number below `n`, which is not 0.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// One of `items`, which is not empty.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// A number in `lo..hi`, rounded to hundredths.
    fn number(&mut self, lo: f64, hi: f64) -> f64 {
        let unit = (self.next() >> 11) as f64 / (1_u64 << 53) as f64;
        ((lo + unit * (hi - lo)) * 100.0).round() / 100.0
    }
}

/// A random edit of `session`'s scene: a key of a layer at any depth, as
/// that instance or as its composition writes it, or of the scene. Many
/// are refused by the edit, which is fine: those are skipped.
fn an_edit(session: &Session, draw: &mut Draw) -> String {
    let graph = session.graph().expect("a scene that compiles");
    let paths = graph.paths();
    let ids = draw.pick(&paths).clone();
    let address = ids.join("/");
    let others: Vec<&str> = match graph.node_at(&ids) {
        Some((_, comp)) => comp.nodes.iter().map(|n| n.id.as_str()).collect(),
        None => Vec::new(),
    };
    let other = others
        .get(draw.below(others.len().max(1)))
        .copied()
        .unwrap_or("x");
    let change = match draw.below(16) {
        0 => format!("opacity={}", draw.number(0.0, 1.0)),
        1 => format!(
            "position=[{},{}]",
            draw.number(-400.0, 2400.0),
            draw.number(-400.0, 1400.0)
        ),
        2 => format!("scale={}", draw.number(0.1, 3.0)),
        3 => format!("rotate={}", draw.number(-180.0, 180.0)),
        4 => format!("hidden={}", draw.below(2) == 0),
        5 => format!(
            "at=\"{} {}\"",
            draw.number(-200.0, 2200.0),
            draw.number(-200.0, 1200.0)
        ),
        6 => format!("fill=#{:06X}", draw.next() & 0xFF_FFFF),
        7 => format!(
            "text=\"{}\"",
            draw.pick(&["Sale", "A much longer line of text", "", "OK"])
        ),
        8 => format!("mblur={}", draw.below(2) == 0),
        9 => format!(
            "span=\"{}s-{}s\"",
            draw.number(0.0, 0.5),
            draw.number(0.6, 3.0)
        ),
        10 => format!("parent={other}"),
        11 => format!("matte={other}"),
        12 => "parent=none".to_owned(),
        13 => format!(
            "anchor=[{},{}]",
            draw.number(0.0, 200.0),
            draw.number(0.0, 200.0)
        ),
        14 => format!("blend={}", draw.pick(&["multiply", "screen", "normal"])),
        _ => format!("3d={}", draw.below(2) == 0),
    };
    // A third of the edits change the composition every instance shows.
    match ids.split_last() {
        Some((last, outer)) if !outer.is_empty() && draw.below(3) == 0 => {
            let comp = graph
                .node_at(outer)
                .and_then(|(node, _)| match node.content {
                    nakkash_scene::NodeContent::Comp(i) => Some(graph.comps[i].id.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            format!("scene comps.{comp}.layers[{last}].{change}")
        }
        _ => format!("{address} {change}"),
    }
}

/// The report of `edited`'s scene from scratch: written beside `path`, the
/// scene it was made from, so it reads its files from the same folder, then
/// read by a new session and checked, with no check kept from before.
fn from_scratch(edited: &Session, path: &Path) -> String {
    let name = format!(
        ".incremental-{}-{}",
        std::process::id(),
        path.file_name().expect("a file name").to_string_lossy()
    );
    let copy = path.with_file_name(name);
    fs::write(
        &copy,
        nakkash_format::write_document(edited.document().raw()),
    )
    .expect("the edited scene is written");
    let report = Session::open(&copy).and_then(|session| session.check());
    let _ = fs::remove_file(&copy);
    format!("{:?}", report.expect("the edited scene checks"))
}

/// Where two reports, as `{:?}` writes them, first part, with what follows
/// in each.
fn first_difference(kept: &str, fresh: &str) -> String {
    let at = kept
        .bytes()
        .zip(fresh.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(kept.len().min(fresh.len()));
    let from = at.saturating_sub(300);
    let show = |s: &str| {
        s.get(from..(at + 300).min(s.len()))
            .unwrap_or_default()
            .to_owned()
    };
    format!(
        "at byte {at}\nkept:  …{}\nfresh: …{}",
        show(kept),
        show(fresh)
    )
}

#[test]
fn an_edit_checks_as_a_check_from_scratch() {
    let edits_each: usize = std::env::var("NAKKASH_CHECK_EDITS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(EDITS);
    let (mut scenes_checked, mut compared) = (0, 0);
    for path in scenes() {
        let Ok(master) = Session::open(&path) else {
            continue;
        };
        // Scenes whose fonts, pictures or sounds are not here are left out.
        if master.check().is_err() {
            continue;
        }
        scenes_checked += 1;
        let started = std::time::Instant::now();
        let seed = path.file_name().map_or(0, |n| {
            n.as_encoded_bytes()
                .iter()
                .fold(7_u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(*b)))
        });
        let mut draw = Draw(seed);
        let mut made = 0;
        let mut tries = 0;
        while made < edits_each && tries < edits_each * 20 {
            // A run of edits, each checked from the check before it.
            let mut session = master.variant();
            let mut applied: Vec<String> = Vec::new();
            for _ in 0..CHAIN {
                tries += 1;
                let edit = an_edit(&session, &mut draw);
                let mut next = session.variant();
                let Ok(report) = next.edit(&edit) else {
                    continue;
                };
                applied.push(edit);
                let kept = format!("{report:?}");
                let fresh = from_scratch(&next, &path);
                assert!(
                    kept == fresh,
                    "{}: after {applied:?} the kept check differs from the check from scratch: {}",
                    path.display(),
                    first_difference(&kept, &fresh),
                );
                session = next;
                made += 1;
                compared += 1;
            }
        }
        eprintln!(
            "{}: {made} edits in {:?}",
            path.display(),
            started.elapsed()
        );
    }
    eprintln!("{scenes_checked} scenes, {compared} edits compared");
    assert!(
        scenes_checked >= 10,
        "too few scenes checked: {scenes_checked}"
    );
    assert!(
        compared >= scenes_checked * 5,
        "too few edits applied: {compared}"
    );
}
