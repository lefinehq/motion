//! Unit tests for `src/map/variable.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;

use serde_json::json;

use super::*;

#[rstest]
#[case("01_EDIT_TEXT_and_LOGO", true)]
#[case("Change Logo Here", true)]
#[case("YOUR TEXT", true)]
#[case("Placeholder 1", true)]
#[case("BG_SET_V1", false)]
#[case("Main Comp", false)]
fn an_edit_composition_says_so(#[case] name: &str, #[case] edit: bool) {
    assert_eq!(is_edit_comp(name), edit);
}

#[rstest]
#[case("www.logosample.com", Some("url"))]
#[case("hello@example.com", Some("email"))]
#[case("+1 (555) 010-2030", Some("phone"))]
#[case("$99", Some("price"))]
#[case("12.05.2026", Some("date"))]
#[case("2026", Some("number"))]
#[case("SPEED", None)]
#[case("Sale ends soon", None)]
fn words_say_what_some_texts_are(#[case] words: &str, #[case] role: Option<&str>) {
    assert_eq!(worded_role(words), role);
}

#[rstest]
#[case("Title 01", Some("title"))]
#[case("Subtitle", Some("subtitle"))]
#[case("www", Some("url"))]
#[case("TEXT AND LOGO", None)]
#[case("@headline Big", Some("title"))]
fn names_say_what_some_texts_are(#[case] name: &str, #[case] role: Option<&str>) {
    assert_eq!(named_role(name), role);
}

/// `scene` with its variables made, and what was made, with no project
/// facts beyond `media`.
fn made(mut scene: Json, media: &[MediaFact]) -> (Json, Variables) {
    let names = HashMap::from([(None, "Main".to_owned())]);
    let known = Known {
        media,
        layer_names: &HashMap::new(),
        comp_names: &names,
        texts: &[],
    };
    let variables = make(&mut scene, &known);
    (scene, variables)
}

#[test]
fn texts_colours_media_and_length_become_slots() {
    let logo = MediaFact {
        item: 7,
        name: "logo.png".into(),
        stored: Some("/footage/logo.png".into()),
        key: "image",
        found: true,
        why: None,
    };
    let (scene, made) = made(
        json!({"nakkash":1,"size":"100x100","dur":"5s","bg":"#000000","layers":[
            {"id":"big","text":"SPEED","font":"Inter Variable 96","fill":[1,1,1,1]},
            {"id":"small","text":"www.example.com","font":"Inter Variable 20","fill":[1,1,1,1]},
            {"id":"bar","rect":"10x10","fill":[0.9,0.2,0.25,1]},
            {"id":"bar-2","rect":"10x10","fill":[0.9,0.21,0.26,0.5]},
            {"id":"logo","image":"footage/logo.png","box":[10,10],"\u{0}media":0}
        ]}),
        &[logo],
    );
    assert_eq!(
        (made.text, made.colors, made.media, made.timing),
        (2, 3, 1, 1)
    );
    let slots = &scene["slots"];
    assert_eq!(slots["title"]["text"], json!("SPEED"));
    assert_eq!(slots["title"]["role"], json!("title"));
    assert_eq!(slots["url"]["role"], json!("url"));
    assert_eq!(scene["layers"][0]["text"], json!("$title"));
    assert_eq!(scene["layers"][1]["text"], json!("$url"));
    // The two reds are one entry; the second keeps its difference.
    assert_eq!(scene["layers"][2]["fill"], json!("$accent"));
    let near = scene["layers"][3]["fill"].as_str().unwrap();
    assert!(
        near.starts_with("$accent ") && near.ends_with("/ 0.5"),
        "{near}"
    );
    assert_eq!(scene["bg"], json!("$background"));
    assert_eq!(slots["text"]["role"], json!("text"));
    assert_eq!(scene["layers"][4]["image"], json!("$logo"));
    assert!(scene["layers"][4].get("\u{0}media").is_none());
    assert_eq!(slots["logo"]["image"], json!("footage/logo.png"));
    assert_eq!(scene["dur"], json!("$duration"));
    assert_eq!(slots["duration"]["number"], json!(5));
}

