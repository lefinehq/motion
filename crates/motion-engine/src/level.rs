//! Measuring the levels a scene's listening tracks follow (step `listen`):
//! for each hearing ([`motion_scene::Hearing`]), the sound of the layer it
//! names, mixed once as it plays in its list, then measured over a frame's
//! window centred on each frame of the list's clock by
//! [`motion_raster::level`] and smoothed. The routes come from a first compile: nothing a level drives
//! changes what plays, since `volume` may not listen.

use std::sync::Arc;

use motion_audio::{MIX_RATE, Sound};
use motion_format::Result;
use motion_scene::{CompGraph, Hearing, Levels, Route, SceneGraph};

use crate::mix::{Canvas, mix_in};

/// The levels of every hearing of `hearings` in `graph`, reading each
/// route's file through `sound`.
///
/// # Errors
/// A sound file that cannot be read or decoded.
pub(crate) fn levels(
    graph: &SceneGraph,
    hearings: &[Hearing],
    sound: impl Fn(&Route) -> Result<Arc<Sound>>,
) -> Result<Levels> {
    let mut levels = Levels::none();
    for hearing in hearings {
        if let Some(measured) = measure(graph, hearing, &sound)? {
            levels.insert(hearing.clone(), measured);
        }
    }
    Ok(levels)
}

/// The level of `hearing` at every frame of its list's clock, from 0 to the
/// list's last frame, or `None` when its list or layer is not in `graph`.
fn measure(
    graph: &SceneGraph,
    hearing: &Hearing,
    sound: &impl Fn(&Route) -> Result<Arc<Sound>>,
) -> Result<Option<Vec<f64>>> {
    let list: Option<&CompGraph> = std::iter::once(&graph.root)
        .chain(&graph.comps)
        .find(|comp| comp.id == hearing.comp);
    let Some(list) = list else {
        return Ok(None);
    };
    let Some(index) = list.nodes.iter().position(|n| n.id == hearing.layer) else {
        return Ok(None);
    };
    let routes = motion_scene::routes_from(graph, &list.nodes, index);
    let canvas = || Canvas {
        nodes: &list.nodes,
        fps: list.fps,
    };
    let frames = list.duration_frames.ceil().max(0.0) as usize + 1;
    // A one-pole filter: each frame closes this share of the gap.
    let smooth = hearing.smooth();
    let step = match smooth > 0.0 {
        true => 1.0 - (-1.0 / (list.fps * smooth)).exp(),
        false => 1.0,
    };
    // One mix over the whole clock, half a frame either side, then each
    // frame's window cut from it: the routes are read once, not once a
    // frame.
    let start = -0.5;
    let mixed = mix_in(graph, canvas(), &routes, start..frames as f64 - 0.5, sound)?;
    let mono: Vec<f32> = match mixed.channels() {
        [left, right] => left.iter().zip(right).map(|(l, r)| (l + r) / 2.0).collect(),
        channels => channels.first().cloned().unwrap_or_default(),
    };
    let per_frame = f64::from(MIX_RATE) / list.fps;
    let mut held = 0.0;
    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let from = ((frame as f64 * per_frame).round() as usize).min(mono.len());
        let to = (((frame + 1) as f64 * per_frame).round() as usize).min(mono.len());
        let level = motion_raster::level(&mono[from..to], f64::from(MIX_RATE), hearing.band());
        held += (level - held) * step;
        out.push(held.clamp(0.0, 1.0));
    }
    Ok(Some(out))
}

#[cfg(test)]
#[path = "../tests/unit/level.rs"]
mod tests;
