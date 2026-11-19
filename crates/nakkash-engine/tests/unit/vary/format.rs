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

#[test]
fn a_group_keeps_its_whole_move_on_the_new_canvas() {
    let master = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let canvas = Rect::new(0.0, 0.0, 1080.0, 1920.0);
    // Rests at 800..1100 x 500..580, swings out to 300..1600 on its way:
    // inside the master's canvas on every side.
    let anchor = [950.0, 540.0];
    let path = Rect::new(300.0, 460.0, 1600.0, 620.0);
    let mut placed = (
        Rect::new(390.0, 920.0, 690.0, 1000.0),
        BTreeSet::new(),
        1.0,
        anchor,
    );
    keep_on_canvas(&mut placed, path, master, canvas);
    let (rect, _, scale, _) = placed;
    let landed = |x: f64, y: f64| {
        (
            rect.center().x + (x - anchor[0]) * scale,
            rect.center().y + (y - anchor[1]) * scale,
        )
    };
    let (x0, _) = landed(path.x0, path.y0);
    let (x1, _) = landed(path.x1, path.y1);
    let margin = 1080.0 * ON_CANVAS;
    assert!(
        x0 >= margin - 1e-9 && x1 <= 1080.0 - margin + 1e-9,
        "{x0} {x1} {scale}"
    );
    assert!(scale < 1.0, "{scale}");
}

#[test]
fn a_group_that_enters_from_an_edge_may_still_leave_by_it() {
    let master = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let canvas = Rect::new(0.0, 0.0, 1080.0, 1920.0);
    // Slides in from the left edge: its path touches it.
    let path = Rect::new(0.0, 500.0, 1100.0, 580.0);
    let before = (
        Rect::new(390.0, 920.0, 690.0, 1000.0),
        BTreeSet::new(),
        1.0,
        [950.0, 540.0],
    );
    let mut placed = before.clone();
    keep_on_canvas(&mut placed, path, master, canvas);
    // Its right end lands at 690 + 150 = 840, inside: nothing moves.
    assert_eq!(placed, before);
}

#[test]
fn a_move_that_peaks_between_samples_stays_on_the_new_canvas() {
    // 300 frames are sampled every third; the title darts right and back
    // at frame 31 alone, which no sample sees.
    let session: Session = r##"{"nakkash":1,"size":"1920x1080","fps":30,"dur":"10s","layers":[
        {"id":"bg","rect":"1920x1080","fill":"#101820"},
        {"id":"title","rect":"600x120","at":"center center","fill":"#FFFFFF",
         "anim":{"x":[{"t":"30f","v":960},{"t":"31f","v":1500},{"t":"32f","v":960}]}}]}"##
        .parse()
        .expect("a scene");
    let (layout, _) = layout(&session, 1080, 1920).expect("a layout");
    let title = layout.shown[1..]
        .iter()
        .find(|s| !s.hide.iter().any(|h| h == "title"))
        .expect("the title's group");
    // Its right edge at the peak, 1800 in the master, lands inside.
    let right = title.position[0] + (1800.0 - title.anchor[0]) * title.scale;
    assert!(right <= 1080.0, "{title:?} reaches {right}");
}

#[test]
fn a_cropped_background_is_not_cut_through_an_element() {
    // A 600 px window centred at 960 would cut the dot at 640..700.
    let dot = Rect::new(640.0, 500.0, 700.0, 560.0);
    let [x, y] = uncut([960.0, 540.0], (600.0, 1080.0), &[dot], (1920.0, 1080.0));
    let (x0, x1) = (x - 300.0, x + 300.0);
    assert!(x0 <= dot.x0 || x0 >= dot.x1, "{x}");
    assert!(x1 <= dot.x0 || x1 >= dot.x1, "{x}");
    assert!((x - 960.0).abs() <= 150.0 + 1e-9, "{x}");
    assert_eq!(y, 540.0);
    // A centre that cuts nothing stays.
    assert_eq!(
        uncut([960.0, 540.0], (600.0, 1080.0), &[], (1920.0, 1080.0))[0],
        960.0
    );
}
