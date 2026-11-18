use super::*;

#[test]
fn an_aspect_takes_the_masters_short_side() {
    assert_eq!(parse_size("9:16", 1080).unwrap(), (1080, 1920));
    assert_eq!(parse_size("4:5", 1080).unwrap(), (1080, 1350));
    assert_eq!(parse_size("16:9", 1080).unwrap(), (1920, 1080));
    assert_eq!(parse_size("1080x1350", 720).unwrap(), (1080, 1350));
    assert!(parse_size("tall", 1080).is_err());
}

#[test]
fn a_wide_title_and_a_corner_logo_are_placed_apart_inside_the_new_safe_area() {
    let session: Session = r##"{"nakkash":1,"size":"1920x1080","dur":"1s","layers":[
        {"id":"bg","rect":"1920x1080","fill":"#101820"},
        {"id":"title","rect":"1400x200","at":"center center","fill":"#FFFFFF"},
        {"id":"logo","rect":"200x100","at":"1750 120","fill":"#E63946"}]}"##
        .parse()
        .expect("a scene");
    let (layout, _) = layout(&session, 1080, 1920).expect("a layout");
    // The background, then the title and the logo, each its own group.
    assert_eq!(layout.shown.len(), 3, "{layout:?}");
    let safe = Rect::new(54.0, 96.0, 1026.0, 1824.0);
    for shown in &layout.shown[1..] {
        assert!(shown.scale <= 1.0 + 1e-9);
        let [x, y] = shown.position;
        assert!(safe.contains(kurbo::Point::new(x, y)), "{shown:?}");
    }
    // The 1400 px title is set to fit the 972 px safe width.
    let title = layout.shown[1..]
        .iter()
        .find(|s| !s.hide.iter().any(|h| h == "title"))
        .expect("the title's group");
    assert!((title.scale - 972.0 / 1400.0).abs() < 1e-6, "{title:?}");
    // The background covers the portrait canvas, a hair past its edges.
    let cover = layout.shown[0].scale;
    assert!((cover - 1920.0 / 1080.0 * 1.004).abs() < 1e-6, "{cover}");
}
