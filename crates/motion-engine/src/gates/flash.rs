//! `flash`: a quarter or more of the frame flashing more than three times
//! in a second, as WCAG 2's general flash threshold counts flashes.

use std::sync::{Mutex, PoisonError};

use motion_format::address::SCENE;
use motion_format::{Address, Result};
use motion_query::{Code, Measure, Report, Severity, Unit, Violation};
use motion_raster::Image;

use super::{luminance, over_black, shrunk};
use crate::session::Session;

/// The frame is measured in this many rows and columns of cells.
const GRID: usize = 4;
/// Cells in the frame.
const CELLS: usize = GRID * GRID;
/// A quarter of the frame, in cells.
const QUARTER: usize = CELLS / 4;
/// The least change of a cell's mean relative luminance that counts.
const CHANGE: f64 = 0.1;
/// A change counts only when its darker side is darker than this.
const DARK: f64 = 0.8;
/// The most flashes a second.
const MOST: f64 = 3.0;

/// The `flash` report of the scene: one error line per run of frames, on
/// `scene`, or one pass. A scene of one frame cannot flash.
pub(super) fn check(session: &Session) -> Result<Report> {
    let frames = session.frames()?;
    let mut report = Report {
        violations: Vec::new(),
        passed: 0,
    };
    if frames.len() < 2 {
        return Ok(report);
    }
    let fps = session.graph()?.root.fps;
    let means = Mutex::new(vec![[0.0; CELLS]; frames.len()]);
    session.on_threads(frames, |frame| {
        let drawn = session
            .display_list(f64::from(frame))
            .and_then(|list| shrunk(session, list))
            .map_err(|d| (frame, d))?;
        means.lock().unwrap_or_else(PoisonError::into_inner)[frame as usize] = cells(&drawn);
        Ok(())
    })?;
    let means = means.into_inner().unwrap_or_else(PoisonError::into_inner);
    let runs = runs(&rates(&means, fps));
    if runs.is_empty() {
        report.passed = 1;
    }
    let scene: Address = SCENE.parse().expect("the scene's address");
    report.violations = runs
        .into_iter()
        .map(|(first, last, worst)| Violation {
            severity: Severity::Error,
            code: Code::Flash,
            address: scene.clone(),
            first_frame: first,
            last_frame: last,
            measure: Measure::Above {
                value: worst,
                limit: MOST,
                unit: Unit::PerSecond,
            },
            fix: None,
        })
        .collect();
    Ok(report)
}

/// The mean relative luminance over black of each cell of `image`, row by
/// row.
fn cells(image: &Image) -> [f64; CELLS] {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut sum = [0.0; CELLS];
    let mut count = [0_u32; CELLS];
    for (i, px) in image.pixels().chunks_exact(4).enumerate() {
        let (x, y) = (i % width, i / width);
        let cell = y * GRID / height * GRID + x * GRID / width;
        sum[cell] += luminance(over_black(px));
        count[cell] += 1;
    }
    std::array::from_fn(|c| match count[c] {
        0 => 0.0,
        n => sum[c] / f64::from(n),
    })
}

/// The frames at which one cell's luminance, `series` frame by frame,
/// completes a change of at least [`CHANGE`] from its last turn, with the
/// darker side darker than [`DARK`]. A change that goes on the same way
/// counts once, however long it takes.
fn turns(series: impl IntoIterator<Item = f64>) -> Vec<usize> {
    let mut found = Vec::new();
    let mut series = series.into_iter().enumerate();
    let Some((_, first)) = series.next() else {
        return found;
    };
    // The last turn, and which way the luminance has gone since.
    let (mut extreme, mut rising) = (first, None);
    for (frame, now) in series {
        match rising {
            Some(true) if now >= extreme => extreme = now,
            Some(false) if now <= extreme => extreme = now,
            _ if (now - extreme).abs() >= CHANGE && now.min(extreme) < DARK => {
                found.push(frame);
                rising = Some(now > extreme);
                extreme = now;
            }
            _ => {}
        }
    }
    found
}

/// For each frame, the flashes in the second up to it that a quarter of the
/// frame reaches: each cell's changes in pairs, a flash being a change and
/// its return, and the cell at the quarter mark of the most flashing.
fn rates(means: &[[f64; CELLS]], fps: f64) -> Vec<f64> {
    let window = (fps.round() as usize).max(1);
    let turns: Vec<Vec<usize>> = (0..CELLS)
        .map(|c| turns(means.iter().map(|cells| cells[c])))
        .collect();
    (0..means.len())
        .map(|frame| {
            let mut flashes: Vec<usize> = turns
                .iter()
                .map(|at| {
                    at.iter()
                        .filter(|&&t| t <= frame && t + window > frame)
                        .count()
                        / 2
                })
                .collect();
            flashes.sort_unstable_by(|a, b| b.cmp(a));
            flashes[QUARTER - 1] as f64
        })
        .collect()
}

/// The runs of consecutive frames whose rate is above [`MOST`]: first and
/// last frame, and the highest rate.
fn runs(rates: &[f64]) -> Vec<(u32, u32, f64)> {
    let mut runs: Vec<(u32, u32, f64)> = Vec::new();
    for (frame, &rate) in (0_u32..).zip(rates) {
        if rate <= MOST {
            continue;
        }
        match runs.last_mut() {
            Some((_, last, worst)) if *last + 1 == frame => {
                *last = frame;
                *worst = worst.max(rate);
            }
            _ => runs.push((frame, frame, rate)),
        }
    }
    runs
}

#[cfg(test)]
#[path = "../../tests/unit/gates/flash.rs"]
mod tests;
