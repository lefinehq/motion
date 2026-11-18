//! Unit tests for `src/colors.rs`.

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;
use crate::address::key_text;

fn listed(scene: &Json) -> Vec<(String, bool, bool, bool)> {
    places(scene)
        .into_iter()
        .map(|p| (key_text(&p.steps), p.text, p.background, p.shown))
        .collect()
}

#[test]
fn colours_are_found_by_the_kinds_of_their_keys() {
    let scene = json!({"nakkash":1,"size":"100x100","dur":"1s","bg":"#000000","layers":[
        {"id":"t","text":"Hi","fill":"#FFFFFF","style":{"stroke":[0,0,0,1]},
         "runs":[{"range":[0,1],"fill":"$accent"}],
         "animate":[{"animator":[{"range":"char"}],"fill":"#FF0000"}]},
        {"id":"s","shapes":[{"group":[{"rect":[10,10]},{"fill":[1,0,0,1],"anim":{"color":[{"t":"0s","v":[1,0,0,1]},{"t":"1s","v":[0,0,1,1]}]}}]}],
         "fx":[{"drop-shadow":10,"color":"#00FF00"}],"opacity":[0.1,0.2,0.3]},
        {"id":"a","rect":"10x10","fill":"#FFFFFF","adjust":true}
    ]});
    let found = listed(&scene);
    let at = |path: &str| found.iter().find(|p| p.0 == path).cloned();
    assert_eq!(at("bg"), Some(("bg".into(), false, true, true)));
    assert_eq!(at("layers[0].fill").map(|p| p.1), Some(true));
    assert!(at("layers[0].style.stroke").is_some());
    assert!(at("layers[0].runs[0].fill").is_some());
    assert!(at("layers[0].animate[0].fill").is_some());
    assert!(at("layers[1].shapes[0].group[1].fill").is_some());
    assert!(at("layers[1].shapes[0].group[1].anim.color[1].v").is_some());
    assert!(at("layers[1].fx[0].color").is_some());
    // An adjustment layer's fill does not show.
    assert_eq!(at("layers[2].fill").map(|p| p.3), Some(false));
    // Three numbers that are no colour property are not one.
    assert!(found.iter().all(|p| !p.0.contains("opacity")));
}

#[test]
fn a_bottom_layer_over_the_canvas_hides_the_background() {
    let scene = json!({"nakkash":1,"size":"100x100","dur":"1s","bg":"#000000","layers":[
        {"id":"plate","rect":"100x100","fill":"#203040","anchor":[50,50],"position":[50,50]}
    ]});
    let found = listed(&scene);
    assert_eq!(found[0], ("bg".into(), false, true, false));
    assert_eq!(found[1], ("layers[0].fill".into(), false, true, true));
}
