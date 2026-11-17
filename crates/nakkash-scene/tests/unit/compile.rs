//! Unit tests for `src/compile.rs`.

use super::*;
use nakkash_text::{FontSet, Typesetter};
use rstest::rstest;

fn launch() -> nakkash_format::Scene {
    nakkash_format::parse_document(include_str!("../../../../examples/launch.json"))
        .unwrap()
        .scene()
        .clone()
}

fn compile_json(json: &str) -> SceneGraph {
    let scene = nakkash_format::parse_document(json)
        .unwrap()
        .scene()
        .clone();
    compile(&scene, &mut Typesetter::new(FontSet::builtin())).unwrap()
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
        r#"{{"nakkash":1,{top}"dur":"2s","layers":[{{"id":"a","rect":"10x10","at":"100 200"{extra}}}]}}"#
    ))
}

#[test]
fn resolves_sizes_and_rest_positions() {
    let g = compile(&launch(), &mut Typesetter::new(FontSet::builtin())).unwrap();
    assert_eq!(
        (g.root.width, g.root.height, g.root.duration_frames),
        (1920.0, 1080.0, 150.0)
    );
    let t = &g.root.nodes[0];
    assert_eq!(track(t, "x").sample(0.0), 960.0);
    assert_eq!(track(t, "y").sample(1e6), 260.0);
    // The title is set in Inter at 96 px: its box is the layout's bounds,
    // from the first baseline's start, and the anchor rests at its centre.
    assert!(
        t.bounds.x0 == 0.0 && t.bounds.y0 < 0.0 && t.bounds.width() > 400.0,
        "{:?}",
        t.bounds
    );
    let anchor = track(t, "anchor-x").sample(0.0);
    assert_eq!(anchor, t.bounds.center().x);
    let c = &g.root.nodes[1];
    assert_eq!((c.bounds.width(), c.bounds.height()), (300.0, 400.0));
    assert_eq!(track(c, "x").sample(0.0), 960.0);
}

#[test]
fn an_image_node_keeps_its_file_and_its_box_is_the_layers_box() {
    let g = compile_json(
        r#"{"nakkash":1,"dur":"1s","layers":[{"id":"photo","image":"a.png","box":[-10,0,40,20],"at":"100 50"}]}"#,
    );
    let node = &g.root.nodes[0];
    let NodeContent::Image(image) = &node.content else {
        panic!("not an image: {:?}", node.content);
    };
    assert_eq!(image.path, "a.png");
    assert_eq!(node.bounds, Rect::new(-10.0, 0.0, 30.0, 20.0));
    // The anchor rests at the box's centre, and `at` places it.
    assert_eq!(track(node, "anchor-x").sample(0.0), 10.0);
    assert_eq!(track(node, "x").sample(0.0), 100.0);
    let state = crate::evaluate(&g, 0.0);
    assert!(matches!(&state.nodes[0].content, crate::ContentState::Image(i) if i.path == "a.png"));
    assert_eq!(state.nodes[0].bounds(), Rect::new(80.0, 40.0, 120.0, 60.0));
}

#[test]
fn every_node_has_one_track_per_property() {
    for n in &compile(&launch(), &mut Typesetter::new(FontSet::builtin()))
        .unwrap()
        .root
        .nodes
    {
        assert_eq!(n.tracks.len(), nakkash_format::property::PROPERTIES.len());
    }
}

#[test]
fn untouched_properties_are_constant_at_their_default() {
    let g = one("", "");
    for (spec, t) in nakkash_format::property::PROPERTIES
        .iter()
        .zip(&g.root.nodes[0].tracks)
    {
        assert!(t.is_constant(), "{}", spec.name);
    }
    assert_eq!(track(&g.root.nodes[0], "opacity").sample(0.0), 1.0);
    assert_eq!(track(&g.root.nodes[0], "scale").sample(0.0), 1.0);
    assert_eq!(track(&g.root.nodes[0], "x").sample(0.0), 100.0);
    assert_eq!(g.root.nodes[0].follows_at, (true, true));
}

