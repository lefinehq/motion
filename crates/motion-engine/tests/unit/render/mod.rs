//! Unit tests for `src/render/mod.rs`, and the helpers the other render
//! tests share.

use motion_raster::kurbo::{BezPath, Rect};
use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::Session;

/// The session of the scene JSON `scene`.
pub(crate) fn session(scene: &str) -> Session {
    scene.parse().expect("a valid scene")
}

/// The display list of `scene` at `frame`.
pub(crate) fn list(scene: &str, frame: f64) -> Frame {
    session(scene).display_list(frame).expect("a display list")
}

/// A 200x100 scene at 10 fps lasting 1 s, with `layers`.
pub(crate) fn scene(layers: &str) -> String {
    format!(r#"{{"motion":1,"size":"200x100","fps":10,"dur":"1s","layers":[{layers}]}}"#)
}

/// The one layer of a list that holds one.
pub(crate) fn only_layer(frame: &Frame) -> &Layer {
    match frame.nodes.as_slice() {
        [Node::Layer(layer)] => layer,
        other => panic!("not one layer: {other:?}"),
    }
}

fn path_of(node: &Node) -> &PathDraw {
    match node {
        Node::Path(path) => path,
        other => panic!("not a path: {other:?}"),
    }
}

#[test]
fn the_scene_background_is_drawn_and_blending_is_gamma() {
    let frame = list(
        r##"{"motion":1,"dur":"1s","bg":"#FF000080","layers":[]}"##,
        0.0,
    );
    assert_eq!(
        frame.background,
        Some(Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0))
    );
    assert_eq!(frame.blending, Blending::Gamma);
    assert!(frame.nodes.is_empty());
}

#[test]
fn a_clear_background_is_left_out() {
    let frame = list(
        r##"{"motion":1,"dur":"1s","bg":"#00000000","layers":[]}"##,
        0.0,
    );
    assert_eq!(frame.background, None);
}

#[test]
fn a_rect_layer_is_its_box_filled_under_its_transform() {
    let session = session(&scene(
        r##"{"id":"a","rect":"40x20 r4","fill":"#00FF00","at":"100 50","rotate":30,"opacity":0.5}"##,
    ));
    let state = session.frame(0.0).unwrap();
    let frame = session.display_list(0.0).unwrap();
    let layer = only_layer(&frame);
    assert_eq!(layer.transform, state.nodes[0].transform);
    assert_eq!(layer.opacity, 0.5);
    assert!(layer.effects.is_empty() && layer.clip.is_none() && !layer.isolate);
    let [leaf] = layer.children.as_slice() else {
        panic!("one leaf")
    };
    let rect = path_of(leaf);
    assert_eq!(rect.paint, Paint::Solid(Color::rgba(0.0, 1.0, 0.0, 1.0)));
    assert_eq!(rect.style, DrawStyle::Fill(FillRule::NonZero));
    // Rounded corners leave float dust on the edges.
    assert_eq!(
        rect.path.bounding_box().round(),
        Rect::new(0.0, 0.0, 40.0, 20.0)
    );
}

/// The golden scenes' picture, 48x32, by its absolute path.
const TILES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/images/tiles.png");

#[test]
fn an_image_layer_is_its_picture_over_its_box_under_its_transform() {
    let session = session(&scene(&format!(
        r#"{{"id":"photo","image":"{TILES}","box":[-4,-2,96,64],"at":"100 50","rotate":30,"opacity":0.5}}"#
    )));
    let state = session.frame(0.0).unwrap();
    let frame = session.display_list(0.0).unwrap();
    let layer = only_layer(&frame);
    assert_eq!(layer.transform, state.nodes[0].transform);
    assert_eq!(layer.opacity, 0.5);
    let [Node::Image(image)] = layer.children.as_slice() else {
        panic!("one picture: {:?}", layer.children)
    };
    assert_eq!(image.rect, Rect::new(-4.0, -2.0, 92.0, 62.0));
    assert_eq!((image.picture.width(), image.picture.height()), (48, 32));
}

