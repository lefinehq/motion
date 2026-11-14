//! Unit tests for `src/compile.rs`.

use super::*;
use crate::text::ApproxTextMeasure;

fn launch() -> motion_format::Scene {
    motion_format::parse_document(include_str!("../../../../examples/launch.json"))
        .unwrap()
        .scene()
        .clone()
}

fn compile_json(json: &str) -> SceneGraph {
    let scene = motion_format::parse_document(json).unwrap().scene().clone();
    compile(&scene, &ApproxTextMeasure)
}

/// The track of `node` for the property called `name`.
fn track<'a>(node: &'a Node, name: &str) -> &'a Track {
    &node.tracks[Prop::named(name).unwrap().index()]
}

/// The keys of a track as `(frame, value)` pairs.
fn pairs(t: &Track) -> Vec<(f64, f64)> {
    t.keys().iter().map(|k| (k.frame, k.value)).collect()
}

/// One layer `a` (10x10 at 100,200) with the given extra layer keys.
fn one(extra: &str, top: &str) -> SceneGraph {
    compile_json(&format!(
        r#"{{"motion":1,{top}"dur":"2s","layers":[{{"id":"a","rect":"10x10","at":"100 200"{extra}}}]}}"#
    ))
}

#[test]
fn resolves_sizes_and_rest_positions() {
    let g = compile(&launch(), &ApproxTextMeasure);
    assert_eq!(
        (g.width, g.height, g.duration_frames),
        (1920.0, 1080.0, 150.0)
    );
    let t = &g.nodes[0];
    assert_eq!(track(t, "x").sample(0.0), 960.0);
    assert_eq!(track(t, "y").sample(1e6), 260.0);
    assert!((t.width - 580.8).abs() < 1e-9 && (t.height - 115.2).abs() < 1e-9);
    let c = &g.nodes[1];
    assert_eq!((c.width, c.height), (300.0, 400.0));
    assert_eq!(track(c, "x").sample(0.0), 960.0);
}

#[test]
fn every_node_has_one_track_per_property() {
    for n in &compile(&launch(), &ApproxTextMeasure).nodes {
        assert_eq!(n.tracks.len(), PROPERTIES.len());
    }
}

#[test]
fn untouched_properties_are_constant_at_their_default() {
    let g = one("", "");
    for (spec, t) in PROPERTIES.iter().zip(&g.nodes[0].tracks) {
        assert!(t.is_constant(), "{}", spec.name);
    }
    assert_eq!(track(&g.nodes[0], "opacity").sample(0.0), 1.0);
    assert_eq!(track(&g.nodes[0], "scale").sample(0.0), 1.0);
    assert_eq!(track(&g.nodes[0], "x").sample(0.0), 100.0);
    assert_eq!(g.nodes[0].follows_at, (true, true));
}

#[test]
fn fade_up_compiles_to_an_opacity_and_a_y_track() {
    let g = compile(&launch(), &ApproxTextMeasure);
    let c = &g.nodes[1];
    assert_eq!(pairs(track(c, "opacity")), [(15.0, 0.0), (27.0, 1.0)]);
    assert_eq!(pairs(track(c, "y")), [(15.0, 760.0), (27.0, 700.0)]);
    assert!(track(c, "x").is_constant());
}

