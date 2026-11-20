//! The timing axis ([D78]): a new length, a beat grid or a pace, made as a
//! time map on the master.
//!
//! The master is evaluated frame by frame. A frame is active when some
//! layer that draws moves, turns, grows or fades from the frame before;
//! the others are holds. Active frames keep the master's rate (or the
//! pace's), so every move keeps its speed and its easing curve's shape,
//! and the holds take up the difference, at a rate between 1/3 and 3.
//! With a beat grid, the moments where moves start after a hold are moved
//! to the nearest beat, in order.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use nakkash_format::units::Time;
use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_scene::walk;
use serde::{Deserialize, Serialize};

use super::TimingSpec;
use super::rng::Rng;
use super::wrap::TimeMap;
use crate::session::Session;

/// How fast moves play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pace {
    /// Moves take 3/4 of their time.
    Snappy,
    /// Moves take 4/3 of their time.
    Calm,
}

impl Pace {
    /// How long a move takes, against the master.
    fn factor(self) -> f64 {
        match self {
            Pace::Snappy => 0.75,
            Pace::Calm => 4.0 / 3.0,
        }
    }
}

/// A beat grid.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Beats {
    /// Beats a minute.
    pub bpm: f64,
    /// Where the first beat falls, in seconds.
    pub offset: f64,
}

/// The timing step of a recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingStep {
    /// The new length, in seconds; the master's when unset.
    pub seconds: Option<f64>,
    /// The beat grid moves start on.
    pub beats: Option<Beats>,
    /// The pace of moves.
    pub pace: Option<Pace>,
    /// The music track played at the root, relative to the master's folder.
    pub music: Option<PathBuf>,
}

impl fmt::Display for TimingStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = vec!["timing".to_owned()];
        if let Some(s) = self.seconds {
            parts.push(format!("{}s", (s * 100.0).round() / 100.0));
        }
        if let Some(b) = self.beats {
            parts.push(format!("{} bpm", (b.bpm * 10.0).round() / 10.0));
        }
        if let Some(p) = self.pace {
            parts.push(format!("{p:?}").to_lowercase());
        }
        write!(f, "{}", parts.join(" "))
    }
}

/// Draws a timing step from `spec`: one of its lengths, its grid (from
/// its tempo, or found in its track) and its pace.
pub(crate) fn draw(
    master: &Session,
    spec: &TimingSpec,
    base: &Path,
    rng: &mut Rng,
) -> Result<TimingStep> {
    let fps = master.scene().root.fps;
    let own = master.scene().root.duration.to_frames(fps) / fps;
    let lengths: Vec<f64> = spec
        .durations
        .iter()
        .map(|d| match d.strip_suffix('%') {
            Some(share) => share.parse::<f64>().map(|p| own * p / 100.0).map_err(|_| {
                Diagnostic::new(Kind::Invalid, format!("`{d}` is not a share of the length"))
                    .expected("a length such as `15s`, `450f` or `67%`")
            }),
            None => Ok(d.parse::<Time>()?.to_frames(fps) / fps),
        })
        .collect::<Result<_>>()?;
    let seconds = rng.pick(&lengths).copied();
    let offset = match &spec.offset {
        Some(o) => o.parse::<Time>()?.to_frames(fps) / fps,
        None => 0.0,
    };
    let music = spec.music.as_ref().map(|m| base.join(m));
    let beats = match (spec.bpm, &music) {
        (Some(bpm), _) => Some(Beats { bpm, offset }),
        (None, Some(track)) => Some(find_beats(track)?),
        (None, None) => None,
    };
    // The track is written relative to the variant's scene, which is
    // written in the master's folder's terms.
    let music = music.map(|m| relative_to_master(master, &m));
    Ok(TimingStep {
        seconds,
        beats,
        pace: spec.pace,
        music,
    })
}

/// `path` as the master's scene would name it.
fn relative_to_master(master: &Session, path: &Path) -> PathBuf {
    let folder = master
        .path()
        .and_then(Path::parent)
        .unwrap_or(Path::new(""));
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let folder = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
    pathdiff::diff_paths(&absolute, &folder).unwrap_or(absolute)
}

/// Which of the master's frames are active: something that draws changes
/// from the frame before. Frame 0 is a hold.
pub(crate) fn activity(session: &Session) -> Result<Vec<bool>> {
    let frames = session.frames()?;
    let graph = session.graph()?;
    let mut before: BTreeMap<usize, ([f64; 4], f64)> = BTreeMap::new();
    let mut active = Vec::with_capacity(frames.len());
    for frame in frames {
        let state = session.frame(f64::from(frame))?;
        let mut now = BTreeMap::new();
        let mut moved = false;
        for visit in walk(graph, &state) {
            if !visit.visible {
                continue;
            }
            let b = visit.bounds();
            let seen = ([b.x0, b.y0, b.x1, b.y1], visit.opacity);
            if let Some((was, opacity)) = before.get(&visit.index) {
                let shift = was
                    .iter()
                    .zip(&seen.0)
                    .fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()));
                moved |= shift > 0.5 || (opacity - seen.1).abs() > 0.01;
            } else if !before.is_empty() {
                // Something appears.
                moved = true;
            }
            now.insert(visit.index, seen);
        }
        moved |= before.keys().any(|k| !now.contains_key(k));
        active.push(moved && !before.is_empty());
        before = now;
    }
    if let Some(first) = active.first_mut() {
        *first = false;
    }
    Ok(active)
}

