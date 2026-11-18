//! The GPU against the CPU reference ([D74]): every kernel runs on the same
//! pixels as the CPU function it ports, and the largest difference of any
//! channel of any pixel must stay within the kernel's tolerance in D74's
//! table, [`TOLERANCES`] here.
//!
//! On a machine without a hardware GPU (CI's Linux runners), every test
//! says so and passes without checking anything: the CPU path is the one
//! CI holds to its goldens.
//!
//! `cargo test -p nakkash-gpu --test parity -- --nocapture` prints each
//! measured difference.
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

use std::sync::OnceLock;

use nakkash_gpu::Gpu;
use nakkash_raster::kurbo::{Affine, Rect};
use nakkash_raster::{
    Accelerator, Amount, BLEND_MODES, BlendMode, Color, Effect, MatteMode, ParamValue, Pixels,
    Share, Space,
};
use rstest::rstest;

/// The largest difference allowed in any channel of any pixel, on values
/// from 0 to 1, by what runs: D74's tolerance table.
const TOLERANCES: &[(&str, f32)] = &[
    ("upload", 0.0),
    ("upload-bytes", 0.0),
    ("cut", 0.0),
    ("mix", 1e-6),
    ("lay", 1e-5),
    ("lay-matte", 1e-5),
    ("transform", 1e-4),
    ("gaussian-blur", 1e-5),
    ("box-blur", 1e-5),
    ("glow", 1e-5),
    ("drop-shadow", 1e-4),
];

fn tolerance(name: &str) -> f32 {
    TOLERANCES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("{name} has a tolerance"))
}

/// The GPU, opened once for every test; `None` where there is no hardware
/// adapter.
fn gpu() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(|| match Gpu::open() {
        Ok(gpu) => {
            eprintln!("GPU: {}", gpu.adapter());
            Some(gpu)
        }
        Err(refusal) => {
            eprintln!("skipped: {refusal}");
            None
        }
    })
    .as_ref()
}

/// A small deterministic random sequence, so failures reproduce.
struct Random(u64);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }
}

/// `width` by `height` premultiplied pixels with what effects meet in
/// practice: hard edges, a soft disc, a ramp, transparent areas and noise.
fn pattern(width: u16, height: u16, seed: u64) -> Pixels {
    let mut random = Random(seed);
    let (w, h) = (f32::from(width), f32::from(height));
    let mut data = Vec::with_capacity(usize::from(width) * usize::from(height));
    for y in 0..height {
        for x in 0..width {
            let (fx, fy) = (f32::from(x) / w, f32::from(y) / h);
            let disc = 1.0 - (((fx - 0.6).powi(2) + (fy - 0.4).powi(2)).sqrt() * 4.0).min(1.0);
            let rect = if fx > 0.1 && fx < 0.35 && fy > 0.2 && fy < 0.8 {
                1.0
            } else {
                0.0
            };
            let noise = random.next();
            let a = (disc + rect * 0.8 + noise * 0.15 * fy).min(1.0);
            let rgb = [fx, 1.0 - fy, (fx * fy * 7.0).fract()];
            data.push([rgb[0] * a, rgb[1] * a, rgb[2] * a, a]);
        }
    }
    Pixels::from_data(width, height, data)
}

/// The largest difference between two grids, over every channel.
fn largest_difference(a: &Pixels, b: &Pixels) -> f32 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    a.data()
        .iter()
        .zip(b.data())
        .flat_map(|(p, q)| p.iter().zip(q).map(|(x, y)| (x - y).abs()))
        .fold(0.0, f32::max)
}

fn check(name: &str, case: &str, cpu: &Pixels, gpu: &Pixels) {
    // A kernel must not make a NaN the CPU does not: one NaN spreads
    // through every blur and lay after it.
    let made_nan = cpu
        .data()
        .iter()
        .zip(gpu.data())
        .any(|(p, q)| p.iter().all(|c| !c.is_nan()) && q.iter().any(|c| c.is_nan()));
    assert!(
        !made_nan,
        "{name} {case}: the GPU made a NaN the CPU did not"
    );
    let difference = largest_difference(cpu, gpu);
    let allowed = tolerance(name);
    eprintln!("{name} {case}: {difference:e} (allowed {allowed:e})");
    assert!(
        difference <= allowed,
        "{name} {case}: the GPU differs by {difference}, more than {allowed}"
    );
}

