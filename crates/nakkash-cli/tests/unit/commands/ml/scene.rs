use serde_json::json;

use super::*;

/// A scene with a video layer asking for a cutout and a track at the top,
/// one inside a composition shown twice, and a plain video.
fn scene() -> Session {
    let prompts = json!([{"frame": 2, "box": [10, 20, 300, 400], "include": [[50, 60]]}]);
    json!({
        "nakkash": 1, "dur": "1s",
        "comps": {"card": {"size": "1080p", "dur": "1s", "layers": [
            {"id": "phone", "video": "phone.mov", "box": [1920, 1080], "tracks": [{"id": "screen", "prompts": prompts}]}
        ]}},
        "layers": [
            {"id": "a", "comp": "card"},
            {"id": "b", "comp": "card"},
            {"id": "plain", "video": "plain.mov", "box": [1920, 1080]},
            {"id": "hero", "video": "clips/hero.mov", "box": [1920, 1080],
             "cutout": {"prompts": prompts, "edges": "soft", "smooth": {"amount": 0.5}}}
        ]
    })
    .to_string()
    .parse()
    .unwrap_or_else(|e: Diagnostic| panic!("{e}"))
}

#[test]
fn finds_each_layer_that_asks_once() {
    let session = scene();
    let found = asking(&session, None).unwrap();
    let addresses: Vec<String> = found.iter().map(|a| a.address.to_string()).collect();
    assert_eq!(addresses, ["a/phone", "hero"]);
    assert_eq!(found[0].list.as_deref(), Some("card"));
    assert_eq!(found[1].list, None);
    assert_eq!(found[1].clip, Path::new("clips/hero.mov"));
    let one = asking(&session, Some(&"hero".parse().unwrap())).unwrap();
    assert_eq!(one.len(), 1);
    let err = asking(&session, Some(&"plain".parse().unwrap()))
        .err()
        .unwrap();
    assert_eq!(err.kind, Kind::Missing);
}

#[test]
fn hands_the_requests_to_the_bake_as_written() {
    let session = scene();
    let found = asking(&session, None).unwrap();
    let c = cutout(found[1].video.cutout.as_ref().unwrap());
    assert_eq!(
        c.prompts,
        [Prompt {
            frame: 2,
            area: Some([10.0, 20.0, 300.0, 400.0]),
            include: vec![[50.0, 60.0]],
            exclude: vec![]
        }]
    );
    assert_eq!(c.edges, matte::Edges::Soft);
    assert_eq!(c.smooth.map(|s| s.0), Some(0.5));
    assert_eq!(c.request, found[1].video.cutout.as_ref().unwrap().request());
    let t = track_bake(&found[0].video.tracks[0]);
    assert_eq!(t.spec.prompt_frame, 2);
    assert_eq!(t.spec.corners, None);
    assert_eq!(t.prompts[0].frame, 2);
}

#[test]
fn finds_a_layers_json_in_its_list() {
    let session = scene();
    let mut raw = session.document().raw().clone();
    assert_eq!(
        layer_mut(&mut raw, &None, "hero").unwrap()["video"],
        "clips/hero.mov"
    );
    assert_eq!(
        layer_mut(&mut raw, &Some("card".into()), "phone").unwrap()["video"],
        "phone.mov"
    );
    assert_eq!(
        layer_mut(&mut raw, &None, "phone").unwrap_err().kind,
        Kind::Missing
    );
}

#[test]
fn names_a_sheet_per_request_beside_the_one_asked_for() {
    assert_eq!(
        beside(Path::new("out/sheet.png"), "hero-cutout"),
        Path::new("out/sheet-hero-cutout.png")
    );
    assert_eq!(beside(Path::new("look"), "a-b"), Path::new("look-a-b.png"));
}

#[test]
fn applies_only_a_track_address() {
    let err = apply(
        Path::new("missing.json"),
        &"hero.cutout".parse().unwrap(),
        "p",
        false,
    )
    .unwrap_err();
    assert_eq!(err.kind, Kind::Invalid);
    assert!(err.message.contains("names no track"), "{err}");
}
