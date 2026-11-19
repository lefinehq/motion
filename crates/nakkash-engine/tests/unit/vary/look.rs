use super::*;

/// Look `name`, drawn from seed 1 with no faces.
fn look(name: &str) -> LookStep {
    draw(
        &[name.into()],
        0,
        &Faces::default(),
        false,
        &mut Rng::new(1, 0, "look"),
    )
}

#[test]
fn every_look_is_drawn_the_same_from_the_same_seed_and_differs_from_the_others() {
    let draw_one = |i| {
        draw(
            &[],
            i,
            &Faces::default(),
            false,
            &mut Rng::new(7, i, "look"),
        )
    };
    let all: Vec<LookStep> = (0..LOOKS.len()).map(draw_one).collect();
    assert_eq!(all, (0..LOOKS.len()).map(draw_one).collect::<Vec<_>>());
    let names: Vec<&str> = all.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, LOOKS);
    // Decisions, not shades of one: each look draws its field and title
    // its own way.
    let ways: BTreeSet<String> = all
        .iter()
        .map(|l| {
            format!(
                "{:?} {:?} {} {:?}",
                l.field.noise, l.field.fractal, l.title.glitch, l.title.case
            )
        })
        .collect();
    assert_eq!(ways.len(), LOOKS.len());
}

/// A template in the shape a look reads: a ground, a field of two noise
/// layers coloured alike by curves, and a title of red, green and blue
/// copies of a text, glowing and displaced.
fn template() -> Json {
    let colours = json!({"curves": "M 0,0 L 1,1", "red": "M 0,0 L 0.5,0.7 L 1,1",
        "green": "M 0,0 L 0.6,0.3 L 1,1", "blue": "M 0,0 L 0.7,0.2 L 1,1"});
    json!({
        "nakkash": 1, "size": "200x100", "dur": "1s",
        "comps": {
            "rays": {"size": "200x100", "dur": "1s", "layers": [
                {"id": "a", "rect": "200x100", "fill": [1, 1, 1, 1],
                 "fx": [{"fractal-noise": 0, "noise": "block", "scale-width": 1, "scale-height": 10}, colours.clone(),
                        {"glow": 20, "intensity": 0.5}]},
                {"id": "b", "rect": "200x100", "fill": [1, 1, 1, 1],
                 "fx": [{"fractal-noise": 0, "scale": 2}, colours]}]},
            "word": {"size": "200x100", "dur": "1s", "layers": [
                {"id": "t", "text": "Go fast", "font": "Inter 40", "fill": [1, 1, 1, 1]}]},
            "title": {"size": "200x100", "dur": "1s", "layers": [
                {"id": "r", "comp": "word", "fx": [{"fill": [1, 0, 0, 1]}]},
                {"id": "g", "comp": "word", "fx": [{"fill": [0, 1, 0.19, 1]}]},
                {"id": "bl", "comp": "word", "fx": [{"fill": [0, 0.02, 1, 1]}, {"glow": 10, "intensity": 0.4},
                                                  {"displacement-map": 20}]}]}},
        "layers": [
            {"id": "bg", "rect": "200x100", "fill": [0, 0, 0, 1]},
            {"id": "field", "comp": "rays", "blend": "screen"},
            {"id": "head", "comp": "title", "blend": "screen"}]
    })
}

#[test]
fn a_look_colours_each_field_layer_in_turn_and_leaves_no_map() {
    let step = look("aurora");
    let mut raw = template();
    let note = apply(&mut raw, &Held::default(), &step);
    assert!(note.contains("2 field layers coloured in turn"), "{note}");
    let rays = &raw["comps"]["rays"]["layers"];
    // The two layers the master coloured alike now part.
    assert_ne!(rays[0]["fx"][1]["red"], rays[1]["fx"][1]["red"]);
    let [r, g, b] = rgb(&json!(step.palette.field[0])).unwrap();
    assert_eq!(
        rays[0]["fx"][1]["red"],
        json!(format!(
            "M 0,0 L 0.35,{} L 0.7,{} L 1,{}",
            round(r * 0.3),
            round(r * 0.75),
            round(r)
        ))
    );
    let _ = (g, b);
    // The levels curve stays the master's.
    assert_eq!(rays[0]["fx"][1]["curves"], json!("M 0,0 L 1,1"));
    // No colour effect over the whole film.
    let text = raw.to_string();
    assert!(!text.contains("color-cycle") && !text.contains("black-white"));
}

