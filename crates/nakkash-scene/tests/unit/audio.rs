//! Unit tests for `src/audio.rs`: routes reach audio layers through
//! compositions, and say what each plays at a frame.

use pretty_assertions::assert_eq;

use super::*;

/// The compiled scene `json`.
fn graph(json: &str) -> SceneGraph {
    crate::compile(
        nakkash_format::parse_document(json).unwrap().scene(),
        &mut nakkash_text::Typesetter::new(nakkash_text::FontSet::builtin()),
    )
    .unwrap()
}

/// A 10 fps scene: music from 1 s, faded in over its first second; a
/// jingle inside a composition shown from 2 s at half speed and half
/// volume; and a hidden layer.
const SCENE: &str = r#"{"nakkash":1,"fps":10,"dur":"5s",
  "comps":{"jingle":{"size":"10x10","fps":20,"dur":"1s","layers":[
    {"id":"blip","audio":"blip.wav","volume":0.5}]}},
  "layers":[
    {"id":"music","audio":"music.mp3","offset":"1s","anim":{"volume":"0 -> 1 @0-10f"}},
    {"id":"j","comp":"jingle","offset":"2s","stretch":2,"volume":0.5},
    {"id":"muted","audio":"music.mp3","hidden":true},
    {"id":"box","rect":"10x10"}]}"#;

#[test]
fn every_audio_layer_has_a_route_through_its_compositions() {
    let graph = graph(SCENE);
    let routes = routes(&graph);
    let found: Vec<(&str, &str)> = routes.iter().map(|r| (r.id(&graph), r.path())).collect();
    assert_eq!(
        found,
        [
            ("music", "music.mp3"),
            ("blip", "blip.wav"),
            ("muted", "music.mp3")
        ]
    );
}

#[test]
fn a_layer_plays_its_own_time_at_its_volume() {
    let graph = graph(SCENE);
    let music = &routes(&graph)[0];
    // Its own time is 0.5 s at 1.5 s, halfway through its fade.
    assert_eq!(
        music.at(&graph, 15.0),
        Some(Playing {
            seconds: 0.5,
            gain: 0.5,
            speed: 1.0
        })
    );
    // Before its offset, its own time is negative: silence in the file.
    assert_eq!(music.at(&graph, 5.0).map(|p| p.seconds), Some(-0.5));
}

#[test]
fn a_composition_passes_on_its_time_speed_and_volume() {
    let graph = graph(SCENE);
    let blip = &routes(&graph)[1];
    // At 3 s, the comp layer's own time is 0.5 s at half speed; the
    // composition runs at 20 fps, so its frame is 10, and the blip is
    // 0.5 s in, at both volumes.
    assert_eq!(
        blip.at(&graph, 30.0),
        Some(Playing {
            seconds: 0.5,
            gain: 0.25,
            speed: 0.5
        })
    );
    // Before the comp layer's offset, and once past the composition's
    // length (1 s of its own time, 2 s of the scene's), it is silent.
    assert_eq!(blip.at(&graph, 19.0), None);
    assert_eq!(blip.at(&graph, 40.0), None);
}

#[test]
fn a_hidden_layer_is_silent() {
    let graph = graph(SCENE);
    assert_eq!(routes(&graph)[2].at(&graph, 10.0), None);
}

#[test]
fn a_remapped_composition_plays_the_time_it_remaps_to_at_its_speed() {
    // The jingle's layer plays its second backwards, from 1 s to 0, over
    // 2 s of its own time, which runs at half speed.
    let graph = graph(&SCENE.replace(
        r#""volume":0.5},"#,
        r#""volume":0.5,"anim":{"remap":"1 -> 0 @0-20f linear"}},"#,
    ));
    let blip = &routes(&graph)[1];
    // At 3 s, the layer's own time is 0.5 s, a quarter of the way back:
    // 0.75 s into the jingle, read backwards at a quarter of the speed.
    let playing = blip.at(&graph, 30.0).expect("it plays");
    assert!((playing.seconds - 0.75).abs() < 1e-9, "{}", playing.seconds);
    assert!((playing.speed + 0.25).abs() < 1e-9, "{}", playing.speed);
}
