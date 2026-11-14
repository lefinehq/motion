//! Unit tests for `src/compile.rs`.

use super::*;
use crate::text::ApproxTextMeasure;
use rstest::rstest;

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

/// What drives the property or component called `name` on `node`: its
/// track, and the component of the track's values that is `name`.
struct Driven<'a> {
    track: &'a Track,
    component: usize,
}

impl Driven<'_> {
    /// The value at the node's own `frame`.
    fn sample(&self, frame: f64) -> f64 {
        self.track.sample(frame).components()[self.component]
    }

    /// The keys as `(frame, value)` pairs.
    fn pairs(&self) -> Vec<(f64, f64)> {
        let i = self.component;
        self.track
            .keys()
            .iter()
            .map(|k| (k.frame, k.value.components()[i]))
            .collect()
    }

    fn is_constant(&self) -> bool {
        self.track.is_constant()
    }
}

/// The track of `node` that drives the target called `name`.
fn track<'a>(node: &'a Node, name: &str) -> Driven<'a> {
    let target = Target::named(name).unwrap();
    match (&node.tracks[target.prop.index()], target.component) {
        (PropertyTrack::Whole(track), component) => Driven {
            track,
            component: component.unwrap_or(0),
        },
        (PropertyTrack::Split(tracks), Some(i)) => Driven {
            track: &tracks[i],
            component: 0,
        },
        (PropertyTrack::Split(_), None) => panic!("`{name}` is split into components"),
    }
}

/// The keys of a target as `(frame, value)` pairs.
fn pairs(driven: Driven) -> Vec<(f64, f64)> {
    driven.pairs()
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
        assert_eq!(n.tracks.len(), motion_format::property::PROPERTIES.len());
    }
}