/// The time map `step` makes of `session`, and a line on what it did.
pub(crate) fn time_map(session: &Session, step: &TimingStep) -> Result<(TimeMap, String)> {
    let fps = session.scene().root.fps;
    let active = activity(session)?;
    let n = active.len();
    let target = step
        .seconds
        .map_or(n as f64, |s| (s * fps).round().max(1.0));
    let pace = step.pace.map_or(1.0, Pace::factor);
    // The moves that start after a hold of at least 3 frames, which a beat
    // grid lands on beats.
    let mut starts: Vec<usize> = (3..n)
        .filter(|&i| active[i] && active[i - 3..i].iter().all(|a| !a))
        .collect();
    // The root's labels, the master's own sections, land on beats too.
    starts.extend(
        session
            .scene()
            .root
            .labels
            .iter()
            .map(|(_, t)| t.to_frames(fps).round() as usize)
            .filter(|&f| f > 0 && f < n),
    );
    starts.sort_unstable();
    starts.dedup();
    let mut anchors = vec![(0usize, 0.0), (n, target)];
    let mut snapped = 0;
    if let Some(beats) = step.beats {
        let free = durations(&active, 0, n, target, pace);
        let mut out = Vec::with_capacity(n + 1);
        let mut t = 0.0;
        out.push(t);
        for d in &free {
            t += d;
            out.push(t);
        }
        let period = 60.0 / beats.bpm * fps;
        let first = beats.offset * fps;
        let mut last = 0.0;
        let mut inner = Vec::new();
        for &s in &starts {
            let beat = first + ((out[s] - first) / period).round() * period;
            if beat > last + 1.0 && beat < target - 1.0 && beat > 0.0 {
                inner.push((s, beat));
                last = beat;
            }
        }
        snapped = inner.len();
        anchors = std::iter::once((0, 0.0))
            .chain(inner)
            .chain(std::iter::once((n, target)))
            .collect();
    }
    // Each stretch between anchors gets its length.
    let mut lengths = vec![0.0; n];
    for pair in anchors.windows(2) {
        let ((a, ta), (b, tb)) = (pair[0], pair[1]);
        let part = durations(&active, a, b, tb - ta, pace);
        lengths[a..b].copy_from_slice(&part);
    }
    // Keys at every change of rate: (frame of the variant, second of the
    // master).
    let mut keys = vec![(0.0, 0.0)];
    let mut t = 0.0;
    for (i, d) in lengths.iter().enumerate() {
        let rate_changes = i + 1 < n && (lengths[i + 1] - d).abs() > 1e-9;
        t += d;
        if rate_changes || i + 1 == n {
            keys.push((t, (i + 1) as f64 / fps));
        }
    }
    // The master shows nothing from its end on: the last key holds its last
    // frame.
    if let Some(last) = keys.last_mut() {
        last.1 = (n as f64 - 1.0).max(0.0) / fps;
    }
    whole_frames(&mut keys, target, fps);
    let held = active.iter().filter(|a| !**a).count();
    let note = format!(
        "{step}: {n} frames to {target}, {} active, {held} held, {} keys{}",
        n - held,
        keys.len(),
        match step.beats {
            Some(_) => format!(", {snapped} of {} move starts on beats", starts.len()),
            None => String::new(),
        }
    );
    Ok((
        TimeMap {
            frames: target,
            keys,
        },
        note,
    ))
}

/// Moves `keys` onto whole frames of the variant, each at least a frame
/// past the one before and the last at `target`: a stretch at the master's
/// own rate (`fps` master frames a second) keeps its length from a whole
/// frame, so it shows the master's very frames, not moments between two,
/// which no check of the master has seen (a fade caught a third of the way
/// in reads fainter than either frame it lies between). The other
/// stretches, holds mostly, take up the rounding.
fn whole_frames(keys: &mut [(f64, f64)], target: f64, fps: f64) {
    let last = keys.len().saturating_sub(1);
    let mut was = keys.first().copied().unwrap_or_default();
    if let Some(first) = keys.first_mut() {
        first.0 = first.0.round();
    }
    for i in 1..keys.len() {
        let (t, m) = keys[i];
        let own_rate = ((t - was.0) - (m - was.1) * fps).abs() < 1e-6;
        let before = keys[i - 1].0;
        let room = target - (last - i) as f64;
        keys[i].0 = match (i == last, own_rate) {
            (true, _) => target,
            (false, true) => (before + (t - was.0).round()).min(room),
            (false, false) => t.round().max(before + 1.0).min(room),
        };
        was = (t, m);
    }
}