#[test]
fn fade_up_compiles_to_an_opacity_and_a_y_track() {
    let g = compile(&launch(), &mut Typesetter::new(FontSet::builtin())).unwrap();
    let c = &g.root.nodes[1];
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
    let n = &one(&format!(r#","in":"{preset}""#), "").root.nodes[0];
    assert_eq!(track(n, "x").sample(0.0), from_x, "x");
    assert_eq!(track(n, "y").sample(0.0), from_y, "y");
    assert_eq!(track(n, "x").sample(1e6), 100.0, "x end");
    assert_eq!(track(n, "y").sample(1e6), 200.0, "y end");
    assert_eq!(track(n, "opacity").sample(0.0), 0.0);
}

#[test]
fn pop_and_spin_in_produce_scale_and_rotate_tracks() {
    let n = &one(r#","in":"pop 10f linear""#, "").root.nodes[0];
    assert_eq!(pairs(track(n, "scale-x")), [(0.0, 0.6), (10.0, 1.0)]);
    assert_eq!(pairs(track(n, "scale-y")), [(0.0, 0.6), (10.0, 1.0)]);
    assert!(track(n, "rotate").is_constant());
    let n = &one(r#","in":"spin-in 10f linear""#, "").root.nodes[0];
    assert_eq!(pairs(track(n, "rotate")), [(0.0, -90.0), (10.0, 0.0)]);
    assert!(track(n, "scale").is_constant());
}

#[test]
fn seconds_convert_to_frames_at_a_non_default_fps() {
    let n = &one(r#","in":"fade-up 40px 0.25s @0.5s""#, r#""fps":60,"#)
        .root
        .nodes[0];
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
    let n = &g.root.nodes[0];
    assert_eq!(pairs(track(n, "rotate")), [(30.0, 0.0), (60.0, 90.0)]);
    assert_eq!(g.root.fps, 60.0);
}

#[test]
fn anim_and_entrance_drive_different_properties() {
    let n = &one(
        r#","in":"fade-up","anim":{"scale":"0.2 -> 0.8 @0-10f"}"#,
        "",
    )
    .root
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
    assert_eq!(one(extra, top).root.nodes[0].follows_at, follows);
}

#[test]
fn a_scene_preset_overrides_a_built_in() {
    // The built-in `fade` has no y; this one slides in from below.
    let g = one(
        r#","in":"fade 30px""#,
        r#""presets":{"fade":{"opacity":0,"y":"+d"}},"#,
    );
    let n = &g.root.nodes[0];
    assert_eq!(pairs(track(n, "y")).first(), Some(&(0.0, 230.0)));
    assert!(!track(n, "y").is_constant());
}

#[test]
fn resting_values_set_on_the_layer_are_the_constant_tracks() {
    let n = &one(
        r#","rotate":45,"scale":[0.8,0.5],"anchor":[0,0],"opacity":0.5"#,
        "",
    )
    .root
    .nodes[0];
    assert_eq!(track(n, "rotate").sample(0.0), 45.0);
    assert_eq!(track(n, "scale-y").sample(0.0), 0.5);
    assert_eq!(track(n, "anchor-x").sample(0.0), 0.0);
    assert_eq!(track(n, "opacity").sample(0.0), 0.5);
    // Without it the anchor rests at the centre of the box.
    assert_eq!(
        track(&one("", "").root.nodes[0], "anchor-y").sample(0.0),
        5.0
    );
}

#[test]
fn a_whole_point_track_keeps_its_path_and_speed() {
    let n = &one(
        r#","anim":{"position":[{"t":0,"v":[0,0],"curve":"speed(0,0.5,0,0.5)","path":[[50,0],[0,-50]]},{"t":"1s","v":[100,100]}]}"#,
        r#""fps":10,"#,
    )
    .root.nodes[0];
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
    let n = &one(r#","anim":{"scale-x":"0 -> 2 @0-10f"}"#, "").root.nodes[0];
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
        r#"{"nakkash":1,"fps":10,"dur":"2s","layers":[
        {"id":"card","rect":"10x10"},
        {"id":"label","rect":"5x5","parent":"card","span":"1s-2s","offset":"1s","stretch":2}]}"#,
    );
    assert_eq!(g.root.nodes[0].parent, None);
    assert_eq!(g.root.nodes[0].timing, Timing::ALWAYS);
    let label = &g.root.nodes[1];
    assert_eq!(label.parent, Some(0));
    assert_eq!(
        label.timing,
        Timing {
            span: Some((10.0, 20.0)),
            offset: 10.0,
            stretch: 2.0,
            remaps: false,
        }
    );
}

#[test]
#[should_panic(expected = "its own ancestor")]
fn a_code_built_parent_cycle_is_refused() {
    let mut scene = nakkash_format::parse_document(
        r#"{"nakkash":1,"dur":"1s","layers":[{"id":"a","rect":"1x1"},{"id":"b","rect":"1x1","parent":"a"}]}"#,
    )
    .unwrap()
    .scene()
    .clone();
    scene.root.layers[0].parent = Some("b".into());
    compile(&scene, &mut Typesetter::new(FontSet::builtin())).unwrap();
}

#[test]
fn at_resolves_in_the_parents_box() {
    let g = compile_json(
        r#"{"nakkash":1,"dur":"1s","layers":[{"id":"card","rect":"200x100"},
        {"id":"kid","rect":"10x10","parent":"card","at":"center center"}]}"#,
    );
    assert_eq!(track(&g.root.nodes[1], "x").sample(0.0), 100.0);
    assert_eq!(track(&g.root.nodes[1], "y").sample(0.0), 50.0);
}

#[test]
fn a_comp_layer_with_overrides_draws_a_composition_of_its_own() {
    let g = compile_json(
        r#"{"nakkash":1,"dur":"1s","comps":{
          "label":{"size":"100x50","dur":"1s","layers":[{"id":"title","rect":"10x10"}]}},
        "layers":[{"id":"first","comp":"label"},{"id":"same","comp":"label"},
          {"id":"wide","comp":"label","overrides":{"title":{"rect":"40x10"}}}]}"#,
    );
    let drawn: Vec<usize> = g
        .root
        .nodes
        .iter()
        .map(|n| match n.content {
            NodeContent::Comp(index) => index,
            _ => panic!("a comp node"),
        })
        .collect();
    // Layers without overrides share the composition; the other gets its
    // own, after the scene's, with the same id and canvas.
    assert_eq!(drawn, [0, 0, 1]);
    assert_eq!(g.comps.len(), 2);
    assert_eq!((g.comps[1].id.as_str(), g.comps[1].width), ("label", 100.0));
    assert_eq!(g.comps[0].nodes[0].bounds.width(), 10.0);
    assert_eq!(g.comps[1].nodes[0].bounds.width(), 40.0);
}

