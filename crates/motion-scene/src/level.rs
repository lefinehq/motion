//! Tracks that follow a sound's level ([`Listen`]): what each hears, the
//! levels the engine measures for them, and the keys they become.
//!
//! This crate reads no sound. A listening track compiles to its value at
//! silence unless [`compile_with`](crate::compile_with) is given its levels:
//! one a frame of its list's clock, which the engine measures from a first
//! compile's sound routes. Each level becomes a key on that frame, linear to
//! the next, so any frame, fractional ones too, evaluates alone.

use std::collections::HashMap;

use motion_format::anim::{AnimTrack, Listen};
use motion_format::{Content, Item, Layer, Scene, Value};

use crate::curve::Ease;
use crate::graph::Timing;
use crate::track::KeySpec;

/// What one listening track hears: the list it sits in, by its
/// composition's id, the layer of that list whose sound it follows, the
/// band and how it smooths. Tracks that hear the same share their levels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hearing {
    /// The id of the composition whose list holds the track and the layer.
    pub comp: String,
    /// The audio or `comp` layer it follows.
    pub layer: String,
    band: Option<(u64, u64)>,
    smooth: u64,
}

impl Hearing {
    /// What `listen` hears in the list of the composition `comp`, whose
    /// clock runs at `fps`.
    pub fn of(comp: &str, listen: &Listen, fps: f64) -> Hearing {
        Hearing {
            comp: comp.to_owned(),
            layer: listen.layer.clone(),
            band: listen
                .band
                .map(|(low, high)| (low.to_bits(), high.to_bits())),
            smooth: (listen.smooth.to_frames(fps) / fps).to_bits(),
        }
    }

    /// The frequencies it hears, from and to, in Hz; the whole sound
    /// without them.
    pub fn band(&self) -> Option<(f64, f64)> {
        self.band
            .map(|(low, high)| (f64::from_bits(low), f64::from_bits(high)))
    }

    /// The time constant its level rises and falls with, in seconds.
    pub fn smooth(&self) -> f64 {
        f64::from_bits(self.smooth)
    }
}

/// Every hearing of `scene`'s listening tracks, each once: on layers and
/// on their items, however deep, in the root and every composition.
pub fn hearings(scene: &Scene) -> Vec<Hearing> {
    let mut all: Vec<Hearing> = Vec::new();
    for comp in std::iter::once(&scene.root).chain(&scene.comps) {
        for listen in comp.layers.iter().flat_map(listens) {
            let hearing = Hearing::of(&comp.id, listen, comp.fps);
            if !all.contains(&hearing) {
                all.push(hearing);
            }
        }
    }
    all
}

/// What the tracks of `layer` and of its items listen to.
fn listens(layer: &Layer) -> Vec<&Listen> {
    let mut tracks: Vec<&AnimTrack> = layer.anim.iter().collect();
    let mut lists: Vec<&[Item]> = vec![&layer.fx, &layer.mask];
    match &layer.content {
        Content::Shapes(items) => lists.push(items),
        Content::Text(text) => lists.push(&text.animators),
        _ => {}
    }
    while let Some(items) = lists.pop() {
        for item in items {
            tracks.extend(&item.anim);
            lists.push(&item.items);
        }
    }
    tracks.iter().filter_map(|t| t.listen.as_ref()).collect()
}

/// The level each hearing measured, one a frame of its list's clock from
/// frame 0, each from 0 (silence) to 1 (full level).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Levels {
    by: HashMap<Hearing, Vec<f64>>,
}

impl Levels {
    /// No levels: every listening track holds its value at silence.
    pub fn none() -> Levels {
        Levels::default()
    }

    /// Records the levels `hearing` measured.
    pub fn insert(&mut self, hearing: Hearing, levels: Vec<f64>) {
        self.by.insert(hearing, levels);
    }

    /// The levels `hearing` measured, if any.
    pub fn get(&self, hearing: &Hearing) -> Option<&[f64]> {
        self.by.get(hearing).map(Vec::as_slice)
    }
}

/// Where a listening track's levels are and how its list's clock maps to
/// its own: the levels, and the composition and the timing of the layer
/// that holds it.
#[derive(Clone, Copy)]
pub(crate) struct Ear<'a> {
    pub(crate) levels: &'a Levels,
    pub(crate) comp: &'a str,
    pub(crate) fps: f64,
    pub(crate) timing: Timing,
}

impl Ear<'_> {
    /// The keys of `track`, which listens to `listen`, with `from` and `to`
    /// its values at silence and at full level: one a frame of the list's
    /// clock, at the layer's own time of that frame. `None` when no levels
    /// were measured for it.
    pub(crate) fn keys(&self, listen: &Listen, from: &Value, to: &Value) -> Option<Vec<KeySpec>> {
        let levels = self.levels.get(&Hearing::of(self.comp, listen, self.fps))?;
        let ease = Ease::from_curve(listen.curve, 1.0, 1.0);
        let (a, b) = (from.components(), to.components());
        let keys = levels
            .iter()
            .enumerate()
            .map(|(frame, level)| {
                let p = ease.sample(0.0, 1.0, level.clamp(0.0, 1.0));
                let mixed: Vec<f64> = a.iter().zip(b).map(|(a, b)| a + (b - a) * p).collect();
                KeySpec::linear(
                    self.timing.local(frame as f64),
                    from.with_components(&mixed),
                )
            })
            .collect::<Vec<_>>();
        (!keys.is_empty()).then_some(keys)
    }
}

#[cfg(test)]
#[path = "../tests/unit/level.rs"]
mod tests;
