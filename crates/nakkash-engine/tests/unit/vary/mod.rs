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
