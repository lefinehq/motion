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