#[test]
fn a_missing_picture_is_an_error_at_its_layers_image() {
    let e = session(&scene(
        r#"{"id":"photo","image":"no/such/file.png","box":[10,10]}"#,
    ))
    .display_list(0.0)
    .unwrap_err();
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (motion_format::Kind::Io, Some("photo.image"))
    );
    assert!(e.message.contains("file.png"), "{e}");
    // A picture no frame shows is never read.
    let hidden = session(&scene(
        r#"{"id":"photo","image":"no/such/file.png","box":[10,10],"hidden":true}"#,
    ));
    assert!(hidden.display_list(0.0).is_ok());
}

#[test]
fn rounding_stops_at_half_the_shorter_side() {
    let pill = rect(
        &RectShape {
            width: 40.0,
            height: 20.0,
            radius: 500.0,
        },
        [1.0; 4],
    );
    let capped = rect(
        &RectShape {
            width: 40.0,
            height: 20.0,
            radius: 10.0,
        },
        [1.0; 4],
    );
    assert_eq!(pill, capped);
}

#[rstest]
#[case::hidden(r#""hidden":true"#)]
#[case::outside_its_span(r#""span":"5f-8f""#)]
#[case::transparent(r#""opacity":0"#)]
fn a_layer_that_shows_nothing_is_left_out(#[case] key: &str) {
    let frame = list(
        &scene(&format!(r#"{{"id":"a","rect":"10x10",{key}}}"#)),
        0.0,
    );
    assert!(frame.nodes.is_empty());
}

#[test]
fn a_pivot_draws_nothing_and_its_children_follow_it() {
    let session = session(&scene(
        r#"{"id":"p","pivot":true,"at":"50 50","rotate":90},{"id":"k","rect":"10x10","parent":"p","at":"20 0"}"#,
    ));
    let state = session.frame(0.0).unwrap();
    let frame = session.display_list(0.0).unwrap();
    assert_eq!(only_layer(&frame).transform, state.nodes[1].transform);
}

const COMP: &str = r##"{"motion":1,"size":"200x100","fps":10,"dur":"2s",
  "comps":{"card":{"size":"40x30","fps":10,"dur":"1s","bg":"#102030","layers":[
    {"id":"dot","rect":"10x10","anim":{"opacity":"0 -> 1 @0-10f"}}]}},
  "layers":[{"id":"c","comp":"card","offset":"2f","fx":[{"fill":"#FF0000"}]}]}"##;

#[test]
fn a_comp_is_an_isolated_layer_clipped_to_its_canvas_over_its_background() {
    let session = session(COMP);
    let state = session.frame(7.0).unwrap();
    let frame = session.display_list(7.0).unwrap();
    let layer = only_layer(&frame);
    assert_eq!(layer.transform, state.nodes[0].transform);
    assert_eq!(
        layer.clip,
        Some(Clip::Rect(Rect::new(0.0, 0.0, 40.0, 30.0)))
    );
    assert!(layer.isolate);
    assert_eq!(layer.effects.len(), 1);
    let [background, Node::Layer(dot)] = layer.children.as_slice() else {
        panic!("a background and a layer: {:?}", layer.children)
    };
    let background = path_of(background);
    assert_eq!(
        background.path.bounding_box(),
        Rect::new(0.0, 0.0, 40.0, 30.0)
    );
    assert_eq!(
        background.paint,
        Paint::Solid(color([16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 1.0]))
    );
    // The card's own clock: five frames in, its dot is half faded in.
    assert_eq!(dot.opacity, 0.5);
}

/// [`COMP`] with its layer moved and collapsed, with or without its effect.
fn collapsed(effect: bool) -> String {
    let collapsed = COMP.replace(
        r#""offset":"2f","#,
        r#""offset":"2f","position":[50,20],"collapse":true,"#,
    );
    match effect {
        true => collapsed,
        false => collapsed.replace(r##","fx":[{"fill":"#FF0000"}]"##, ""),
    }
}

#[test]
fn a_collapsed_comp_draws_into_its_canvas_with_its_effects_over_it() {
    let session = session(&collapsed(true));
    let state = session.frame(7.0).unwrap();
    let frame = session.display_list(7.0).unwrap();
    let layer = only_layer(&frame);
    // Its effects run in the scene's coordinates, over the scene's canvas,
    // and its layers are moved there, without the card's clip or background.
    // Its box is the card's canvas where it lands.
    assert_eq!(layer.transform, Affine::IDENTITY);
    assert_eq!(layer.clip, None);
    let card = Rect::new(0.0, 0.0, 40.0, 30.0);
    assert_eq!(
        layer.bounds,
        Some(state.nodes[0].transform.transform_rect_bbox(card))
    );
    assert_ne!(layer.bounds, Some(card));
    assert_eq!(layer.effects.len(), 1);
    assert!(layer.isolate);
    let [Node::Layer(moved)] = layer.children.as_slice() else {
        panic!("one moving layer: {:?}", layer.children)
    };
    assert_eq!(moved.transform, state.nodes[0].transform);
    assert_ne!(moved.transform, Affine::IDENTITY);
    let [Node::Layer(dot)] = moved.children.as_slice() else {
        panic!("the dot alone: {:?}", moved.children)
    };
    assert_eq!(dot.opacity, 0.5);
}

/// What the displacement map of a grey layer reads from `map`, a moved,
/// scaled and hidden layer of a 40x20 composition holding one square.
fn read_map(map: &str) -> Node {
    let scene = format!(
        r##"{{"motion":1,"size":"200x100","fps":10,"dur":"1s",
        "comps":{{"m":{{"size":"40x20","dur":"1s","layers":[{{"id":"dot","rect":"10x10","at":"20 10"}}]}}}},
        "layers":[{{"id":"map","comp":"m","hidden":true,"position":[150,70],"scale":[2,3],{map}}},
        {{"id":"grey","rect":"200x100","fill":"#808080","fx":[{{"displacement-map":10,"map":"map"}}]}}]}}"##
    );
    let frame = list(&scene, 0.0);
    let grey = only_layer(&frame);
    grey.effects[0].input().expect("the map's drawing").clone()
}

