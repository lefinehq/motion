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
fn an_effect_without_a_kernel_stays_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let spec = nakkash_raster::EFFECTS
        .iter()
        .find(|spec| {
            let effect = Effect::new(spec.name, &[]).expect("an effect");
            effect.movement().is_none() && !effect.has_plan()
        })
        .expect("an effect without a plan");
    let effect = Effect::new(spec.name, &[]).expect("an effect");
    assert!(!gpu.runs(&effect), "{}", spec.name);
}

#[test]
fn an_effect_that_reads_another_layer_without_a_kernel_for_it_stays_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    // The fill reads no layer: one handed to it is left to the CPU.
    let effect =
        Effect::new("fill", &[])
            .expect("an effect")
            .with_input(nakkash_raster::Node::Layer(nakkash_raster::Layer::new(
                Vec::new(),
            )));
    assert!(!gpu.runs(&effect));
}

#[test]
fn a_blur_leaves_transparent_what_it_does_not_reach_as_the_cpu_does() {
    let Some(gpu) = gpu() else { return };
    // Two squares, one over the other, on a transparent grid: a running
    // sum in f32 would leave its rounding where the blur does not reach,
    // and an effect that reads straight colour would make any colour of it.
    let (width, height) = (160u16, 120u16);
    let mut pixels = Pixels::transparent(width, height);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = (i % usize::from(width), i / usize::from(width));
        if (10..70).contains(&x) && (10..70).contains(&y) {
            *p = [0.9, 0.2, 0.1, 1.0];
        }
        if (50..120).contains(&x) && (40..110).contains(&y) {
            *p = std::array::from_fn(|c| [0.08, 0.48, 0.72, 0.8][c] + p[c] * 0.2);
        }
    }
    for blurriness in [6.0, 40.0] {
        let effect = Effect::new(
            "gaussian-blur",
            &[("blurriness", ParamValue::Number(blurriness))],
        )
        .expect("a blur");
        let mut cpu = pixels.clone();
        effect.apply(&mut cpu, &space(Affine::IDENTITY));
        let mut held = gpu.upload(&pixels).expect("uploaded");
        gpu.effect(&mut held, &effect, &space(Affine::IDENTITY))
            .expect("ran");
        let on_gpu = gpu.download(&held).expect("downloaded");
        let zeros = |p: &Pixels| p.data().iter().filter(|p| p[3] == 0.0).count();
        assert_eq!(zeros(&on_gpu), zeros(&cpu), "blurriness {blurriness}");
        for (c, g) in cpu.data().iter().zip(on_gpu.data()) {
            if c[3] == 0.0 {
                assert_eq!(g, &[0.0; 4], "blurriness {blurriness}");
            }
        }
        check(
            "gaussian-blur",
            &format!("two squares, {blurriness}"),
            &cpu,
            &on_gpu,
        );
    }
}