fn space(transform: Affine) -> Space {
    Space {
        transform,
        bounds: Some(Rect::new(0.0, 0.0, 400.0, 300.0)),
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
    }
}

#[test]
fn pixels_go_there_and_back_unchanged() {
    let Some(gpu) = gpu() else { return };
    let pixels = pattern(257, 131, 1);
    let back = gpu
        .download(&gpu.upload(&pixels).expect("uploaded"))
        .expect("downloaded");
    check("upload", "257x131", &pixels, &back);
}

#[test]
fn bytes_become_the_floats_the_cpu_makes() {
    let Some(gpu) = gpu() else { return };
    let (width, height) = (61u16, 17u16);
    let mut random = Random(2);
    let bytes: Vec<u8> = (0..usize::from(width) * usize::from(height) * 4)
        .map(|_| (random.next() * 256.0) as u8)
        .collect();
    let expected = Pixels::from_data(
        width,
        height,
        bytes
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]].map(|c| f32::from(c) / 255.0))
            .collect(),
    );
    let held = gpu.upload_bytes(&bytes, width, height).expect("uploaded");
    check(
        "upload-bytes",
        "61x17",
        &expected,
        &gpu.download(&held).expect("downloaded"),
    );
}

#[test]
fn a_cut_multiplies_as_the_cpu_does() {
    let Some(gpu) = gpu() else { return };
    let pixels = pattern(200, 90, 3);
    let mut random = Random(4);
    let kept: Vec<f32> = (0..pixels.data().len()).map(|_| random.next()).collect();
    let mut cpu = pixels.clone();
    for (p, k) in cpu.data_mut().iter_mut().zip(&kept) {
        *p = p.map(|c| c * k);
    }
    let mut held = gpu.upload(&pixels).expect("uploaded");
    gpu.cut(&mut held, &kept).expect("cut");
    check(
        "cut",
        "random",
        &cpu,
        &gpu.download(&held).expect("downloaded"),
    );
}

#[test]
fn every_blend_mode_lays_as_the_cpu_does() {
    let Some(gpu) = gpu() else { return };
    let below = pattern(160, 120, 5);
    let source = pattern(160, 120, 6).transformed(Affine::translate((23.0, -11.0)));
    let mut random = Random(7);
    let coverage: Vec<f32> = (0..below.data().len())
        .map(|_| random.next() * 1.2)
        .collect();
    for spec in BLEND_MODES {
        for shared in [false, true] {
            let amount = match shared {
                true => Amount::constant(0.7).and(Share::Values(&coverage)),
                false => Amount::constant(0.85),
            };
            let mut cpu = below.clone();
            let at = |i: usize| match shared {
                true => 0.7 * coverage[i],
                false => 0.85,
            };
            cpu.lay(&source, spec.mode, at);
            let mut canvas = gpu.upload(&below).expect("uploaded");
            let laid = gpu.upload(&source).expect("uploaded");
            gpu.lay(&mut canvas, &laid, spec.mode, &amount)
                .expect("laid");
            let case = format!("{} shared={shared}", spec.name);
            check(
                "lay",
                &case,
                &cpu,
                &gpu.download(&canvas).expect("downloaded"),
            );
        }
    }
}