/// The length each of frames `a..b` of the master takes in the variant,
/// summing to `total`: active frames `pace` each and holds what is left,
/// each at a rate between 1/3 and 3, then all alike when that is not
/// enough.
fn durations(active: &[bool], a: usize, b: usize, total: f64, pace: f64) -> Vec<f64> {
    let span = &active[a..b];
    let moving = span.iter().filter(|x| **x).count() as f64;
    let holding = span.len() as f64 - moving;
    let (mut on, mut off) = (pace, 1.0);
    if holding > 0.0 {
        off = (total - moving * on) / holding;
        if !(1.0 / 3.0..=3.0).contains(&off) {
            off = off.clamp(1.0 / 3.0, 3.0);
            if moving > 0.0 {
                on = ((total - holding * off) / moving).clamp(1.0 / 3.0, 3.0);
            }
        }
    } else if moving > 0.0 {
        on = total / moving;
    }
    let sum = moving * on + holding * off;
    let k = match sum > 0.0 {
        true => total / sum,
        false => 1.0,
    };
    span.iter()
        .map(|&x| match x {
            true => on * k,
            false => off * k,
        })
        .collect()
}

/// The beat grid of a music track: its tempo from the autocorrelation of
/// its onsets (spectral flux of the summed channels, between 70 and 180
/// beats a minute), and the phase whose beats land on the most onset.
///
/// # Errors
/// A diagnostic when the track cannot be read, or is too short to time.
pub(crate) fn find_beats(track: &Path) -> Result<Beats> {
    let sound = nakkash_audio::open(track)?;
    let rate = f64::from(sound.rate());
    let channels = sound.channels();
    let frames = sound.frames();
    let hop = 512;
    let window = 1024;
    if frames < window * 8 {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("`{}` is too short to find its beats", track.display()),
        )
        .expected("a track of a few seconds or more"));
    }
    // Onset strength: the rise of energy in 8 bands, frame to frame.
    let mono: Vec<f32> = (0..frames)
        .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / channels.len().max(1) as f32)
        .collect();
    let bands = 8;
    let mut previous = vec![0.0_f64; bands];
    let mut onsets = Vec::new();
    let mut start = 0;
    while start + window <= frames {
        let chunk = &mono[start..start + window];
        // Band energies through a cheap filter bank: differences of moving
        // averages at octave-spaced widths.
        let mut energy = vec![0.0_f64; bands];
        for (band, e) in energy.iter_mut().enumerate() {
            let width = 1usize << band;
            let mut acc = 0.0;
            for i in width..chunk.len() {
                let d = f64::from(chunk[i] - chunk[i - width]);
                acc += d * d;
            }
            *e = (acc / chunk.len() as f64 + 1e-12).ln();
        }
        let flux: f64 = energy
            .iter()
            .zip(&previous)
            .map(|(e, p)| (e - p).max(0.0))
            .sum();
        onsets.push(flux);
        previous = energy;
        start += hop;
    }
    let mean = onsets.iter().sum::<f64>() / onsets.len() as f64;
    let onsets: Vec<f64> = onsets.iter().map(|o| (o - mean).max(0.0)).collect();
    let per_second = rate / hop as f64;
    let lag = |bpm: f64| 60.0 / bpm * per_second;
    let score = |l: f64| -> f64 {
        let whole = l.floor() as usize;
        let frac = l - whole as f64;
        (0..onsets.len().saturating_sub(whole + 1))
            .map(|i| onsets[i] * (onsets[i + whole] * (1.0 - frac) + onsets[i + whole + 1] * frac))
            .sum()
    };
    let mut best = (120.0, f64::MIN);
    let mut bpm = 70.0;
    while bpm <= 180.0 {
        // A tempo whose half is as strong is counted with it, so the beat
        // is not taken for its off-beat.
        let s = score(lag(bpm)) + 0.5 * score(lag(bpm) * 2.0);
        if s > best.1 {
            best = (bpm, s);
        }
        bpm += 0.25;
    }
    let period = lag(best.0);
    let mut phase = (0.0, f64::MIN);
    let steps = period.ceil() as usize;
    for p in 0..steps.max(1) {
        let mut sum = 0.0;
        let mut t = p as f64;
        while (t as usize) < onsets.len() {
            sum += onsets[t as usize];
            t += period;
        }
        if sum > phase.1 {
            phase = (p as f64, sum);
        }
    }
    Ok(Beats {
        bpm: best.0,
        offset: phase.0 / per_second,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/vary/timing.rs"]
mod tests;