#[test]
fn the_title_takes_split_colours_and_loses_its_glitch_when_the_look_has_none() {
    let step = look("aurora");
    assert!(!step.title.glitch);
    let mut raw = template();
    apply(&mut raw, &Held::default(), &step);
    let title = &raw["comps"]["title"]["layers"];
    assert_eq!(title[0]["fx"][0]["fill"], json!(step.palette.split[0]));
    assert_eq!(title[1]["fx"][0]["fill"], json!(step.palette.split[1]));
    assert_eq!(title[2]["fx"][0]["fill"], json!(step.palette.split[2]));
    let kinds: Vec<String> = title[2]["fx"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_object().unwrap().keys().next().unwrap().clone())
        .collect();
    assert_eq!(kinds, ["fill", "glow"]);
    // The text: the look's colour, in capitals, spaced wider.
    let t = &raw["comps"]["word"]["layers"][0];
    assert_eq!(t["fill"], json!(step.palette.text));
    assert_eq!(t["text"], json!("GO FAST"));
    assert_eq!(t["style"]["tracking"], json!(step.title.tracking));
}

#[test]
fn a_flat_look_drops_the_fields_glow_and_keeps_the_titles_glitch() {
    let step = look("riso");
    assert_eq!(step.field.glow, 0.0);
    let mut raw = template();
    apply(&mut raw, &Held::default(), &step);
    let a = raw["comps"]["rays"]["layers"][0]["fx"].as_array().unwrap();
    assert!(a.iter().all(|e| e.get("glow").is_none()));
    let bl = raw["comps"]["title"]["layers"][2]["fx"].as_array().unwrap();
    assert!(bl.iter().any(|e| e.get("displacement-map").is_some()));
    let t = &raw["comps"]["word"]["layers"][0];
    assert_eq!(t["fill"], json!(step.palette.text));
    // Grain is a layer of its own, over the film.
    let top = raw["layers"].as_array().unwrap().last().unwrap();
    assert_eq!(top["id"], json!("look-grain"));
    assert_eq!(top["blend"], json!("overlay"));
}

#[test]
fn the_ground_becomes_the_looks_gradient_and_the_noise_its_material() {
    let step = look("aurora");
    let mut raw = template();
    apply(&mut raw, &Held::default(), &step);
    let bg = &raw["layers"][0];
    assert_eq!(bg["fill"], json!(step.palette.ground[1]));
    assert_eq!(bg["fx"][0]["shape"], json!("radial"));
    assert_eq!(bg["fx"][0]["start-color"], json!(step.palette.ground[0]));
    assert_eq!(bg["fx"][0]["start"], json!([100.0, 50.0]));
    let noise = &raw["comps"]["rays"]["layers"][0]["fx"][0];
    assert_eq!(noise["noise"], json!("spline"));
    assert_eq!(noise["scale-width"], json!(round(step.field.cells[0])));
    // A noise scaled alike both ways takes the cells on its scale.
    let even = &raw["comps"]["rays"]["layers"][1]["fx"][0];
    assert_eq!(
        even["scale"],
        json!(round(
            2.0 * (step.field.cells[0] * step.field.cells[1]).sqrt()
        ))
    );
}

#[test]
fn held_layers_and_slots_keep_their_colours() {
    let step = look("ember");
    let mut raw = template();
    raw["comps"]["word"]["layers"][0]["fill"] = json!("$ink");
    let held = Held::default();
    apply(&mut raw, &held, &step);
    assert_eq!(raw["comps"]["word"]["layers"][0]["fill"], json!("$ink"));
    assert_eq!(raw["comps"]["word"]["layers"][0]["text"], json!("go fast"));
}

