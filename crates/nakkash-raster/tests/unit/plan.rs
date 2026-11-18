use super::*;
use crate::effect::EFFECTS;
use vello_cpu::kurbo::Affine;

fn space(scale: f64) -> Space {
    Space {
        transform: Affine::scale(scale),
        bounds: None,
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
    }
}

#[test]
fn every_plan_names_an_effect_once() {
    for (i, row) in PLANS.iter().enumerate() {
        assert!(
            EFFECTS.iter().any(|e| e.name == row.name),
            "{} is not an effect",
            row.name
        );
        assert!(
            PLANS[..i].iter().all(|other| other.name != row.name),
            "{} has two plans",
            row.name
        );
    }
}

#[test]
fn a_gaussian_blur_plans_three_passes_each_way_in_grid_px() {
    let effect = Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(10.0))])
        .expect("an effect");
    let Some(Plan::Blur(blur)) = effect.plan(&space(2.0)) else {
        panic!("a blur");
    };
    // σ = 10 · 0.3 · 2 = 6 px.
    assert_eq!(blur.rows, blur.columns);
    assert_eq!(blur.rows.count, 3);
    assert_eq!(blur.rows.radius, gaussian_radius(6.0));
    assert_eq!(blur.edges, Edges::Transparent);
}

#[test]
fn a_blur_along_one_way_plans_no_passes_along_the_other() {
    let effect = Effect::new(
        "box-blur",
        &[
            ("radius", ParamValue::Number(4.0)),
            ("dimensions", ParamValue::Choice(1)),
        ],
    )
    .expect("an effect");
    let Some(Plan::Blur(blur)) = effect.plan(&space(1.0)) else {
        panic!("a blur");
    };
    assert!(blur.rows.any());
    assert!(!blur.columns.any());
}

#[test]
fn an_effect_without_a_row_has_no_plan() {
    let effect = Effect::new("invert", &[]).expect("an effect");
    assert!(!effect.has_plan());
    assert_eq!(effect.plan(&space(1.0)), None);
}
