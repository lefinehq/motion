//! `motion fonts fetch`, and the fetch `motion import --fetch-fonts` runs:
//! resolve the fonts a scene lacks, lock them beside it, and download the
//! locked files into the cache ([D39]). The only commands that reach the
//! network.
//!
//! [D39]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use motion_engine::{
    Diagnostic, Kind, LayoutJudge, Result, Session, StoredText, add_fonts_entry, lock_path,
    missing_fonts, write_document,
};
use motion_fonts::fetch::{Choice, Fetcher, Request, Resolution, Sources, Unresolved};
use motion_fonts::{Cache, Lock};
use serde::Serialize;
use serde_json::Value;

/// What a fetch did, for reports.
#[derive(Debug, Default, Serialize)]
pub struct Fetched {
    /// The lock written, relative to the scene's folder.
    pub lock: Option<String>,
    /// How many faces it locks.
    pub faces: usize,
    /// The release each family takes.
    pub choices: Vec<Choice>,
    /// The faces no open source has.
    pub unresolved: Vec<Unresolved>,
    /// How many files were downloaded into the cache.
    pub downloaded: usize,
}

/// A fetcher on this user's cache and the built-in sources, reporting its
/// steps on stderr.
fn fetcher() -> Result<Fetcher> {
    Ok(Fetcher::new(Cache::user()?, Sources::builtin())
        .with_progress(|line| eprintln!("  fonts: {line}")))
}

/// `motion fonts fetch`: locks the faces `scene` lacks, keeps the ones its
/// lock has unless `update`, downloads what the cache lacks, and names the
/// lock in the scene's `fonts`. Exits 1 when a face was not found.
pub fn fetch(scene: &Path, update: bool, release: Option<&str>) -> Result<ExitCode> {
    let session = Session::open(scene)?;
    let lock_file = session
        .locks()
        .into_iter()
        .next()
        .unwrap_or_else(|| lock_path(scene));
    let kept = match !update && lock_file.is_file() {
        true => Lock::read(&lock_file)?.fonts,
        false => Vec::new(),
    };
    let wanted: Vec<String> = missing_fonts(session.scene(), &session.unlocked_fonts()?)
        .into_iter()
        .filter(|name| !kept.iter().any(|f| &f.postscript == name))
        .collect();
    let mut fetcher = fetcher()?;
    let resolution = match wanted.is_empty() {
        true => None,
        false => Some(fetcher.resolve(Request {
            fonts: wanted,
            judge: None,
            release: release.map(str::to_owned),
        })?),
    };
    let mut fonts = kept;
    let (choices, unresolved) = match resolution {
        Some(Resolution {
            lock,
            choices,
            unresolved,
        }) => {
            fonts.extend(lock.fonts);
            (choices, unresolved)
        }
        None => Default::default(),
    };
    let lock = Lock::new(fonts);
    fetcher.install(&lock)?;
    let downloaded = fetcher.downloaded();
    lock.write(&lock_file)?;
    if session.locks().is_empty() {
        name_lock_in_scene(scene, &lock_file)?;
    }
    let fetched = Fetched {
        lock: Some(entry(scene, &lock_file)),
        faces: lock.fonts.len(),
        choices,
        unresolved,
        downloaded,
    };
    print(&fetched);
    Ok(match fetched.unresolved.is_empty() {
        true => ExitCode::SUCCESS,
        false => ExitCode::from(1),
    })
}

/// The fetch of `motion import --fetch-fonts`: the faces the imported scene
/// at `scene` lacks, each family's release chosen by `texts`, the layout
/// the project stored. Writes the lock and names it in `json`, the scene.
pub fn for_import(scene: &Path, json: &mut Value, texts: &[StoredText]) -> Result<Fetched> {
    let session = Session::open(scene)?;
    let base = session.unlocked_fonts()?;
    let wanted = missing_fonts(session.scene(), &base);
    if wanted.is_empty() {
        return Ok(Fetched::default());
    }
    let mut judge = LayoutJudge::new(session.scene(), texts, base);
    let mut fetcher = fetcher()?;
    let resolution = fetcher.resolve(Request {
        fonts: wanted,
        judge: Some(&mut judge),
        release: None,
    })?;
    let lock_file = lock_path(scene);
    fetcher.install(&resolution.lock)?;
    let downloaded = fetcher.downloaded();
    resolution.lock.write(&lock_file)?;
    add_fonts_entry(json, &entry(scene, &lock_file));
    fs::write(scene, write_document(json)).map_err(|e| Diagnostic::io("write", scene, &e))?;
    Ok(Fetched {
        lock: Some(entry(scene, &lock_file)),
        faces: resolution.lock.fonts.len(),
        choices: resolution.choices,
        unresolved: resolution.unresolved,
        downloaded,
    })
}

/// How `scene`'s `fonts` names `lock`, which sits beside it.
fn entry(scene: &Path, lock: &Path) -> String {
    match lock.parent() == scene.parent() {
        true => lock
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        false => lock.display().to_string(),
    }
}

/// Adds `lock` to the `fonts` of the scene file at `scene`.
fn name_lock_in_scene(scene: &Path, lock: &Path) -> Result<()> {
    let text = fs::read_to_string(scene).map_err(|e| Diagnostic::io("read", scene, &e))?;
    let mut json: Value = serde_json::from_str(&text).map_err(|e| {
        Diagnostic::new(
            Kind::Invalid,
            format!("{} is not JSON: {e}", scene.display()),
        )
    })?;
    add_fonts_entry(&mut json, &entry(scene, lock));
    fs::write(scene, write_document(&json)).map_err(|e| Diagnostic::io("write", scene, &e))
}

/// Prints what a fetch did.
pub fn print(fetched: &Fetched) {
    for choice in &fetched.choices {
        let evidence = match &choice.score {
            None => String::new(),
            Some(score) => format!(
                ": {} of {} stored glyphs match, {} of {} lines within the tolerance",
                score.lines.glyphs - score.lines.glyphs_differ.min(score.lines.glyphs),
                score.lines.glyphs,
                score.lines.lines - score.lines.lines_failing,
                score.lines.lines
            ),
        };
        println!(
            "  {}: {} (of {} candidates){evidence}",
            choice.family, choice.release, choice.candidates
        );
    }
    for face in &fetched.unresolved {
        println!("  not found: {}: {}", face.postscript, face.why);
    }
    if let Some(lock) = &fetched.lock {
        println!(
            "  locked {} faces in {lock}, downloaded {} files",
            fetched.faces, fetched.downloaded
        );
    }
}