#[test]
fn noise_travels_faster_by_its_keys() {
    let mut noise = json!({"fractal-noise": 0, "scale-width": 0.06, "scale-height": 30,
        "anim": {"offset": [{"t": "0s", "v": [2000, -1000]}, {"t": "1s", "v": [2000, 3000]}]}});
    let mut step = look("riso");
    step.field.cells = [2.0, 0.5];
    step.field.speed = 1.5;
    move_effect(noise.as_object_mut().unwrap(), Role::Field, &step);
    assert_eq!(noise["scale-width"], json!(0.12));
    assert_eq!(noise["scale-height"], json!(15.0));
    // Half as far again from where it starts.
    assert_eq!(noise["anim"]["offset"][1]["v"], json!([2000.0, 5000.0]));
    // A title's noise is not the field's.
    let mut other = json!({"fractal-noise": 0, "scale": 2});
    assert_eq!(
        move_effect(other.as_object_mut().unwrap(), Role::Title, &step),
        0
    );
}

#[test]
fn outlined_text_keeps_its_colour_in_the_stroke() {
    let mut text = json!({"id":"t","text":"GO","fill":"#FF0000"});
    assert!(treat(
        text.as_object_mut().unwrap(),
        Treatment::Outline(4.0),
        96.0
    ));
    assert_eq!(text["style"]["stroke"], json!("#FF0000"));
    assert_eq!(text["fill"], json!("#FFFFFF00"));
}

/// A scene with a ring (a disc with a smaller disc cut out) in `card`, and
/// a plain plate at the root.
fn ringed() -> Json {
    json!({
        "nakkash": 1, "size": "200x200", "dur": "1s",
        "comps": {"card": {"size": "100x100", "dur": "1s", "layers": [
            {"id": "ring", "rect": "100x100", "scale": [0.5, 0.5],
             "anim": {"opacity": [{"t": "0s", "v": 1}, {"t": "1s", "v": 0.5}]},
             "mask": [{"mask": "M0,0 L100,0 L100,100 Z", "expand": -5},
                      {"mask": "M0,0 L100,0 L100,100 Z", "mode": "subtract", "expand": 10}]}]}},
        "layers": [{"id": "plate", "rect": "50x50", "mask": [{"mask": "M0,0 L50,0 L50,50 Z"}]},
                   {"id": "card", "comp": "card"}]
    })
}

#[test]
fn rings_go_widen_or_echo_and_other_layers_stay() {
    let held = Held::default();
    let mut off = ringed();
    assert_eq!(rings(&mut off, &held, Rings::Off), 1);
    assert_eq!(off["comps"]["card"]["layers"][0]["hidden"], json!(true));
    assert!(off["layers"][0].get("hidden").is_none());

    let mut wide = ringed();
    rings(&mut wide, &held, Rings::Width(4.0));
    // The cut shrinks by as much as the ring widens.
    assert_eq!(
        wide["comps"]["card"]["layers"][0]["mask"][1]["expand"],
        json!(6.0)
    );
    assert_eq!(
        wide["comps"]["card"]["layers"][0]["mask"][0]["expand"],
        json!(-5)
    );

    let mut echoed = ringed();
    rings(&mut echoed, &held, Rings::Echo(2));
    let layers = echoed["comps"]["card"]["layers"].as_array().unwrap();
    let ids: Vec<&str> = layers.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["ring-echo-2", "ring-echo-1", "ring"]);
    assert_eq!(layers[1]["scale"], json!([0.31, 0.31]));
    assert_eq!(layers[0]["anim"]["opacity"][1]["v"], json!(0.245));
    assert_eq!(rings(&mut ringed(), &held, Rings::Keep), 0);
}

/// A family of `weights` whose strokes are `strokes` thick.
fn family(name: &str, weights: &[u16], strokes: &[f64]) -> Family {
    Family {
        name: name.into(),
        class: "sans".into(),
        weights: weights.to_vec(),
        strokes: strokes.to_vec(),
        source: None,
    }
}

