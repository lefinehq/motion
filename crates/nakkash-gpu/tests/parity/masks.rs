//! Layer masks: outlines as the rasteriser draws them, grown, shrunk,
//! feathered, inverted and combined by every mode, by the GPU and by the
//! CPU reference ([`combine_on_cpu`]), and a layer cut by the coverage the
//! GPU holds.

use nakkash_raster::{Accelerator, MaskCoverage, MaskMode, MaskStep, Pixels, combine_on_cpu};

use super::{Random, check, gpu, pattern};

const WIDTH: u16 = 211;
const HEIGHT: u16 = 157;

/// An outline the rasteriser drew in white: a disc with a soft edge at
/// `(cx, cy)`, speckled so single px differ, as premultiplied bytes.
fn drawn(cx: f32, cy: f32, radius: f32, random: &mut Random) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 * usize::from(WIDTH) * usize::from(HEIGHT));
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let d = (f32::from(x) - cx).hypot(f32::from(y) - cy);
            let inside = (radius - d + 0.5).clamp(0.0, 1.0);
            let a = (255.0 * inside * (0.6 + 0.4 * random.next())).round() as u8;
            bytes.extend_from_slice(&[a; 4]);
        }
    }
    bytes
}

/// A step that grows by `grow`, feathers by `sigma` and combines by `mode`.
fn step(grow: f32, sigma: [f32; 2], inverted: bool, mode: MaskMode) -> MaskStep {
    MaskStep {
        grow,
        sigma,
        inverted,
        opacity: 0.8,
        mode,
    }
}

/// `masks` combined from `start`, on the CPU and on the GPU, as pixels
/// whose every channel is the coverage.
fn combined(gpu: &nakkash_gpu::Gpu, start: f32, masks: &[(Vec<u8>, MaskStep)]) -> (Pixels, Pixels) {
    let mut kept = vec![start; usize::from(WIDTH) * usize::from(HEIGHT)];
    let mut held = None;
    for (bytes, step) in masks {
        let mask = MaskCoverage {
            drawn: bytes,
            step: *step,
        };
        combine_on_cpu(&mut kept, &mask, WIDTH, HEIGHT);
        held = Some(
            gpu.mask(held, start, &mask, WIDTH, HEIGHT)
                .expect("combined on the GPU"),
        );
    }
    let mut cpu = Pixels::transparent(WIDTH, HEIGHT);
    for (p, k) in cpu.data_mut().iter_mut().zip(&kept) {
        *p = [*k; 4];
    }
    let held = held.expect("at least one mask");
    (cpu, gpu.download(&held).expect("downloaded"))
}

#[test]
fn masks_combine_as_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let mut random = Random(61);
    let modes = [
        MaskMode::Add,
        MaskMode::Subtract,
        MaskMode::Intersect,
        MaskMode::Lighten,
        MaskMode::Darken,
        MaskMode::Difference,
    ];
    let ways = [
        ("plain", 0.0, [0.0, 0.0], false),
        ("grown", 4.0, [0.0, 0.0], false),
        ("shrunk", -3.0, [0.0, 0.0], true),
        ("grown far", 40.0, [0.0, 0.0], false),
        ("feathered", 0.0, [3.0, 3.0], false),
        ("feathered one way", 2.0, [9.0, 0.7], true),
        ("feathered wide", -5.0, [30.0, 30.0], false),
    ];
    for (name, grow, sigma, inverted) in ways {
        for (n, mode) in modes.into_iter().enumerate() {
            let first = drawn(80.0, 70.0, 45.0, &mut random);
            let second = drawn(130.0 + n as f32, 85.0, 50.0, &mut random);
            let start = match mode {
                MaskMode::Add | MaskMode::Lighten | MaskMode::Difference => 0.0,
                _ => 1.0,
            };
            let masks = [
                (first, step(grow, sigma, false, MaskMode::Add)),
                (second, step(grow, sigma, inverted, mode)),
            ];
            let (cpu, on_gpu) = combined(gpu, start, &masks);
            check("mask", &format!("{name} {mode:?}"), &cpu, &on_gpu);
        }
    }
}

#[test]
fn a_layer_cut_by_held_coverage_matches_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let mut random = Random(62);
    let masks = [(
        drawn(100.0, 80.0, 60.0, &mut random),
        step(3.0, [4.0, 4.0], false, MaskMode::Add),
    )];
    let (coverage, held_coverage) = combined(gpu, 0.0, &masks);
    let kept = gpu.upload(&held_coverage).expect("uploaded");
    let layer = pattern(WIDTH, HEIGHT, 63);
    let mut cpu = layer.clone();
    for (p, k) in cpu.data_mut().iter_mut().zip(coverage.data()) {
        *p = p.map(|c| c * k[3]);
    }
    let mut held = gpu.upload(&layer).expect("uploaded");
    gpu.cut_by(&mut held, &kept).expect("cut");
    check(
        "mask",
        "a layer cut",
        &cpu,
        &gpu.download(&held).expect("downloaded"),
    );
}
