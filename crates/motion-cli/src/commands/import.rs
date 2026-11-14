//! `motion import`: import an `.aep` project as a scene, write it, and print
//! what the import mapped, defaulted, ignored, substituted or could not
//! support, and how far the scene's text lies from the layout the project
//! stored.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use motion_engine::{
    Diagnostic, ImportOptions, Kind, LineCheck, Mapping, Result, Session, TextGeometry,
    write_document,
};
use serde::Serialize;

use super::fonts::{self, Fetched};

/// The text check's outcome: the geometry, or why the scene's text could
/// not be laid out, such as fonts the import was not given.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum TextOutcome {
    /// Every text layer's lines against the stored ones.
    Checked(TextGeometry),
    /// Why the text was not checked.
    NotChecked(String),
}

/// What `--json` prints.
#[derive(Serialize)]
struct Report<'a> {
    /// What the import mapped.
    mapping: &'a Mapping,
    /// What the font fetch did, with `--fetch-fonts`.
    #[serde(skip_serializing_if = "Option::is_none")]
    fonts: Option<&'a Fetched>,
    /// The text check.
    text: TextOutcome,
}

/// How `motion import` runs.
pub struct Args {
    /// Fetch and lock the fonts the scene lacks.
    pub fetch_fonts: bool,
    /// Print the report as JSON.
    pub json: bool,
}

/// Imports `project` with `options`, writes the scene to `out`, fetches its
/// fonts when asked, sets the text of fonts it still lacks in the closest
/// faces it has, checks its text against the stored layout, and prints the
/// report, as JSON when asked.
pub fn run(project: &Path, options: &ImportOptions, out: &Path, args: &Args) -> Result<ExitCode> {
    let mut import = motion_engine::import_scene(project, options, Some(out))?;
    write(out, &import.json)?;
    let fonts = match args.fetch_fonts {
        true => Some(fonts::for_import(out, &mut import.json, &import.texts)?),
        false => None,
    };
    // Only what neither the given fonts nor the fetch have is substituted,
    // from the fonts the scene has where it was written.
    let available = Session::open(out)?;
    motion_engine::substitute_fonts(&mut import, available.fonts()?)?;
    write(out, &import.json)?;
    let json = args.json;
    // Opened from where it was written, so `fonts` resolve as they will.
    let text = match Session::open(out).and_then(|s| s.text_geometry(&import.texts)) {
        Ok(geometry) => TextOutcome::Checked(geometry),
        Err(error) => TextOutcome::NotChecked(error.to_string()),
    };
    if json {
        let report = Report {
            mapping: &import.mapping,
            fonts: fonts.as_ref(),
            text,
        };
        let text = serde_json::to_string_pretty(&report).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the report as JSON: {e}"),
            )
        })?;
        println!("{text}");
    } else {
        print!("{}", import.mapping);
        if let Some(fonts) = &fonts {
            fonts::print(fonts);
        }
        print_text(&text);
        println!("  wrote {}", out.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// Writes the scene `json` to `out`.
fn write(out: &Path, json: &serde_json::Value) -> Result<()> {
    fs::write(out, write_document(json)).map_err(|e| Diagnostic::io("write", out, &e))
}

/// Prints the text check, one line per text layer.
fn print_text(text: &TextOutcome) {
    let geometry = match text {
        TextOutcome::Checked(geometry) if geometry.texts.is_empty() => return,
        TextOutcome::Checked(geometry) => geometry,
        TextOutcome::NotChecked(why) => return println!("  text not checked: {why}"),
    };
    let failing = geometry.failing().count();
    println!(
        "  text against the stored layout: {} of {} within {} px",
        geometry.texts.len() - failing,
        geometry.texts.len(),
        geometry.tolerance
    );
    for check in &geometry.texts {
        let place = match &check.comp {
            Some(comp) => format!("{comp}/{}", check.layer),
            None => check.layer.clone(),
        };
        let lines = check.lines.len();
        let plural = if lines == 1 { "" } else { "s" };
        let worst = check
            .worst_line()
            .filter(|_| check.off() > geometry.tolerance)
            .map(|i| format!(", line {}: {}", i + 1, describe(&check.lines[i])))
            .unwrap_or_default();
        let (differ, glyphs) = check
            .lines
            .iter()
            .filter_map(LineCheck::glyphs_differ)
            .fold((0, 0), |(d, g), (differ, compared)| {
                (d + differ, g + compared)
            });
        let glyphs = match differ {
            0 => String::new(),
            _ => format!(", {differ} of {glyphs} glyphs differ (another version of the font?)"),
        };
        println!(
            "    {place}: {lines} line{plural}, off {}{worst}{glyphs}",
            px(check.off())
        );
    }
}

/// How one line differs, stored first.
fn describe(line: &LineCheck) -> String {
    match (&line.stored, &line.ours) {
        (Some(stored), Some(ours)) if stored.characters != ours.characters => format!(
            "breaks after {} characters, stored after {}",
            ours.characters, stored.characters
        ),
        (Some(stored), Some(ours)) => format!(
            "start {} vs stored {}, width {} vs {}, baseline {} vs {}",
            px(ours.start),
            px(stored.start),
            px(ours.width),
            px(stored.width),
            px(ours.baseline),
            px(stored.baseline)
        ),
        (Some(_), None) => "stored, but the scene has no such line".into(),
        (None, _) => "the scene's, but not stored".into(),
    }
}

/// `value` pixels to two decimals.
fn px(value: f64) -> String {
    match value.is_finite() {
        true => format!("{value:.2}"),
        false => "a whole line".into(),
    }
}