#[test]
fn untouched_properties_are_constant_at_their_default() {
    let g = one("", "");
    for (spec, t) in motion_format::property::PROPERTIES
        .iter()
        .zip(&g.nodes[0].tracks)
    {
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

/// y grows downwards: fade-up starts below (+y), fade-down above (-y),
/// fade-left right of (+x), fade-right left of (-x).
#[rstest]
#[case("fade-up", 100.0, 240.0)]
#[case("fade-down", 100.0, 160.0)]
#[case("fade-left", 140.0, 200.0)]
#[case("fade-right", 60.0, 200.0)]
#[case("fade", 100.0, 200.0)]
fn offsets_point_away_from_the_direction_of_travel(
    #[case] preset: &str,
    #[case] from_x: f64,
    #[case] from_y: f64,
) {
    let n = &one(&format!(r#","in":"{preset}""#), "").nodes[0];
    assert_eq!(track(n, "x").sample(0.0), from_x, "x");
    assert_eq!(track(n, "y").sample(0.0), from_y, "y");
    assert_eq!(track(n, "x").sample(1e6), 100.0, "x end");
    assert_eq!(track(n, "y").sample(1e6), 200.0, "y end");
    assert_eq!(track(n, "opacity").sample(0.0), 0.0);
}

#[test]
fn pop_and_spin_in_produce_scale_and_rotate_tracks() {
    let n = &one(r#","in":"pop 10f linear""#, "").nodes[0];
    assert_eq!(pairs(track(n, "scale-x")), [(0.0, 0.6), (10.0, 1.0)]);
    assert_eq!(pairs(track(n, "scale-y")), [(0.0, 0.6), (10.0, 1.0)]);
    assert!(track(n, "rotate").is_constant());
    let n = &one(r#","in":"spin-in 10f linear""#, "").nodes[0];
    assert_eq!(pairs(track(n, "rotate")), [(0.0, -90.0), (10.0, 0.0)]);
    assert!(track(n, "scale").is_constant());
}

#[test]
fn seconds_convert_to_frames_at_a_non_default_fps() {
    let n = &one(r#","in":"fade-up 40px 0.25s @0.5s""#, r#""fps":60,"#).nodes[0];
    // 0.5 s and 0.25 s at 60 fps.
    let frames: Vec<f64> = track(n, "opacity").pairs().iter().map(|k| k.0).collect();
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
    assert_eq!(pairs(track(n, "scale-y")), [(0.0, 0.2), (10.0, 0.8)]);
    // The entrance still drives opacity and y.
    assert!(!track(n, "opacity").is_constant());
    assert!(!track(n, "y").is_constant());
}

/// Moving `at` moves the motion on an axis unless `anim` drives it or the
/// entrance starts it at an absolute value; a relative start moves with `at`.
#[rstest]
#[case::anim_x(r#","anim":{"x":"0 -> 50 @0-10f"}"#, (false, true))]
#[case::anim_y(r#","anim":{"y":"0 -> 50 @0-10f"}"#, (true, false))]
#[case::anim_off_the_axes(r#","anim":{"opacity":"0 -> 1 @0-10f"}"#, (true, true))]
#[case::absolute_start(r#","in":"swoop""#, (false, true))]
#[case::relative_start(r#","in":"drop""#, (true, true))]
fn follows_at_per_axis(#[case] extra: &str, #[case] follows: (bool, bool)) {
    let top = r#""presets":{"swoop":{"x":1900},"drop":{"y":"-d"}},"#;
    assert_eq!(one(extra, top).nodes[0].follows_at, follows);
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
fn resting_values_set_on_the_layer_are_the_constant_tracks() {
    let n = &one(
        r#","rotate":45,"scale":[0.8,0.5],"anchor":[0,0],"opacity":0.5"#,
        "",
    )
    .nodes[0];
    assert_eq!(track(n, "rotate").sample(0.0), 45.0);
    assert_eq!(track(n, "scale-y").sample(0.0), 0.5);
    assert_eq!(track(n, "anchor-x").sample(0.0), 0.0);
    assert_eq!(track(n, "opacity").sample(0.0), 0.5);
    // Without it the anchor rests at the centre of the box.
    assert_eq!(track(&one("", "").nodes[0], "anchor-y").sample(0.0), 5.0);
}

#[test]
fn a_whole_point_track_keeps_its_path_and_speed() {
    let n = &one(
        r#","anim":{"position":[{"t":0,"v":[0,0],"curve":"speed(0,0.5,0,0.5)","path":[[50,0],[0,-50]]},{"t":"1s","v":[100,100]}]}"#,
        r#""fps":10,"#,
    )
    .nodes[0];
    let PropertyTrack::Whole(position) = &n.tracks[Prop::POSITION.index()] else {
        panic!("position is animated whole");
    };
    assert_eq!(position.value_type(), ValueType::Point);
    let [x, y] = position.sample(5.0).as_point().unwrap();
    // The bent path passes above the straight diagonal.
    assert!(x > y, "{x},{y}");
    assert_eq!(n.follows_at, (false, false));
}

#[test]
fn components_split_a_point_and_rest_where_nothing_drives_them() {
    let n = &one(r#","anim":{"scale-x":"0 -> 2 @0-10f"}"#, "").nodes[0];
    let PropertyTrack::Split(tracks) = &n.tracks[Prop::SCALE.index()] else {
        panic!("scale is split");
    };
    assert_eq!(tracks.len(), 2);
    assert_eq!(track(n, "scale-x").sample(5.0), 1.0);
    assert!(track(n, "scale-y").is_constant());
    assert_eq!(track(n, "scale-y").sample(5.0), 1.0);
}

#[test]
fn parents_and_timing_are_resolved() {
    let g = compile_json(
        r#"{"motion":1,"fps":10,"dur":"2s","layers":[
        {"id":"card","rect":"10x10"},
        {"id":"label","rect":"5x5","parent":"card","span":"1s-2s","offset":"1s","stretch":2}]}"#,
    );
    assert_eq!(g.nodes[0].parent, None);
    assert_eq!(g.nodes[0].timing, Timing::ALWAYS);
    let label = &g.nodes[1];
    assert_eq!(label.parent, Some(0));
    assert_eq!(
        label.timing,
        Timing {
            span: Some((10.0, 20.0)),
            offset: 10.0,
            stretch: 2.0
        }
    );
}

#[test]
#[should_panic(expected = "its own ancestor")]
fn a_code_built_parent_cycle_is_refused() {
    let mut scene = motion_format::parse_document(
        r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1"},{"id":"b","rect":"1x1","parent":"a"}]}"#,
    )
    .unwrap()
    .scene()
    .clone();
    scene.layers[0].parent = Some("b".into());
    compile(&scene, &ApproxTextMeasure);
}

#[test]
fn at_resolves_in_the_parents_box() {
    let g = compile_json(
        r#"{"motion":1,"dur":"1s","layers":[{"id":"card","rect":"200x100"},
        {"id":"kid","rect":"10x10","parent":"card","at":"center center"}]}"#,
    );
    assert_eq!(track(&g.nodes[1], "x").sample(0.0), 100.0);
    assert_eq!(track(&g.nodes[1], "y").sample(0.0), 50.0);
}
