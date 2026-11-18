//! The effects with kernels of their own: the blurs, the glow and the
//! drop shadow, and the moving effects.

use super::*;
use rstest::rstest;

#[rstest]
#[case::small(1.5, 0, 0, (1.0, 1.0))]
#[case::medium(12.0, 0, 1, (1.0, 1.0))]
#[case::rows_only(30.0, 1, 0, (1.0, 1.0))]
#[case::columns_only(30.0, 2, 1, (1.0, 1.0))]
#[case::scaled(40.0, 0, 0, (2.5, 0.8))]
#[case::huge(900.0, 0, 0, (1.0, 1.0))]
fn gaussian_blurs_match(
    #[case] blurriness: f32,
    #[case] dimensions: u8,
    #[case] edges: u8,
    #[case] scale: (f64, f64),
) {
    let effect = Effect::new(
        "gaussian-blur",
        &[
            ("blurriness", ParamValue::Number(blurriness)),
            ("dimensions", ParamValue::Choice(dimensions)),
            ("edges", ParamValue::Choice(edges)),
        ],
    )
    .expect("an effect");
    let space = space(Affine::scale_non_uniform(scale.0, scale.1));
    let case = format!("{blurriness} {dimensions} {edges} {scale:?}");
    effect_matches(
        "gaussian-blur",
        &case,
        &effect,
        &pattern(301, 167, 16),
        &space,
    );
}

#[rstest]
#[case::one(3.0, 1.0, 0, 0)]
#[case::three(7.4, 3.0, 0, 1)]
#[case::many(2.2, 9.0, 1, 0)]
#[case::wide(120.0, 2.0, 0, 1)]
fn box_blurs_match(
    #[case] radius: f32,
    #[case] iterations: f32,
    #[case] dimensions: u8,
    #[case] edges: u8,
) {
    let effect = Effect::new(
        "box-blur",
        &[
            ("radius", ParamValue::Number(radius)),
            ("iterations", ParamValue::Number(iterations)),
            ("dimensions", ParamValue::Choice(dimensions)),
            ("edges", ParamValue::Choice(edges)),
        ],
    )
    .expect("an effect");
    let case = format!("{radius} {iterations} {dimensions} {edges}");
    effect_matches(
        "box-blur",
        &case,
        &effect,
        &pattern(233, 149, 17),
        &space(Affine::IDENTITY),
    );
}

#[rstest]
#[case::added(0.4, 25.0, 1.5, 0)]
#[case::alone(0.2, 60.0, 3.0, 1)]
#[case::dim(0.9, 8.0, 0.5, 0)]
#[case::behind(0.4, 25.0, 1.5, 2)]
fn glows_match(
    #[case] threshold: f32,
    #[case] radius: f32,
    #[case] intensity: f32,
    #[case] composite: u8,
) {
    let effect = Effect::new(
        "glow",
        &[
            ("threshold", ParamValue::Number(threshold)),
            ("radius", ParamValue::Number(radius)),
            ("intensity", ParamValue::Number(intensity)),
            ("composite", ParamValue::Choice(composite)),
        ],
    )
    .expect("an effect");
    let case = format!("{threshold} {radius} {intensity} {composite}");
    effect_matches(
        "glow",
        &case,
        &effect,
        &pattern(256, 160, 18),
        &space(Affine::scale(1.3)),
    );
}

#[rstest]
#[case::behind(135.0, 12.0, 20.0, 0)]
#[case::alone(40.0, 30.5, 5.0, 1)]
fn drop_shadows_match(
    #[case] direction: f32,
    #[case] distance: f32,
    #[case] softness: f32,
    #[case] layer: u8,
) {
    let effect = Effect::new(
        "drop-shadow",
        &[
            ("color", ParamValue::Color(Color::rgba(0.2, 0.1, 0.4, 1.0))),
            ("opacity", ParamValue::Number(0.75)),
            ("direction", ParamValue::Number(direction)),
            ("distance", ParamValue::Number(distance)),
            ("softness", ParamValue::Number(softness)),
            ("layer", ParamValue::Choice(layer)),
        ],
    )
    .expect("an effect");
    let case = format!("{direction} {distance} {softness} {layer}");
    effect_matches(
        "drop-shadow",
        &case,
        &effect,
        &pattern(220, 140, 19),
        &space(Affine::rotate(0.2).then_scale(1.4)),
    );
}

#[test]
fn a_moving_effect_moves_as_the_cpu_does() {
    let effect = Effect::new(
        "transform",
        &[
            ("position", ParamValue::Point([30.0, 12.0])),
            ("scale", ParamValue::Point([1.2, 0.9])),
            ("rotate", ParamValue::Number(15.0)),
            ("opacity", ParamValue::Number(0.6)),
        ],
    )
    .expect("an effect");
    effect_matches(
        "transform",
        "moved and faded",
        &effect,
        &pattern(190, 120, 20),
        &space(Affine::scale(1.1)),
    );
}

#[test]
fn an_effect_that_reads_another_layer_stays_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let effect = Effect::new("invert", &[]).expect("an effect");
    assert!(!gpu.runs(&effect));
}
