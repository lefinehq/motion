//! Unit tests for `src/level.rs`: what listening tracks hear, and the keys
//! their levels become.

use motion_format::property::Prop;
use pretty_assertions::assert_eq;

use super::*;
use crate::{SceneGraph, compile_with, evaluate};

/// A 10 fps scene: music from 0.5 s; a box shown from 0.2 s at half speed,
/// whose opacity follows the music's bass and its scale the whole music;
/// and in a composition, a dot whose fill follows a kick.
const SCENE: &str = r##"{"motion":1,"fps":10,"dur":"1s",
  "comps":{"beat":{"size":"10x10","dur":"1s","layers":[
    {"id":"kick","audio":"kick.wav"},
    {"id":"dot","rect":"4x4","fx":[{"fill":"#FF0000","anim":{"opacity":"0 -> 1 @level kick"}}]}]}},
  "layers":[
    {"id":"music","audio":"music.mp3","offset":"0.5s"},
    {"id":"box","rect":"10x10","offset":"0.2s","stretch":2,
     "anim":{"opacity":"0.2 -> 1 @level music 30-150Hz smooth 0.1s",
             "scale":"rest -> rest*2 @level music out-quad"}},
    {"id":"b","comp":"beat"}]}"##;

fn scene() -> motion_format::Scene {
    motion_format::parse_document(SCENE)
        .unwrap()
        .scene()
        .clone()
}

/// `scene` compiled with `levels`.
fn graph(levels: &Levels) -> SceneGraph {
    compile_with(
        &scene(),
        &mut motion_text::Typesetter::new(motion_text::FontSet::builtin()),
        levels,
    )
    .unwrap()
}

/// The hearing of `layer` in `comp`, at 10 fps, as `SCENE` writes it.
fn hearing(comp: &str, layer: &str, band: Option<(f64, f64)>, smooth: f64) -> Hearing {
    let listen = motion_format::anim::Listen {
        layer: layer.into(),
        band,
        smooth: motion_format::units::Time::Seconds(smooth),
        curve: motion_format::Curve::Linear,
        to: motion_format::anim::KeyValue::Set(motion_format::Value::Number(0.0)),
    };
    Hearing::of(comp, &listen, 10.0)
}

#[test]
fn every_hearing_is_found_once_with_its_list_band_and_smoothing() {
    let root = scene().root.id.clone();
    let found = hearings(&scene());
    assert_eq!(
        found,
        [
            hearing(&root, "music", Some((30.0, 150.0)), 0.1),
            hearing(&root, "music", None, 0.0),
            hearing("beat", "kick", None, 0.0),
        ]
    );
    assert_eq!(found[0].band(), Some((30.0, 150.0)));
    assert!((found[0].smooth() - 0.1).abs() < 1e-12);
}

#[test]
fn without_levels_a_listening_track_holds_its_value_at_silence() {
    let state = evaluate(&graph(&Levels::none()), 8.0);
    let values = &state.nodes[1].values;
    assert_eq!(values[Prop::OPACITY.index()].as_number(), Some(0.2));
    assert_eq!(values[Prop::SCALE.index()].as_point(), Some([1.0, 1.0]));
}

#[test]
fn levels_become_a_key_every_frame_of_the_lists_clock_at_the_layers_time() {
    let root = scene().root.id.clone();
    let mut levels = Levels::none();
    // One a frame of the root's clock, frames 0 to 10.
    let rising: Vec<f64> = (0..=10).map(|f| f64::from(f) / 10.0).collect();
    levels.insert(
        hearing(&root, "music", Some((30.0, 150.0)), 0.1),
        rising.clone(),
    );
    levels.insert(hearing(&root, "music", None, 0.0), rising);
    let g = graph(&levels);
    // The box's own time runs from 0.2 s at half speed, so the list's frame
    // 6 is its own frame 2; its tracks read the list's frame 6 there.
    let at = |frame: f64, prop: Prop| evaluate(&g, frame).nodes[1].values[prop.index()].clone();
    let opacity = |frame| at(frame, Prop::OPACITY).as_number().unwrap();
    assert!(
        (opacity(6.0) - (0.2 + 0.8 * 0.6)).abs() < 1e-9,
        "{}",
        opacity(6.0)
    );
    // Between frames, linear.
    assert!((opacity(6.5) - (0.2 + 0.8 * 0.65)).abs() < 1e-9);
    // `out-quad` lifts the half level to three quarters of the way.
    let [x, y] = at(5.0, Prop::SCALE).as_point().unwrap();
    assert!(
        (x - 1.75).abs() < 1e-9 && (y - 1.75).abs() < 1e-9,
        "{x} {y}"
    );
}

#[test]
fn a_level_above_one_counts_as_full() {
    let root = scene().root.id.clone();
    let mut levels = Levels::none();
    levels.insert(
        hearing(&root, "music", Some((30.0, 150.0)), 0.1),
        vec![3.0; 11],
    );
    let state = evaluate(&graph(&levels), 4.0);
    assert_eq!(
        state.nodes[1].values[Prop::OPACITY.index()].as_number(),
        Some(1.0)
    );
}
