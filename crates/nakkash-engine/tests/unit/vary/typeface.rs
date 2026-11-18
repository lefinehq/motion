use super::*;

#[test]
fn long_copy_is_about_twice_as_long_from_its_own_words() {
    let long = long_copy("Launch Week");
    assert!(long.chars().count() >= 22, "{long}");
    assert!(
        long.split(' ').all(|w| w == "Launch" || w == "Week"),
        "{long}"
    );
}

#[test]
fn balanced_lines_are_about_one_length() {
    assert_eq!(balanced("one two three four", 2), "one two\nthree four");
    assert_eq!(balanced("single", 3), "single");
}

#[test]
fn the_nearest_weight_is_the_heavier_on_a_tie() {
    assert_eq!(nearest(&[300, 500, 700], 600), 700);
    assert_eq!(nearest(&[400], 900), 400);
}

#[test]
fn long_copy_keeps_the_masters_lines_and_breaks_more_before_the_floor() {
    let master: Session = r##"{"nakkash":1,"size":"1920x1080","dur":"1s","layers":[
        {"id":"call","text":"Box call out\nwrite your\ntext here","font":"Inter Variable 54 700",
         "fill":"#FFFFFF","at":"center center"}]}"##
        .parse()
        .expect("a scene");
    let held = Held::of(master.scene(), master.document().raw());
    let step = TypeStep {
        headline: None,
        body: None,
        copy: Some("long".into()),
        fonts: Vec::new(),
        families: Vec::new(),
    };
    let (raw, note) = apply(&master, &held, &step).expect("fitted");
    let text = raw["layers"][0]["text"].as_str().expect("a text");
    // Twice the words, on at least the master's three lines.
    assert!(text.split('\n').count() >= 3, "{text}");
    assert!(text.split_whitespace().count() >= 14, "{text}");
    // Set no smaller than the floor, with the lines it needs.
    let size: f64 = raw["layers"][0]["font"]
        .as_str()
        .and_then(|f| f.split(' ').nth(2))
        .and_then(|s| s.parse().ok())
        .expect("a size");
    assert!(size >= 54.0 * SMALLEST - 0.01, "{size}: {note}");
}