#[rstest]
#[case::alpha(MatteMode::Alpha)]
#[case::alpha_inverted(MatteMode::AlphaInverted)]
#[case::luma(MatteMode::Luma)]
#[case::luma_inverted(MatteMode::LumaInverted)]
fn a_matte_read_on_the_gpu_lays_as_one_read_on_the_cpu(#[case] mode: MatteMode) {
    let Some(gpu) = gpu() else { return };
    let below = pattern(140, 70, 8);
    let source = pattern(140, 70, 9).transformed(Affine::rotate(0.3));
    let matte = pattern(140, 70, 10).transformed(Affine::scale(0.7));
    // As the painter reads a matte on the CPU.
    let values: Vec<f32> = matte
        .data()
        .iter()
        .map(|&[r, g, b, a]| {
            let luma = (r * 0.2126 + g * 0.7152 + b * 0.0722).clamp(0.0, 1.0);
            let a = a.clamp(0.0, 1.0);
            match mode {
                MatteMode::Alpha => a,
                MatteMode::AlphaInverted => 1.0 - a,
                MatteMode::Luma => luma,
                MatteMode::LumaInverted => 1.0 - luma,
            }
        })
        .collect();
    let mut cpu = below.clone();
    cpu.lay(&source, BlendMode::Screen, |i| 0.9 * values[i]);
    let mut canvas = gpu.upload(&below).expect("uploaded");
    let laid = gpu.upload(&source).expect("uploaded");
    let held_matte = gpu.upload(&matte).expect("uploaded");
    let amount = Amount::constant(0.9).and(Share::Matte(&held_matte, mode));
    gpu.lay(&mut canvas, &laid, BlendMode::Screen, &amount)
        .expect("laid");
    check(
        "lay-matte",
        &format!("{mode:?}"),
        &cpu,
        &gpu.download(&canvas).expect("downloaded"),
    );
}

#[test]
fn a_filter_mix_by_a_region_matches() {
    let Some(gpu) = gpu() else { return };
    let drawn = pattern(150, 100, 11);
    let changed = pattern(150, 100, 12);
    let region = pattern(150, 100, 13);
    let mut random = Random(14);
    let cut: Vec<f32> = (0..drawn.data().len()).map(|_| random.next()).collect();
    let mut cpu = drawn.clone();
    cpu.mix(&changed, |i| cut[i] * 0.6 * region.data()[i][3]);
    let mut held = gpu.upload(&drawn).expect("uploaded");
    let other = gpu.upload(&changed).expect("uploaded");
    let held_region = gpu.upload(&region).expect("uploaded");
    let amount = Amount::constant(0.6)
        .and(Share::Values(&cut))
        .and(Share::Alpha(&held_region));
    gpu.mix(&mut held, &other, &amount).expect("mixed");
    check(
        "mix",
        "cut and region",
        &cpu,
        &gpu.download(&held).expect("downloaded"),
    );
}

#[rstest]
#[case::shift(Affine::translate((13.25, -7.5)))]
#[case::turn(Affine::rotate_about(0.4, nakkash_raster::kurbo::Point::new(80.0, 50.0)))]
#[case::scale(Affine::scale_non_uniform(1.7, 0.6))]
fn moved_pixels_match(#[case] transform: Affine) {
    let Some(gpu) = gpu() else { return };
    let pixels = pattern(180, 110, 15);
    let cpu = pixels.transformed(transform);
    let held = gpu.upload(&pixels).expect("uploaded");
    let moved = gpu.transformed(&held, transform).expect("moved");
    check(
        "transform",
        &format!("{:?}", transform.as_coeffs()),
        &cpu,
        &gpu.download(&moved).expect("downloaded"),
    );
}

/// Runs `effect` on `pixels` on both, in `space`, and checks them.
fn effect_matches(name: &str, case: &str, effect: &Effect, pixels: &Pixels, space: &Space) {
    let Some(gpu) = gpu() else { return };
    assert!(gpu.runs(effect), "the GPU runs {name}");
    let mut cpu = pixels.clone();
    effect.apply(&mut cpu, space);
    let mut held = gpu.upload(pixels).expect("uploaded");
    gpu.effect(&mut held, effect, space).expect("ran");
    check(name, case, &cpu, &gpu.download(&held).expect("downloaded"));
}

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

#[test]
fn a_glow_combined_by_an_operation_other_than_add_stays_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    // Operations by their row in the glow's menu: 0 add, 2 multiply;
    // composites 0 with the original, 1 the light alone, 2 behind it.
    let glow = |operation: u8, composite: u8| {
        Effect::new(
            "glow",
            &[
                ("operation", ParamValue::Choice(operation)),
                ("composite", ParamValue::Choice(composite)),
            ],
        )
        .expect("a glow")
    };
    assert!(gpu.runs(&glow(0, 0)));
    assert!(gpu.runs(&glow(0, 2)));
    assert!(gpu.runs(&glow(2, 1)));
    assert!(!gpu.runs(&glow(2, 0)));
    assert!(!gpu.runs(&glow(2, 2)));
}
