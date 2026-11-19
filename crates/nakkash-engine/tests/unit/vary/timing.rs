use super::*;

#[test]
fn moves_keep_their_rate_and_holds_take_the_change() {
    // Ten frames: four moving, six holding, made two frames shorter.
    let active = [
        false, true, true, true, true, false, false, false, false, false,
    ];
    let d = durations(&active, 0, 10, 8.0, 1.0);
    assert!((d.iter().sum::<f64>() - 8.0).abs() < 1e-9);
    assert!(d[1..5].iter().all(|x| (x - 1.0).abs() < 1e-9), "{d:?}");
    assert!(d[5..].iter().all(|x| (x - 4.0 / 6.0).abs() < 1e-9), "{d:?}");
}

#[test]
fn a_snappy_pace_shortens_moves_and_keeps_the_length() {
    let active = [
        false, true, true, true, true, false, false, false, false, false,
    ];
    let d = durations(&active, 0, 10, 10.0, Pace::Snappy.factor());
    assert!((d.iter().sum::<f64>() - 10.0).abs() < 1e-9);
    assert!((d[1] - 0.75).abs() < 1e-9, "{d:?}");
}

#[test]
fn holds_stay_within_a_third_and_three_times() {
    // Nothing but moves could absorb a cut to a tenth: all frames share it.
    let active = [false, true, true, true, true, true, true, true, true, false];
    let d = durations(&active, 0, 10, 1.0, 1.0);
    assert!((d.iter().sum::<f64>() - 1.0).abs() < 1e-9);
}

#[test]
fn a_scene_retimed_to_half_its_length_has_a_time_map_ending_there() {
    let session: Session = r##"{"nakkash":1,"size":"1000x1000","dur":"2s","layers":[
        {"id":"card","rect":"200x200","anim":{"x":"200 -> 800 @0-15f out-cubic"}}]}"##
        .parse()
        .expect("a scene");
    let step = TimingStep {
        seconds: Some(1.0),
        beats: None,
        pace: None,
        music: None,
    };
    let (map, _) = time_map(&session, &step).expect("a map");
    assert_eq!(map.frames, 30.0);
    let last = map.keys.last().unwrap();
    assert!((last.0 - 30.0).abs() < 1e-6, "{:?}", map.keys);
    // The move keeps its rate: it ends at second 0.5 of the master, and
    // the stretch of the map that leads there is as long in the variant's
    // frames as in the master's.
    let end = map
        .keys
        .iter()
        .position(|k| (k.1 - 0.5).abs() < 1e-6)
        .expect("a key where the move ends");
    let (a, b) = (map.keys[end - 1], map.keys[end]);
    assert!(
        ((b.0 - a.0) - (b.1 - a.1) * 30.0).abs() < 1e-6,
        "{:?}",
        map.keys
    );
    assert!(b.0 - a.0 >= 13.0, "{:?}", map.keys);
}