#[test]
fn a_look_takes_a_family_heavy_enough_for_the_masters_strokes() {
    // Ember names a serif first, then a sans; the serif's heaviest is
    // thinner than the master's poster face, the sans's is not.
    let faces = [
        family("Source Serif Pro", &[400, 700], &[0.07, 0.10]),
        family("Kumbh Sans", &[400, 900], &[0.08, 0.15]),
    ];
    let pick = |stroke: Option<f64>| {
        let step = draw(
            &["ember".into()],
            0,
            &Faces {
                families: &faces,
                fonts: &[],
                stroke,
            },
            false,
            &mut Rng::new(1, 0, "look"),
        );
        step.face.and_then(|t| t.body)
    };
    assert_eq!(pick(None).as_deref(), Some("Source Serif Pro"));
    assert_eq!(pick(Some(0.10)).as_deref(), Some("Source Serif Pro"));
    assert_eq!(pick(Some(0.14)).as_deref(), Some("Kumbh Sans"));
    // None heavy enough: the heaviest.
    assert_eq!(pick(Some(0.30)).as_deref(), Some("Kumbh Sans"));
}

#[test]
fn a_look_reaches_past_its_own_families_for_one_heavy_enough() {
    let faces = [
        family("Source Serif Pro", &[400, 700], &[0.07, 0.10]),
        family("Kumbh Sans", &[400, 700], &[0.08, 0.12]),
        family("Raleway", &[400, 900], &[0.07, 0.19]),
    ];
    let step = draw(
        &["ember".into()],
        0,
        &Faces {
            families: &faces,
            fonts: &[],
            stroke: Some(0.18),
        },
        false,
        &mut Rng::new(1, 0, "look"),
    );
    assert_eq!(step.face.and_then(|t| t.body).as_deref(), Some("Raleway"));
}

#[test]
fn a_looks_ease_reaches_the_titles_moves_and_not_the_fields() {
    let step = look("ember");
    let mut raw = template();
    raw["comps"]["title"]["layers"][0]["anim"] = json!({
        "position": [{"t": "0s", "v": [0, 0], "curve": "linear"}, {"t": "1s", "v": [10, 0]}],
        "scale": [[0, 1], ["1s", 1.2]]});
    raw["comps"]["rays"]["layers"][0]["anim"] = json!({"opacity": [[0, 0], ["1s", 1]]});
    apply(&mut raw, &Held::default(), &step);
    let title = &raw["comps"]["title"]["layers"][0]["anim"];
    assert_eq!(title["position"][0]["curve"], json!("out-back"));
    assert!(title["position"][1].get("curve").is_none());
    assert_eq!(title["scale"][0], json!([0, 1, "out-back"]));
    assert_eq!(
        raw["comps"]["rays"]["layers"][0]["anim"]["opacity"][0],
        json!([0, 0])
    );
}

/// A split title as a template builds it: copies in pure channels inside
/// `rgb`, read back by instances through their red and blue channels, one
/// drawing the whole grey, under a headline and a small line.
fn channelled() -> Json {
    json!({
        "nakkash": 1, "size": "400x200", "dur": "1s",
        "comps": {
            "word": {"size": "400x200", "dur": "1s", "layers": [
                {"id": "big", "text": "NAKKASH", "font": "Anton 120", "fill": "#FFFFFF"},
                {"id": "small", "text": "rust motion engine", "font": "Anton 40", "fill": "#FFFFFF"}]},
            "rgb": {"size": "400x200", "dur": "1s", "layers": [
                {"id": "r", "comp": "word", "fx": [{"fill": [1, 0, 0, 1]}]},
                {"id": "b", "comp": "word", "fx": [{"fill": [0, 0.02, 1, 1]}]}]},
            "set": {"size": "400x200", "dur": "1s", "layers": [
                {"id": "grey", "comp": "rgb", "fx": [{"tint": 1}]},
                {"id": "red", "comp": "rgb", "fx": [{"set-matte": "red"}, {"fill": [1, 1, 1, 1]}]},
                {"id": "blue", "comp": "rgb", "fx": [{"set-matte": "blue"}, {"fill": [1, 1, 1, 1]}]}]}},
        "layers": [{"id": "set", "comp": "set"}]
    })
}

