use super::*;

#[test]
fn a_measure_in_px_is_read_at_the_masters_scale() {
    assert_eq!(
        unscaled(Measure::Overshoot(10.0), 0.5),
        Measure::Overshoot(19.0)
    );
    let ratio = |value| Measure::Below {
        value,
        limit: 4.5,
        unit: Unit::Ratio,
    };
    let Measure::Below { value, .. } = unscaled(ratio(1.2), 0.5) else {
        panic!("a ratio stays a ratio");
    };
    assert!((value - 1.3).abs() < 1e-9);
}

#[test]
fn the_same_spec_plans_the_same_recipes() {
    let master: Session = r##"{"nakkash":1,"size":"1920x1080","dur":"1s","layers":[
        {"id":"title","text":"Hello","font":"Inter Variable 96 700","fill":"#E63946"}]}"##
        .parse()
        .expect("a scene");
    let spec = Spec {
        seed: 3,
        count: Some(3),
        palette: Some(PaletteSpec::default()),
        format: Some(FormatSpec {
            sizes: vec!["9:16".into(), "1:1".into()],
        }),
        ..Spec::default()
    };
    let a = plan(&master, &spec, Path::new("")).expect("a plan");
    let b = plan(&master, &spec, Path::new("")).expect("a plan");
    assert_eq!(a, b);
    assert_eq!(a.len(), 3);
    assert!(matches!(a[0].steps[1], Step::Format { width: 1080, .. }));
}

/// A 16 by 16 picture: a bar `[x0, x1)` wide in `ink` on `ground`.
fn bar(x0: usize, x1: usize, ink: [u8; 3], ground: [u8; 3]) -> crate::Image {
    let px: Vec<u8> = (0..16 * 16)
        .flat_map(|i| {
            let c = if (x0..x1).contains(&(i % 16)) {
                ink
            } else {
                ground
            };
            [c[0], c[1], c[2], 255]
        })
        .collect();
    crate::Image::new(16, 16, px).expect("image")
}

#[test]
fn structure_reads_shapes_and_not_their_colours() {
    let master = [bar(4, 8, [255, 255, 255], [0, 0, 0])];
    // Recoloured, or inverted, it is the same drawing.
    let recoloured = distance(&master, &[bar(4, 8, [255, 40, 40], [10, 0, 40])]);
    let inverted = distance(&master, &[bar(4, 8, [0, 0, 0], [255, 255, 255])]);
    assert!(recoloured.structure.unwrap() < 0.01, "{recoloured:?}");
    assert!(inverted.structure.unwrap() < 0.01, "{inverted:?}");
    assert!(recoloured.palette > 0.05);
    // A bar elsewhere and wider is another drawing.
    let moved = distance(&master, &[bar(9, 15, [255, 255, 255], [0, 0, 0])]);
    assert!(moved.structure.unwrap() > 0.5, "{moved:?}");
    assert!(moved.palette < 0.01, "{moved:?}");
}

#[test]
fn a_filter_leaves_no_residual_and_a_design_does() {
    let master = [bar(4, 8, [255, 255, 255], [0, 0, 0])];
    // Any one colour for each master colour is a filter, however far.
    let mapped = [bar(4, 8, [255, 40, 40], [10, 0, 40])];
    assert!(filter(&master, &mapped) < 1e-9);
    assert!(filter(&master, &master) < 1e-9);
    assert!(filter_failure(filter(&master, &mapped)).is_some());
    // Two colours where the master has one: the ground split into a
    // surface and a field no transform of it can draw.
    let mut px = mapped[0].pixels().to_vec();
    for row in 0..8 {
        for x in [0, 1, 2, 9, 10, 11, 12, 13, 14, 15] {
            px[(row * 16 + x) * 4..][..3].copy_from_slice(&[40, 200, 190]);
        }
    }
    let designed = [crate::Image::new(16, 16, px).expect("image")];
    let residual = filter(&master, &designed);
    assert!(residual > FILTER_FLOOR, "{residual}");
    assert!(filter_failure(residual).is_none());
}

#[test]
fn a_recipe_without_a_look_has_no_colour_twin() {
    let master: Session = r##"{"nakkash":1,"size":"64x64","dur":"1s","layers":[
        {"id":"title","text":"Hi","font":"Inter Variable 24 700","fill":"#E63946"}]}"##
        .parse()
        .expect("a scene");
    let recipe = Recipe {
        id: "v1".into(),
        seed: 1,
        steps: Vec::new(),
    };
    assert!(!recipe.has_look());
    let twin = colour_twin(&master, &recipe, Path::new("")).expect("made");
    assert!(twin.is_none());
}