#[test]
fn the_scene_draws_as_before() {
    let original = json!({"nakkash":1,"size":"100x100","dur":"5s","bg":"#102030","layers":[
        {"id":"a","rect":"10x10","fill":[0.9,0.2,0.25,1]},
        {"id":"b","rect":"10x10","fill":[0.88,0.22,0.27,1]},
        {"id":"c","rect":"10x10","fill":[0.1,0.4,0.95,0.3]},
        {"id":"s","shapes":[{"group":[{"rect":[5,5]},{"stroke":[0.1,0.41,0.95,1],"width":2}]}]}
    ]});
    let (scene, _) = made(original.clone(), &[]);
    let resolved = nakkash_format::slots::resolved(&scene).unwrap();
    for steps in [
        vec![
            Step::Key("layers".into()),
            Step::Index(1),
            Step::Key("fill".into()),
        ],
        vec![
            Step::Key("layers".into()),
            Step::Index(2),
            Step::Key("fill".into()),
        ],
    ] {
        let before = nakkash_format::color(at(&original, &steps).unwrap()).unwrap();
        let after = nakkash_format::color(at(&resolved, &steps).unwrap()).unwrap();
        assert!(
            before.iter().zip(after).all(|(x, y)| (x - y).abs() < 2e-6),
            "{before:?} {after:?}"
        );
    }
}

#[test]
fn copies_of_a_text_in_one_composition_share_its_slot() {
    let (scene, made) = made(
        json!({"nakkash":1,"size":"100x100","dur":"1s","layers":[
            {"id":"a","text":"Sale","font":"Inter Variable 40"},
            {"id":"b","text":"Sale","font":"Inter Variable 40"},
            {"id":"c","text":"Sale","font":"Inter Variable 20"}
        ]}),
        &[],
    );
    assert_eq!(made.text, 2);
    assert_eq!(scene["layers"][0]["text"], scene["layers"][1]["text"]);
    assert_ne!(scene["layers"][0]["text"], scene["layers"][2]["text"]);
}

#[test]
fn a_colour_control_is_the_entry_of_its_colour() {
    let (scene, _) = made(
        json!({"nakkash":1,"size":"100x100","dur":"1s",
            "slots":{"brand":{"color":[0.9,0.2,0.25,1],"label":"Brand"}},
            "layers":[{"id":"a","rect":"10x10","fill":"$brand"},{"id":"b","rect":"10x10","fill":[0.9,0.2,0.25,1]}]}),
        &[],
    );
    assert_eq!(scene["layers"][1]["fill"], json!("$brand"));
    assert_eq!(scene["slots"]["brand"]["label"], json!("Brand"));
    assert!(scene["slots"]["brand"]["role"].is_string());
}

#[test]
fn a_layer_left_out_is_no_slot() {
    let guide = MediaFact {
        item: 3,
        name: "guide.jpg".into(),
        stored: Some("/footage/guide.jpg".into()),
        key: "image",
        found: true,
        why: None,
    };
    let (scene, made) = made(
        json!({"nakkash":1,"size":"100x100","dur":"1s","layers":[
            {"id":"t","text":"Note","font":"Inter Variable 40","hidden":true},
            {"id":"g","image":"footage/guide.jpg","box":[10,10],"hidden":true,"\u{0}media":0}
        ]}),
        &[guide],
    );
    assert_eq!((made.text, made.media), (0, 0));
    assert_eq!(scene["layers"][0]["text"], "Note");
    assert_eq!(scene["layers"][1]["image"], "footage/guide.jpg");
    assert!(scene["layers"][1].get("\u{0}media").is_none());
}

#[test]
fn a_dollar_in_a_text_stays_text() {
    let (scene, _) = made(
        json!({"nakkash":1,"size":"100x100","dur":"1s","layers":[{"id":"p","text":"$$99","font":"Inter Variable 40"}]}),
        &[],
    );
    assert_eq!(scene["slots"]["price"]["text"], json!("$99"));
}

#[test]
fn a_fit_takes_the_size_measured_in_the_scenes_fonts() {
    let mut imported = crate::map::import(
        &crate::test_support::sample_project(),
        &crate::map::Options::default(),
    )
    .expect("the sample imports");
    let fits = |json: &Json| -> Vec<String> {
        json["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["fit"].as_str().map(str::to_owned))
            .collect()
    };
    // The sample's title as point text, which fits a size.
    let title = imported.json["layers"]
        .as_array_mut()
        .and_then(|layers| layers.iter_mut().find(|l| l["id"] == "title"))
        .and_then(Json::as_object_mut)
        .expect("the sample has a title");
    title.remove("box");
    title.insert("fit".into(), json!("wrap 1x1"));
    let mut asked = Vec::new();
    imported
        .measure_fits(|_, address| {
            asked.push(address.to_string());
            Some([12.341, 6.0])
        })
        .expect("the sizes are valid");
    assert!(!asked.is_empty(), "{:?}", fits(&imported.json));
    assert!(
        fits(&imported.json).iter().all(|f| f == "wrap 12.35x6"),
        "{:?}",
        fits(&imported.json)
    );
    // The scene read back holds the new size.
    assert!(format!("{:?}", imported.document.scene()).contains("12.35"));
}
