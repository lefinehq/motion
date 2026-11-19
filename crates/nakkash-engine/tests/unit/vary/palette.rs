use serde_json::json;

use super::*;
use crate::session::Session;

/// A scene whose accent is a slot that a plate follows, a card that
/// shares the accent's colour by value, and a dot whose fill moves from the
/// accent to the logo's blue; `locks` is spliced in.
fn scene(locks: &str) -> Session {
    format!(
        r##"{{"nakkash":1,"size":"1000x1000","dur":"1s","bg":"#101820",
        "slots":{{"accent":{{"color":"#E63946"}}}},
        "roles":{{"logo":"logo"}},"locks":[{locks}],
        "layers":[
          {{"id":"plate","rect":"400x200","fill":"$accent"}},
          {{"id":"card","rect":"200x200","fill":"#E63946"}},
          {{"id":"logo","rect":"100x100","fill":"#3B82F6"}},
          {{"id":"title","text":"Hi","font":"Inter Variable 64 700","fill":[1,1,1,1]}},
          {{"id":"dot","shapes":[{{"ellipse":[50,50]}},
            {{"fill":"#E63946","anim":{{"color":"#E63946 -> #3B82F6 @0-10f"}}}}]}}]}}"##
    )
    .parse()
    .expect("a valid scene")
}

fn moved(session: &Session, scheme: &Scheme) -> serde_json::Value {
    let held = Held::of(session.scene(), session.document().raw());
    let slots = session.slots();
    let mut raw = session.document().raw().clone();
    let palette = gather(&raw, &held, &slots);
    apply(&mut raw, &held, &slots, &mapping(&palette, scheme));
    raw
}

#[test]
fn every_use_of_a_colour_moves_together_and_keeps_its_luminance() {
    let session = scene("");
    let raw = moved(&session, &Scheme::Rotate { degrees: 120.0 });
    let slot = &raw["slots"]["accent"]["color"];
    // The slot and the card, which shares its colour by value, move alike.
    assert_eq!(slot, &raw["layers"][1]["fill"]);
    assert_ne!(slot, &json!("#E63946"));
    // The track's words move too, and the white text, a grey, stays.
    let track = raw["layers"][4]["shapes"][1]["anim"]["color"]
        .as_str()
        .unwrap();
    assert!(track.starts_with(slot.as_str().unwrap()), "{track}");
    assert_eq!(raw["layers"][3]["fill"], json!([1, 1, 1, 1]));
    let before = colour::luminance(nakkash_format::color(&json!("#E63946")).unwrap());
    let after = colour::luminance(nakkash_format::color(slot).unwrap());
    assert!((before - after).abs() < 0.01, "{before} {after}");
}

#[test]
fn a_locked_layer_keeps_its_colours() {
    let session = scene(r#""@logo""#);
    let raw = moved(&session, &Scheme::Rotate { degrees: 120.0 });
    assert_eq!(raw["layers"][2]["fill"], json!("#3B82F6"));
    // The same colour elsewhere still moves.
    assert!(
        !raw["layers"][4]["shapes"][1]["anim"]["color"]
            .as_str()
            .unwrap()
            .ends_with("#3B82F6 @0-10f")
    );
}

#[test]
fn a_locked_palette_only_swaps_the_scenes_own_colours() {
    let session = scene(r#""palette""#);
    let held = Held::of(session.scene(), session.document().raw());
    assert!(held.palette);
    let raw = moved(&session, &Scheme::Swap { seed: 7 });
    let own = gather(session.document().raw(), &held, &session.slots());
    let after = gather(&raw, &held, &session.slots());
    let keys = |p: &[Entry]| {
        let mut k: Vec<String> = p.iter().map(|e| colour::hex(e.colour)).collect();
        k.sort();
        k
    };
    assert_eq!(keys(&own), keys(&after));
    assert_ne!(raw, *session.document().raw());
}

#[test]
fn invert_keeps_every_contrast_ratio() {
    let ratio = |a: f64, b: f64| (a.max(b) + 0.05) / (a.min(b) + 0.05);
    let (dark, light) = (0.02, 0.6);
    assert!((ratio(dark, light) - ratio(mirrored(dark), mirrored(light))).abs() < 1e-9);
    assert!((mirrored(0.0) - 1.0).abs() < 1e-9);
}

#[test]
fn a_scheme_is_drawn_the_same_from_the_same_seed() {
    let a = draw(&[], &[], false, &mut Rng::new(7, 2, "palette"));
    let b = draw(&[], &[], false, &mut Rng::new(7, 2, "palette"));
    assert_eq!(a, b);
}

#[test]
fn the_background_moves_with_what_is_drawn_on_it() {
    // The dark ground is mirrored with the rest, so what reads against it
    // still does.
    let raw = moved(&scene(""), &Scheme::Invert);
    assert_ne!(raw["bg"], json!("#101820"));
    let ground = nakkash_format::color(&raw["bg"]).expect("a colour");
    assert!(colour::luminance(ground) > 0.5, "{}", raw["bg"]);
}

#[test]
fn a_colour_keyed_as_a_list_of_objects_moves_too() {
    let session: Session = r##"{"nakkash":1,"size":"200x200","dur":"1s","bg":"#000000","layers":[
        {"id":"plate","rect":"100x100","fx":[{"fill":"#FF0000",
          "anim":{"color":[{"t":"0s","v":[1,0,0,1]},{"t":"1s","v":[0.8,0.8,0.8,1]}]}}]}]}"##
        .parse()
        .expect("a valid scene");
    let raw = moved(&session, &Scheme::Invert);
    let keys = &raw["layers"][0]["fx"][0]["anim"]["color"];
    assert_ne!(keys[0]["v"], json!([1, 0, 0, 1]), "{keys}");
    assert_ne!(keys[1]["v"], json!([0.8, 0.8, 0.8, 1]), "{keys}");
    assert_eq!(keys[1]["t"], json!("1s"));
}

#[test]
fn a_slot_that_changes_nothing_drawn_keeps_its_colour() {
    // `nakkash slots --verify --mark` found `hidden` and the card's own
    // `edge` change nothing drawn; `shown` does.
    let session: Session = r##"{"nakkash":1,"size":"200x200","dur":"1s","bg":"#000000",
        "slots":{"hidden":{"color":"#E63946","seen":false},"shown":{"color":"#3B82F6"}},
        "comps":{"card":{"size":"100x100","dur":"1s",
          "slots":{"edge":{"color":"#22C55E","seen":false}},
          "layers":[{"id":"rim","rect":"90x90","fill":"$edge"}]}},
        "layers":[
          {"id":"under","rect":"100x100","fill":"$hidden"},
          {"id":"over","rect":"100x100","fill":"$shown"},
          {"id":"card","comp":"card"}]}"##
        .parse()
        .expect("a valid scene");
    let raw = moved(&session, &Scheme::Rotate { degrees: 120.0 });
    assert_eq!(raw["slots"]["hidden"]["color"], json!("#E63946"));
    assert_eq!(
        raw["comps"]["card"]["slots"]["edge"]["color"],
        json!("#22C55E")
    );
    assert_ne!(raw["slots"]["shown"]["color"], json!("#3B82F6"));
}
