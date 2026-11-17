//! The scene's sound as one stereo track at the mix rate (step `mix`).
//!
//! Each route to an audio layer ([`nakkash_scene::Route`]) is asked what it
//! plays once a millisecond, a control block; within a block, the position
//! in its file and its gain move in a straight line, so fades have no
//! steps and a layer plays at its own speed. A block where the layer starts
//! or stops is asked sample by sample, so it starts on its sample. Layers
//! are summed with no limiter.

use std::ops::Range;

use nakkash_audio::{Kernel, MIX_RATE, Sound, Speaker};
use nakkash_format::Result;
use nakkash_scene::{Playing, Route, SceneGraph};

/// Samples a control block holds: one millisecond.
const BLOCK: usize = MIX_RATE as usize / 1000;

/// How close to a whole sample a read must be to take that sample as it is.
const ON_A_SAMPLE: f64 = 1e-6;

/// The mix of `routes` of `graph` over the scene's `frames`, each route's
/// sound found by `sound`: stereo at [`MIX_RATE`].
///
/// # Errors
/// The first route's sound that cannot be read.
pub(crate) fn mix(
    graph: &SceneGraph,
    routes: &[Route],
    frames: Range<f64>,
    sound: impl Fn(&Route) -> Result<std::sync::Arc<Sound>>,
) -> Result<Sound> {
    mix_in(graph, Canvas::root(graph), routes, frames, sound)
}

/// A canvas routes start from: its nodes and its clock.
#[derive(Clone, Copy)]
pub(crate) struct Canvas<'g> {
    /// Its nodes.
    pub(crate) nodes: &'g [nakkash_scene::Node],
    /// Its frames a second.
    pub(crate) fps: f64,
}

impl<'g> Canvas<'g> {
    /// The scene's own canvas.
    pub(crate) fn root(graph: &'g SceneGraph) -> Canvas<'g> {
        Canvas {
            nodes: &graph.root.nodes,
            fps: graph.root.fps,
        }
    }
}

/// The mix of `routes`, which start at `canvas`, over its `frames`.
///
/// # Errors
/// The first route's sound that cannot be read.
pub(crate) fn mix_in(
    graph: &SceneGraph,
    canvas: Canvas<'_>,
    routes: &[Route],
    frames: Range<f64>,
    sound: impl Fn(&Route) -> Result<std::sync::Arc<Sound>>,
) -> Result<Sound> {
    let fps = canvas.fps;
    let start = frames.start / fps;
    let length = ((frames.end - frames.start).max(0.0) / fps * f64::from(MIX_RATE)).ceil() as usize;
    let mut left = vec![0.0_f32; length];
    let mut right = vec![0.0_f32; length];
    let kernel = Kernel::standard();
    // The scene's frame at output sample `n`.
    let frame_at = |n: usize| (start + n as f64 / f64::from(MIX_RATE)) * fps;
    for route in routes {
        let sound = sound(route)?;
        let [file_left, file_right] = sound.channels() else {
            continue;
        };
        let mut play = |n: usize, playing: Playing| {
            if playing.gain == 0.0 {
                return;
            }
            let position = playing.seconds * f64::from(MIX_RATE);
            let read = |channel: &[f32]| {
                let nearest = position.round();
                match playing.speed == 1.0 && (position - nearest).abs() < ON_A_SAMPLE {
                    true => usize::try_from(nearest as i64)
                        .ok()
                        .and_then(|i| channel.get(i).copied())
                        .unwrap_or(0.0),
                    false => kernel.read(channel, position, playing.speed),
                }
            };
            let gain = playing.gain as f32;
            left[n] += read(file_left) * gain;
            right[n] += read(file_right) * gain;
        };
        for block in (0..length).step_by(BLOCK) {
            let end = (block + BLOCK).min(length);
            let at = |frame: f64| route.at_in(graph, canvas.nodes, fps, frame);
            let first = at(frame_at(block));
            let last = at(frame_at(end));
            match (first, last) {
                (None, None) => {}
                (Some(a), Some(b)) => {
                    for n in block..end {
                        let s = (n - block) as f64 / (end - block) as f64;
                        play(
                            n,
                            Playing {
                                seconds: a.seconds + (b.seconds - a.seconds) * s,
                                gain: a.gain + (b.gain - a.gain) * s,
                                speed: a.speed,
                            },
                        );
                    }
                }
                // It starts or stops inside the block: ask each sample.
                _ => {
                    for n in block..end {
                        if let Some(playing) = at(frame_at(n)) {
                            play(n, playing);
                        }
                    }
                }
            }
        }
    }
    Sound::new(
        MIX_RATE,
        vec![left, right],
        vec![Speaker::FrontLeft, Speaker::FrontRight],
    )
}

#[cfg(test)]
#[path = "../tests/unit/mix.rs"]
mod tests;