#[test]
fn an_effect_follows_the_shown_mask_its_outline_names() {
    let g = compile_json(
        r#"{"nakkash":1,"size":"100x100","fps":30,"dur":"1s","layers":[
            {"id":"m","audio":"m.wav"},
            {"id":"a","rect":"100x100","mask":[
                {"mask":"M0,0 L9,0 Z","id":"gone","hidden":true},
                {"mask":"M0,0 L1,0 Z","id":"one"},
                {"mask":"M0,0 L2,0 Z","id":"two"}],
             "fx":[{"audio-spectrum":5,"audio":"m","along":"two"},
                   {"audio-spectrum":5,"audio":"m","hidden":true},
                   {"audio-waveform":5,"audio":"m"},
                   {"audio-waveform":5,"audio":"m","along":"gone"}]}]}"#,
    );
    // Indices among the masks shown, for the effects shown.
    assert_eq!(g.root.nodes[1].outlines, [Some(1), None, None]);
}

/// A pivot draws nothing, so its motion blur is that of what it carries: a
/// layer it moves, at any depth, blurs; one a drawn parent moves does not.
#[rstest]
#[case::carried_by_a_blurred_pivot(r#""pivot":true,"mblur":true"#, true)]
#[case::carried_by_a_pivot(r#""pivot":true"#, false)]
#[case::moved_by_a_blurred_drawn_parent(r#""rect":"10x10","mblur":true"#, false)]
fn a_layer_carried_by_a_blurred_pivot_blurs(#[case] holder: &str, #[case] blurred: bool) {
    let graph = compile_json(&format!(
        r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"p",{holder}}},
            {{"id":"card","rect":"10x10","parent":"p"}},
            {{"id":"label","rect":"5x5","parent":"card"}}]}}"#
    ));
    let mblur: Vec<bool> = graph.root.nodes.iter().skip(1).map(|n| n.mblur).collect();
    assert_eq!(mblur, [blurred, blurred]);
}

