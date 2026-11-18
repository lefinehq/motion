//! The float canvas's own operations: copies, laying, mattes, mixing,
//! cutting and moving.

use nakkash_raster::kurbo::Affine;
use nakkash_raster::{Accelerator, Amount, BLEND_MODES, BlendMode, MatteMode, Pixels, Share};
use rstest::rstest;

use super::{Random, check, gpu, pattern};

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
