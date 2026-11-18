use super::*;

#[test]
fn every_look_is_drawn_the_same_from_the_same_seed_and_differs_from_the_others() {
    let draw_one = |i| {
        draw(
            &[],
            i,
            &Lit::default(),
            &[],
            &[],
            false,
            &mut Rng::new(7, i, "look"),
        )
    };
    let all: Vec<LookStep> = (0..LOOKS.len()).map(draw_one).collect();
    assert_eq!(all, (0..LOOKS.len()).map(draw_one).collect::<Vec<_>>());
    let names: Vec<&str> = all.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, LOOKS);
}

#[test]
fn the_grade_maps_intensity_through_the_looks_stops() {
    let paper = draw(
        &["paper".into()],
        0,
        &Lit::default(),
        &[],
        &[],
        false,
        &mut Rng::new(1, 0, "look"),
    );
    let fx = grade(&paper);
    // Paper runs from a light ground to dark ink: red starts high, ends low.
    let red = fx[1]["red"].as_str().unwrap();
    assert!(red.starts_with("M 0,0.95"), "{red}");
    let last: f64 = red.rsplit(',').next().unwrap().parse().unwrap();
    assert!((last - 0.0549).abs() < 0.002, "{red}");
    // A smooth curve through the stops, as many points as a curve holds.
    assert_eq!(red.matches(" L ").count(), 15, "{red}");
    // Hues go to grey before the map.
    assert!(fx[0].get("black-white").is_some());
    assert!(fx[1].get("color-cycle").is_some());
    // Pure white stays on the last stop rather than wrapping to the first.
    assert!(fx[1]["repetitions"].as_f64().unwrap() < 1.0);
    // And it lays grain over the film.
    assert!(fx.iter().any(|f| f.get("fractal-noise").is_some()));
}

#[test]
fn effect_settings_move_inside_their_range() {
    let mut glow = json!({"glow": 30, "intensity": 2, "anim": {"intensity": "1 -> 3 @0-10f"}});
    let mut step = draw(
        &["amber".into()],
        0,
        &Lit::default(),
        &[],
        &[],
        false,
        &mut Rng::new(1, 0, "look"),
    );
    step.glow = [2.0, 100.0];
    let n = move_effect(glow.as_object_mut().unwrap(), &step);
    assert_eq!(n, 3);
    assert_eq!(glow["glow"], json!(60.0));
    let top = range_of(&nakkash_format::item::GLOW, "intensity").map_or(f64::MAX, |r| r.1);
    assert!(glow["intensity"].as_f64().unwrap() <= top);
    assert!(
        glow["anim"]["intensity"]
            .as_str()
            .unwrap()
            .ends_with("@0-10f")
    );
}

#[test]
fn outlined_text_keeps_its_colour_in_the_stroke() {
    let mut text = json!({"id":"t","text":"GO","fill":"#FF0000"});
    assert!(treat(
        text.as_object_mut().unwrap(),
        Treatment::Outline(4.0)
    ));
    assert_eq!(text["style"]["stroke"], json!("#FF0000"));
    assert_eq!(text["fill"], json!("#FFFFFF00"));
}

#[test]
fn stops_land_at_the_masters_quantiles_in_order() {
    let lit: Vec<f64> = (1..=100).map(|i| f64::from(i) / 400.0).collect();
    let stops = placed(
        &[
            (0.0, "#000000"),
            (0.5, "#FF0000"),
            (0.9, "#00FF00"),
            (1.0, "#FFFFFF"),
        ],
        &lit,
    );
    let at: Vec<f64> = stops.iter().map(|s| s.0).collect();
    assert_eq!(at[0], 0.0);
    // Halfway from where they are written to the quantiles.
    assert!((at[1] - 0.31375).abs() < 1e-3, "{at:?}");
    assert!((at[2] - 0.56375).abs() < 1e-3, "{at:?}");
    assert_eq!(at[3], 1.0);
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

#[test]
fn noise_cells_and_speed_move_its_shape_and_travel() {
    let mut noise = json!({"fractal-noise": 0, "scale-width": 0.06, "scale-height": 30,
        "anim": {"offset": [{"t": "0s", "v": [2000, -1000]}, {"t": "1s", "v": [2000, 3000]}]}});
    let mut step = draw(
        &["acid".into()],
        0,
        &Lit::default(),
        &[],
        &[],
        false,
        &mut Rng::new(1, 0, "look"),
    );
    step.cells = [2.0, 0.5];
    step.noise[3] = 1.5;
    move_effect(noise.as_object_mut().unwrap(), &step);
    assert_eq!(noise["scale-width"], json!(0.12));
    assert_eq!(noise["scale-height"], json!(15.0));
    assert!(noise.get("scale").is_none());
    // Half as far again from where it starts.
    assert_eq!(noise["anim"]["offset"][1]["v"], json!([2000.0, 5000.0]));
    // A noise scaled alike both ways takes the cells on its scale.
    let mut even = json!({"fractal-noise": 0, "scale": 2});
    move_effect(even.as_object_mut().unwrap(), &step);
    assert_eq!(even["scale"], json!(2.0));
}

#[test]
fn a_red_title_reads_lighter_than_the_blue_field_it_sits_on() {
    assert!(grey(&HUES, [0.9, 0.08, 0.12]) > grey(&HUES, [0.08, 0.16, 0.9]) + 0.5);
    // Grey stays grey.
    assert!((grey(&HUES, [0.5, 0.5, 0.5]) - 0.5).abs() < 1e-9);
    let px = [
        230u8, 20, 30, 255, 20, 40, 230, 255, 0, 0, 0, 255, 255, 255, 255, 255,
    ];
    let lit = lit(&[crate::Image::new(4, 1, px.to_vec()).unwrap()]);
    assert_eq!(lit.hues, HUES);
    assert_eq!(lit.levels.len(), 3);
}

#[test]
fn the_map_never_overshoots_a_stop() {
    let stops = vec![
        (0.0, "#000000".to_owned()),
        (0.1, "#3A0A6E".to_owned()),
        (0.2, "#8A2BE2".to_owned()),
        (1.0, "#FFFFFF".to_owned()),
    ];
    let curve = smooth(&stops);
    assert_eq!(curve.len(), GRADE_POINTS);
    let light = |rgb: [f64; 3]| nakkash_format::oklch::oklab(rgb)[0];
    for pair in curve.windows(2) {
        assert!(pair[0].0 < pair[1].0);
        assert!(light(pair[1].1) >= light(pair[0].1) - 1e-6, "{curve:?}");
    }
}