#[test]
fn an_effect_reads_a_collapsed_comp_as_its_composition() {
    // Collapsed, the composition draws into the scene under its layer's
    // transform; read by an effect, it is the composition in its own
    // coordinates all the same, as an uncollapsed one is read.
    let collapsed = read_map(r#""collapse":true"#);
    assert_eq!(collapsed, read_map(r#""collapse":false"#));
    let Node::Layer(read) = &collapsed else {
        panic!("a layer: {collapsed:?}")
    };
    assert_eq!(read.transform, Affine::IDENTITY);
    assert_eq!(read.bounds, Some(Rect::new(0.0, 0.0, 40.0, 20.0)));
}

#[test]
fn a_collapsed_comp_without_effects_lets_its_blends_reach_below() {
    let frame = list(&collapsed(false), 7.0);
    let layer = only_layer(&frame);
    assert!(!layer.isolate && layer.effects.is_empty() && layer.clip.is_none());
}

#[test]
fn a_clear_comp_background_draws_nothing() {
    let frame = list(&COMP.replace("#102030", "#10203000"), 7.0);
    assert!(matches!(
        only_layer(&frame).children.as_slice(),
        [Node::Layer(_)]
    ));
}

#[rstest]
#[case::before_its_offset(1.0)]
#[case::past_its_length(2.0 + 10.0)]
fn a_comp_out_of_its_time_draws_nothing(#[case] frame: f64) {
    assert!(list(COMP, frame).nodes.is_empty());
}

#[test]
fn moving_by_the_identity_adds_no_layer() {
    let leaf = fill(BezPath::new(), Color::BLACK);
    assert_eq!(
        moved(Affine::IDENTITY, vec![leaf.clone()]),
        vec![leaf.clone()]
    );
    let shifted = Affine::translate((3.0, 4.0));
    assert_eq!(
        moved(shifted, vec![leaf.clone()]),
        vec![Node::Layer(Layer {
            transform: shifted,
            ..Layer::new(vec![leaf])
        })]
    );
}

const CLIPS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../motion-video/tests/fixtures"
);

/// The mean difference between `scene` drawn at `frame` and frame `i` of
/// the 32x16 test clip `rgb.mov`, per channel.
fn off_clip_frame(layer: &str, frame: f64, i: usize) -> f64 {
    let scene = format!(
        r##"{{"motion":1,"size":"32x16","fps":10,"dur":"3s","bg":"#000000","layers":[{layer}]}}"##
    );
    let drawn = session(&scene)
        .render_with(frame, motion_raster::RenderOptions::GOLDEN)
        .unwrap();
    let all = std::fs::read(format!("{CLIPS}/rgb.rgba")).unwrap();
    let want = &all[i * 2048..(i + 1) * 2048];
    drawn
        .pixels()
        .iter()
        .zip(want)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / 2048.0
}

#[test]
fn a_video_layer_shows_the_clip_frame_of_its_own_time() {
    let plain = format!(r#"{{"id":"v","video":"{CLIPS}/rgb.mov","box":[32,16],"at":"16 8"}}"#);
    for f in [0, 3, 9] {
        assert!(off_clip_frame(&plain, f as f64, f) < 0.5, "frame {f}");
        assert!(
            off_clip_frame(&plain, f as f64, (f + 1) % 10) > 2.0,
            "frame {f}"
        );
    }
    // Moved later, the clip starts later.
    let late = plain.replace(r#""at""#, r#""offset":"0.3s","at""#);
    assert!(off_clip_frame(&late, 5.0, 2) < 0.5);
    // Stretched, it plays at half speed.
    let slow = plain.replace(r#""at""#, r#""stretch":2,"at""#);
    assert!(off_clip_frame(&slow, 8.0, 4) < 0.5);
}

/// An H.264 clip draws through the system's decoder (D60), frame by frame
/// in its own time: frame `n` of the counting clip is grey 16 + 10 `n` in
/// limited range, at 24000/1001 frames a second.
#[cfg(target_os = "macos")]
#[test]
fn an_h264_layer_shows_the_frame_of_its_own_time() {
    let layer =
        format!(r#"{{"id":"v","video":"{CLIPS}/h264-count.mp4","box":[64,32],"at":"32 16"}}"#);
    let scene = format!(
        r##"{{"motion":1,"size":"64x32","fps":23.976023976023978,"dur":"20f","bg":"#000000","layers":[{layer}]}}"##
    );
    let s = session(&scene);
    for n in [0usize, 3, 6, 13, 19] {
        let drawn = s
            .render_with(n as f64, motion_raster::RenderOptions::GOLDEN)
            .unwrap();
        let grey = drawn.pixels()[4 * (16 * 64 + 32)];
        let want = (10.0 * n as f64 * 255.0 / 219.0).round() as u8;
        assert!(
            grey.abs_diff(want) <= 2,
            "frame {n}: grey {grey}, not {want}"
        );
    }
}

#[test]
fn a_video_layer_draws_nothing_outside_its_clip() {
    let layer = format!(
        r#"{{"id":"v","video":"{CLIPS}/rgb.mov","box":[32,16],"at":"16 8","offset":"0.5s"}}"#
    );
    let scene = format!(r#"{{"motion":1,"size":"32x16","fps":10,"dur":"3s","layers":[{layer}]}}"#);
    let s = session(&scene);
    assert!(s.display_list(4.0).unwrap().nodes.is_empty());
    assert!(!s.display_list(5.0).unwrap().nodes.is_empty());
    assert!(!s.display_list(14.0).unwrap().nodes.is_empty());
    assert!(s.display_list(15.0).unwrap().nodes.is_empty());
}

/// The B-frame test clip with its codec renamed to MPEG-4 Part 2, which
/// motion has no decoder for, in a scratch file.
fn undecodable_clip() -> std::path::PathBuf {
    let mut bytes = std::fs::read(format!("{CLIPS}/bframes.mp4")).unwrap();
    let stsd = bytes.windows(4).position(|w| w == b"stsd").unwrap();
    let at = stsd + bytes[stsd..].windows(4).position(|w| w == b"avc1").unwrap();
    bytes[at..at + 4].copy_from_slice(b"mp4v");
    let path = std::env::temp_dir().join(format!("motion-engine-mp4v-{}.mp4", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_clip_without_a_decoder_is_an_error_at_its_layers_video() {
    let clip = undecodable_clip();
    let e = session(&scene(&format!(
        r#"{{"id":"v","video":"{}","box":[32,16]}}"#,
        clip.display()
    )))
    .display_list(0.0)
    .unwrap_err();
    std::fs::remove_file(&clip).unwrap();
    assert_eq!(e.path_text().as_deref(), Some("v.video"));
    assert!(
        e.message.contains("MPEG-4 Part 2") && e.message.contains("motion-engine-mp4v"),
        "{e}"
    );
    let e = session(&scene(
        r#"{"id":"v","video":"no/such/clip.mov","box":[32,16]}"#,
    ))
    .display_list(0.0)
    .unwrap_err();
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (motion_format::Kind::Io, Some("v.video"))
    );
}

#[test]
fn every_blend_mode_of_the_format_is_one_the_rasteriser_draws() {
    for name in motion_format::BLEND_MODES {
        assert_eq!(BlendMode::named(name).map(BlendMode::name), Ok(name));
    }
}

/// A hidden matte source that blurs still draws into the matte: hidden
/// says it draws nothing of its own, not that it is outside its span at
/// the shutter's moments.
#[rstest]
#[case::drawn_at_each_moment("")]
#[case::drawn_once_and_moved(r##","fx":[{"fill":"#FFFFFF"}]"##)]
fn a_hidden_matte_source_that_blurs_still_mattes(#[case] fx: &str) {
    let frame = list(
        &scene(&format!(
            r##"{{"id":"box","rect":"120x60","fill":"#2E7D32","at":"100 50","matte":"m alpha"}},
            {{"id":"m","rect":"40x20","fill":"#000000","hidden":true,"mblur":true,
              "anim":{{"x":"60 -> 140 @0-10f"}}{fx}}}"##
        )),
        5.0,
    );
    let matte = only_layer(&frame).matte.as_ref().expect("a matte");
    match &*matte.source {
        Node::Layer(source) => assert!(
            !source.children.is_empty() || !source.motion.is_empty(),
            "an empty matte: {source:?}"
        ),
        other => panic!("not a layer: {other:?}"),
    }
}

/// At the first frame of its span a blurred layer is whole: the shutter opens before
/// its span, and those moments move it rather than leave it out.
#[rstest]
#[case::drawn_at_each_moment("")]
#[case::drawn_once_and_moved(r##","fx":[{"fill":"#FFFFFF"}]"##)]
fn a_blurred_layer_does_not_fade_in_at_its_start(#[case] fx: &str) {
    let frame = list(
        &scene(&format!(
            r##"{{"id":"r","rect":"40x20","fill":"#000000","mblur":true,"span":"2f-10f",
              "anim":{{"x":"60 -> 140 @0-10f"}}{fx}}}"##
        )),
        2.0,
    );
    let layer = only_layer(&frame);
    let opacity = match layer.motion.is_empty() {
        true => layer
            .children
            .iter()
            .map(|child| match child {
                Node::Layer(sample) => sample.opacity,
                other => panic!("not a layer: {other:?}"),
            })
            .sum(),
        false => layer.opacity,
    };
    assert!((opacity - 1.0).abs() < 1e-4, "opacity {opacity}");
}

/// The ids of `scene`'s root nodes in the order they draw at frame 0.
fn painted(scene: &str) -> Vec<String> {
    let session = session(scene);
    let state = session.frame(0.0).expect("a frame");
    paint_order(&state)
        .into_iter()
        .map(|i| state.nodes[i].id.clone())
        .collect()
}

#[test]
fn a_run_of_3d_layers_draws_the_farthest_first() {
    let order = painted(&scene(
        r#"{"id":"near","rect":"10x10","3d":true,"z":-50},
           {"id":"far","rect":"10x10","3d":true,"z":200},
           {"id":"middle","rect":"10x10","3d":true}"#,
    ));
    assert_eq!(order, ["far", "middle", "near"]);
}

#[test]
fn a_flat_layer_keeps_the_runs_on_each_side_apart() {
    let order = painted(&scene(
        r#"{"id":"a","rect":"10x10","3d":true,"z":-50},
           {"id":"b","rect":"10x10","3d":true,"z":200},
           {"id":"flat","rect":"10x10"},
           {"id":"c","rect":"10x10","3d":true,"z":-80},
           {"id":"d","rect":"10x10","3d":true,"z":300}"#,
    ));
    assert_eq!(order, ["b", "a", "flat", "d", "c"]);
}

#[test]
fn a_layer_that_draws_nothing_splits_no_run() {
    let order = painted(&scene(
        r#"{"id":"a","rect":"10x10","3d":true,"z":-50},
           {"id":"rig","pivot":true},
           {"id":"cam","camera":true},
           {"id":"b","rect":"10x10","3d":true,"z":200}"#,
    ));
    assert_eq!(order, ["b", "a"]);
}

#[test]
fn equally_deep_3d_layers_keep_their_list_order() {
    let order = painted(&scene(
        r#"{"id":"a","rect":"10x10","3d":true},{"id":"b","rect":"10x10","3d":true}"#,
    ));
    assert_eq!(order, ["a", "b"]);
}

#[test]
fn a_small_layer_just_in_front_of_a_turned_card_draws_over_it() {
    // A wide card turned about Y, and a label 1 px in front of it near the
    // edge turned away: the label's centre is deeper than the card's, but
    // all of it is on the camera's side of the card's plane.
    let layers = r#"{"id":"label","rect":"20x20","3d":true,"parent":"card","position":[30,50],"z":-1},
           {"id":"card","rect":"400x100","3d":true,"rotate-y":50}"#;
    let session = session(&scene(layers));
    let state = session.frame(0.0).expect("a frame");
    let depth = |i: usize| {
        let node = &state.nodes[i];
        state
            .view
            .depth(node.space.expect("3D"), node.local_bounds.center())
    };
    let (label, card) = (0, 1);
    assert_eq!(state.nodes[label].id, "label");
    assert!(depth(label) > depth(card), "the label's centre is deeper");
    assert_eq!(painted(&scene(layers)), ["card", "label"]);
}

#[test]
fn layers_in_one_plane_keep_their_list_order_however_deep_their_centres() {
    let order = painted(&scene(
        r#"{"id":"edge","rect":"20x20","3d":true,"parent":"wide","position":[370,50]},
           {"id":"wide","rect":"400x100","3d":true,"rotate-y":50}"#,
    ));
    assert_eq!(order, ["edge", "wide"]);
}

#[test]
fn a_collapsed_composition_joins_the_outer_camera() {
    // The inner card sits at depth 0 of a 100x50 composition, collapsed into
    // the 200x100 scene: it is seen through the scene's default camera, not
    // the composition's own, so it shows at its size where the composition
    // puts it.
    let session = session(
        r#"{"motion":1,"size":"200x100","fps":10,"dur":"1s",
        "comps":{"inner":{"size":"100x50","dur":"1s","layers":[{"id":"card","rect":"20x10","3d":true,"at":"10 5"}]}},
        "layers":[{"id":"held","comp":"inner","collapse":true,"at":"100 50"}]}"#,
    );
    let state = session.frame(0.0).unwrap();
    let held = &state.nodes[0];
    let ContentState::Comp(_, Some(inner)) = &held.content else {
        panic!("a shown composition");
    };
    let inner = joined(inner, &state.view, held);
    assert_eq!(inner.view, state.view);
    let plane = inner.nodes[0].plane.unwrap();
    // Corner (0, 0) of the card: 10 px left and 5 px up of the
    // composition's (10, 5), in the group the comp node's transform moves.
    let corner = space::project(plane, motion_raster::kurbo::Point::ZERO).unwrap();
    let on_canvas = held.transform * corner;
    assert!(
        (on_canvas.x - 50.0).abs() < 1e-6 && (on_canvas.y - 25.0).abs() < 1e-6,
        "{on_canvas:?}"
    );
}

/// The columns and rows of `scene`'s frame 0 where something opaque enough
/// was drawn over its black background, as `(x0, x1, y0, y1)`.
fn lit_box(scene: &str) -> (usize, usize, usize, usize) {
    let image = session(scene)
        .render_with(0.0, crate::RenderOptions::GOLDEN)
        .expect("a frame");
    let (w, h) = (image.width() as usize, image.height() as usize);
    let px = image.pixels();
    let lit = |x: usize, y: usize| px[(y * w + x) * 4] > 128;
    let xs: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| lit(x, y))).collect();
    let ys: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| lit(x, y))).collect();
    (
        xs[0],
        *xs.last().unwrap() + 1,
        ys[0],
        *ys.last().unwrap() + 1,
    )
}

#[test]
fn a_3d_layer_at_depth_zero_draws_where_a_flat_one_does() {
    let flat = lit_box(&scene(r##"{"id":"c","rect":"80x40","fill":"#FFFFFF"}"##));
    let deep = lit_box(&scene(
        r##"{"id":"c","rect":"80x40","fill":"#FFFFFF","3d":true}"##,
    ));
    assert_eq!(flat, deep);
}

#[test]
fn a_3d_layer_turned_about_y_is_foreshortened() {
    // 80 px wide, turned 60°: about 40 px on screen, a little more for the
    // nearer edge's perspective.
    let (x0, x1, _, _) = lit_box(&scene(
        r##"{"id":"c","rect":"80x40","fill":"#FFFFFF","3d":true,"rotate-y":60}"##,
    ));
    let width = x1 - x0;
    assert!((39..=43).contains(&width), "{width}");
}

#[test]
fn a_3d_layer_farther_away_draws_smaller() {
    // One zoom away from the default camera (200 · 50/36 px): half size.
    let (x0, x1, y0, y1) = lit_box(&scene(
        r##"{"id":"c","rect":"80x40","fill":"#FFFFFF","3d":true,"z":277.7777777777778}"##,
    ));
    assert_eq!((x1 - x0, y1 - y0), (40, 20));
}

/// The colour at the centre of frame `frame` of a 400x200 scene of `layer`
/// over a grey ground, drawn as `render` draws it, with the layer's motion
/// blur on or off. `layer` may name `picture.png`, a 40x40 image of the
/// colour `#3A7BD5`, written beside the scene.
fn centre(layer: &str, mblur: bool, frame: f64) -> [u8; 4] {
    let dir = std::env::temp_dir().join(format!(
        "motion-engine-blur-colour-{}-{}",
        std::process::id(),
        layer.len() + usize::from(mblur)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let picture = motion_raster::Image::new(40, 40, [58, 123, 213, 255].repeat(1600)).unwrap();
    std::fs::write(dir.join("picture.png"), picture.to_png().unwrap()).unwrap();
    let path = dir.join("scene.json");
    let scene = format!(
        r##"{{"motion":1,"size":"400x200","fps":10,"dur":"1s","bg":"#808080",
            "layers":[{{"id":"l",{layer},"mblur":{mblur}}}]}}"##
    );
    std::fs::write(&path, scene).unwrap();
    let image = Session::open(&path).unwrap().render(frame).unwrap();
    image.pixel(200, 100).unwrap()
}

/// Motion blur averages a layer's drawings, so where every one of them is
/// the same colour, the blurred frame keeps that colour: no channel drifts.
/// ENGINE-GAPS #49 saw blurred screenshots come out tinted yellow and green.
#[rstest]
#[case::a_shape(r##""rect":"300x160","fill":"#3A7BD5","anim":{"x":"180 -> 220 @0-1s"}"##)]
#[case::a_scaled_picture(
    r#""image":"picture.png","box":[40,40],"anim":{"y":"90 -> 110 @0-1s","scale":"6 -> 7 @0-1s"}"#
)]
#[case::a_picture_with_effects(
    r#""image":"picture.png","box":[40,40],"scale":6,"fx":[{"transform":1}],"anim":{"x":"180 -> 220 @0-1s"}"#
)]
#[case::a_turning_3d_picture(
    r#""image":"picture.png","box":[40,40],"scale":6,"3d":true,"anim":{"rotate-y":"-20 -> 20 @0-1s"}"#
)]
fn a_blurred_layer_keeps_its_colour(#[case] layer: &str) {
    let sharp = centre(layer, false, 5.0);
    let blurred = centre(layer, true, 5.0);
    let drift = sharp
        .iter()
        .zip(blurred)
        .map(|(a, b)| a.abs_diff(b))
        .max()
        .unwrap();
    assert!(drift <= 1, "sharp {sharp:?}, blurred {blurred:?}");
}

/// A blurred pivot blurs the layer it moves: the layer is drawn at each
/// moment of the shutter, though the pivot itself draws nothing.
#[test]
fn a_layer_moved_by_a_blurred_pivot_is_drawn_blurred() {
    let frame = list(
        &scene(
            r##"{"id":"p","pivot":true,"mblur":true,"anim":{"x":"20 -> 180 @0-10f"}},
                {"id":"card","rect":"40x20","fill":"#000000","parent":"p"}"##,
        ),
        5.0,
    );
    let samples = only_layer(&frame).children.len();
    assert!(samples > 1, "{samples} drawings");
}