#[test]
fn offsets_point_away_from_the_direction_of_travel() {
    // y grows downwards: fade-up starts below (+y), fade-down above (-y),
    // fade-left right of (+x), fade-right left of (-x).
    for (preset, from_x, from_y) in [
        ("fade-up", 100.0, 240.0),
        ("fade-down", 100.0, 160.0),
        ("fade-left", 140.0, 200.0),
        ("fade-right", 60.0, 200.0),
        ("fade", 100.0, 200.0),
    ] {
        let n = &one(&format!(r#","in":"{preset}""#), "").nodes[0];
        assert_eq!(track(n, "x").sample(0.0), from_x, "{preset} x");
        assert_eq!(track(n, "y").sample(0.0), from_y, "{preset} y");
        assert_eq!(track(n, "x").sample(1e6), 100.0, "{preset} x end");
        assert_eq!(track(n, "y").sample(1e6), 200.0, "{preset} y end");
        assert_eq!(track(n, "opacity").sample(0.0), 0.0, "{preset}");
    }
}

#[test]
fn pop_and_spin_in_produce_scale_and_rotate_tracks() {
    let n = &one(r#","in":"pop 10f linear""#, "").nodes[0];
    assert_eq!(pairs(track(n, "scale")), [(0.0, 0.6), (10.0, 1.0)]);
    assert!(track(n, "rotate").is_constant());
    let n = &one(r#","in":"spin-in 10f linear""#, "").nodes[0];
    assert_eq!(pairs(track(n, "rotate")), [(0.0, -90.0), (10.0, 0.0)]);
    assert!(track(n, "scale").is_constant());
}

#[test]
fn seconds_convert_to_frames_at_a_non_default_fps() {
    let n = &one(r#","in":"fade-up 40px 0.25s @0.5s""#, r#""fps":60,"#).nodes[0];
    // 0.5 s and 0.25 s at 60 fps.
    let frames: Vec<f64> = track(n, "opacity").keys().iter().map(|k| k.frame).collect();
    assert_eq!(frames, [30.0, 45.0]);
}

#[test]
fn seconds_based_anim_keys_convert_at_60_fps() {
    let g = one(
        r#","anim":{"rotate":[["0.5s",0],["1s",90,"ease-out"]]}"#,
        r#""fps":60,"#,
    );
    let n = &g.nodes[0];
    assert_eq!(pairs(track(n, "rotate")), [(30.0, 0.0), (60.0, 90.0)]);
    assert_eq!(g.fps, 60.0);
}

#[test]
fn anim_and_entrance_drive_different_properties() {
    let n = &one(
        r#","in":"fade-up","anim":{"scale":"0.2 -> 0.8 @0-10f"}"#,
        "",
    )
    .nodes[0];
    assert_eq!(pairs(track(n, "scale")), [(0.0, 0.2), (10.0, 0.8)]);
    // The entrance still drives opacity and y.
    assert!(!track(n, "opacity").is_constant());
    assert!(!track(n, "y").is_constant());
}

#[test]
fn follows_at_is_false_for_an_axis_that_anim_drives() {
    let n = &one(r#","anim":{"x":"0 -> 50 @0-10f"}"#, "").nodes[0];
    assert_eq!(n.follows_at, (false, true));
    let n = &one(r#","anim":{"y":"0 -> 50 @0-10f"}"#, "").nodes[0];
    assert_eq!(n.follows_at, (true, false));
    let n = &one(r#","anim":{"opacity":"0 -> 1 @0-10f"}"#, "").nodes[0];
    assert_eq!(n.follows_at, (true, true));
}

#[test]
fn a_scene_preset_overrides_a_built_in() {
    // The built-in `fade` has no y; this one slides in from below.
    let g = one(
        r#","in":"fade 30px""#,
        r#""presets":{"fade":{"opacity":0,"y":"+d"}},"#,
    );
    let n = &g.nodes[0];
    assert_eq!(pairs(track(n, "y")).first(), Some(&(0.0, 230.0)));
    assert!(!track(n, "y").is_constant());
}

#[test]
fn follows_at_is_false_for_an_axis_with_an_absolute_preset_start() {
    let top = r#""presets":{"swoop":{"x":1900},"drop":{"y":"-d"}},"#;
    let n = &one(r#","in":"swoop""#, top).nodes[0];
    assert_eq!(n.follows_at, (false, true));
    // A relative start moves with `at`.
    let n = &one(r#","in":"drop""#, top).nodes[0];
    assert_eq!(n.follows_at, (true, true));
}