/// The scene lasts 2 s, frames 0 to 59: an exit reaches its pose on the
/// last, the way its preset's entrance arrives.
#[rstest]
#[case::rises_out("fade-up", "y", [(47.0, 200.0), (59.0, 160.0)])]
#[case::falls_out("fade-down", "y", [(47.0, 200.0), (59.0, 240.0)])]
#[case::leaves_left("fade-left", "x", [(47.0, 100.0), (59.0, 60.0)])]
#[case::fades_out("fade", "opacity", [(47.0, 1.0), (59.0, 0.0)])]
#[case::shrinks_out("pop", "scale-x", [(47.0, 1.0), (59.0, 0.6)])]
fn an_exit_leaves_the_way_its_entrance_arrives(
    #[case] preset: &str,
    #[case] target: &str,
    #[case] keys: [(f64, f64); 2],
) {
    let n = &one(&format!(r#","out":"{preset}""#), "").root.nodes[0];
    assert_eq!(pairs(track(n, target)), keys);
}

#[rstest]
#[case::with_the_span(r#","span":"0s-1s""#, (17.0, 29.0))]
#[case::early(r#","out":"fade 12f @6f""#, (41.0, 53.0))]
// Its own time starts 10 frames late, so its last frame, 59, is its 49.
#[case::in_its_own_time(r#","offset":"10f""#, (37.0, 49.0))]
#[case::at_half_speed(r#","stretch":2"#, (17.5, 29.5))]
fn an_exit_ends_when_the_layer_does(#[case] extra: &str, #[case] frames: (f64, f64)) {
    let extra = match extra.contains("\"out\"") {
        true => extra.to_owned(),
        false => format!(r#","out":"fade 12f"{extra}"#),
    };
    let n = &one(&extra, "").root.nodes[0];
    let keys = pairs(track(n, "opacity"));
    assert_eq!((keys[0].0, keys[1].0), frames);
}

#[test]
fn an_entrance_and_an_exit_share_a_property_one_after_the_other() {
    let n = &one(r#","in":"fade 10f linear","out":"fade 10f linear""#, "")
        .root
        .nodes[0];
    assert_eq!(
        pairs(track(n, "opacity")),
        [(0.0, 0.0), (10.0, 1.0), (49.0, 1.0), (59.0, 0.0)]
    );
}

#[test]
fn an_exit_to_an_absolute_value_does_not_follow_at() {
    let g = compile_json(
        r#"{"nakkash":1,"dur":"1s","presets":{"away":{"x":5000}},"layers":[{"id":"a","rect":"10x10","out":"away"}]}"#,
    );
    assert_eq!(g.root.nodes[0].follows_at, (false, true));
}

/// On the default 1920x1080 canvas, a push moves by the canvas's size.
#[rstest]
#[case::push_up_comes_from_below("in", "push-up", "y", 200.0 + 1080.0)]
#[case::push_up_leaves_at_the_top("out", "push-up", "y", 200.0 - 1080.0)]
#[case::push_down_comes_from_above("in", "push-down", "y", 200.0 - 1080.0)]
#[case::push_left_comes_from_the_right("in", "push-left", "x", 100.0 + 1920.0)]
#[case::push_right_leaves_at_the_right("out", "push-right", "x", 100.0 + 1920.0)]
#[case::rise_moves_without_fading("in", "rise", "y", 240.0)]
#[case::slam_lands_from_larger("in", "slam", "scale-x", 1.6)]
fn new_presets_start_where_they_say(
    #[case] key: &str,
    #[case] preset: &str,
    #[case] target: &str,
    #[case] far: f64,
) {
    let n = &one(&format!(r#","{key}":"{preset} 10f linear""#), "")
        .root
        .nodes[0];
    let keys = pairs(track(n, target));
    let far_key = match key {
        "in" => keys[0].1,
        _ => keys[keys.len() - 1].1,
    };
    assert_eq!(far_key, far);
    if preset != "slam" {
        assert!(
            track(n, "opacity").is_constant(),
            "{preset} keeps its opacity"
        );
    }
}

#[test]
fn a_push_inside_a_composition_moves_by_its_canvas() {
    let g = compile_json(
        r#"{"nakkash":1,"dur":"1s","comps":{"card":{"size":"400x300","dur":"1s",
          "layers":[{"id":"t","rect":"10x10","at":"200 150","in":"push-up 10f"}]}},
          "layers":[{"id":"c","comp":"card"}]}"#,
    );
    assert_eq!(pairs(track(&g.comps[0].nodes[0], "y"))[0].1, 450.0);
}