#[test]
fn a_title_read_by_its_channels_keeps_them_and_colours_each_piece() {
    let step = look("riso");
    let mut raw = channelled();
    apply(&mut raw, &Held::default(), &step);
    // The copies carry the channels the instances read: they stay pure.
    assert_eq!(
        raw["comps"]["rgb"]["layers"][0]["fx"][0]["fill"],
        json!([1, 0, 0, 1])
    );
    assert_eq!(
        raw["comps"]["rgb"]["layers"][1]["fx"][0]["fill"],
        json!([0, 0.02, 1, 1])
    );
    // Each piece takes the split colour of its channel, the grey pass the
    // look's text colour at its light end and black at its dark end, so as
    // a matte it is never lighter than the master's.
    let set = &raw["comps"]["set"]["layers"];
    assert_eq!(set[1]["fx"][1]["fill"], json!(step.palette.split[0]));
    assert_eq!(set[2]["fx"][1]["fill"], json!(step.palette.split[2]));
    assert_eq!(set[0]["fx"][0]["white"], json!(step.palette.text));
    assert!(set[0]["fx"][0].get("black").is_none());
}

#[test]
fn only_the_headline_takes_the_treatment_at_a_depth_of_its_size() {
    let step = look("ember");
    let mut raw = channelled();
    apply(&mut raw, &Held::default(), &step);
    let word = &raw["comps"]["word"]["layers"];
    let shadows = |l: &Json| {
        l["fx"].as_array().map_or(0, |fx| {
            fx.iter().filter(|e| e.get("drop-shadow").is_some()).count()
        })
    };
    assert_eq!(shadows(&word[0]), 4);
    assert_eq!(shadows(&word[1]), 0);
    let Treatment::Extrude(share) = step.title.treatment else {
        panic!("ember extrudes");
    };
    let deepest = word[0]["fx"].as_array().unwrap().last().unwrap()["drop-shadow"]
        .as_f64()
        .unwrap();
    assert!((deepest - round(share * 120.0)).abs() < 1e-9, "{deepest}");
}

#[test]
fn a_look_moves_the_titles_displacement_by_its_amount() {
    let step = look("riso");
    let mut item = json!({"displacement-map": 100, "map": "tex"});
    move_effect(item.as_object_mut().unwrap(), Role::Title, &step);
    assert_eq!(
        item["displacement-map"],
        json!(round(100.0 * step.title.displace))
    );
}

#[test]
fn a_noise_drawn_only_as_a_matte_or_a_map_stays_whole() {
    // The title reads a hidden noise as its matte and another as its
    // displacement map: restyled, they would cut and move the title.
    let mut raw = template();
    let noise = json!({"size": "200x100", "dur": "1s", "layers": [
        {"id": "n", "rect": "200x100", "fill": [1, 1, 1, 1],
         "fx": [{"fractal-noise": 0, "noise": "block", "scale": 3}]},
        {"id": "black", "rect": "200x100", "fill": [0, 0, 0, 1]}]});
    raw["comps"]["matte"] = noise.clone();
    // The map draws its noise one composition in.
    raw["comps"]["map"] = json!({"size": "200x100", "dur": "1s", "layers": [
        {"id": "inner", "comp": "map-noise"}]});
    raw["comps"]["map-noise"] = noise;
    let layers = raw["layers"].as_array_mut().unwrap();
    layers.push(json!({"id": "m", "comp": "matte", "hidden": true}));
    layers.push(json!({"id": "d", "comp": "map", "hidden": true}));
    layers[2]["fx"] = json!([{"set-matte": "luminance", "matte": "m"},
                             {"displacement-map": 10, "map": "d"}]);
    let before = raw.clone();
    let step = look("riso");
    apply(&mut raw, &Held::default(), &step);
    for comp in ["matte", "map", "map-noise"] {
        assert_eq!(raw["comps"][comp], before["comps"][comp], "{comp}");
    }
    // The field's own noise still takes the look's material.
    assert_ne!(
        raw["comps"]["rays"]["layers"][1]["fx"][0],
        before["comps"]["rays"]["layers"][1]["fx"][0]
    );
}
