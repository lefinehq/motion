//! How long the GPU's steps take on a 4K grid, against the CPU's own
//! functions: `cargo test --release -p nakkash-gpu --test timing --
//! --ignored --nocapture`. A timing, not a check.

use std::time::Instant;

use nakkash_gpu::Gpu;
use nakkash_raster::kurbo::Affine;
use nakkash_raster::{Accelerator, Amount, BlendMode, Effect, ParamValue, Pixels, Space};

fn space() -> Space {
    Space {
        transform: Affine::IDENTITY,
        bounds: None,
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
    }
}

/// The fastest of five runs of `f`, in milliseconds.
fn best(mut f: impl FnMut()) -> f64 {
    (0..5)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
#[ignore = "a timing, not a check"]
fn steps_on_a_4k_grid() {
    let Ok(gpu) = Gpu::open() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let (w, h) = (3840u16, 2160u16);
    let data = (0..usize::from(w) * usize::from(h))
        .map(|i| {
            let a = ((i % 977) as f32 / 977.0).min(1.0);
            [a * 0.8, a * 0.5, a * 0.2, a]
        })
        .collect();
    let pixels = Pixels::from_data(w, h, data);
    let bytes = vec![128u8; usize::from(w) * usize::from(h) * 4];
    let held = gpu.upload(&pixels).unwrap();
    eprintln!(
        "upload {:.1} ms",
        best(|| drop(gpu.upload(&pixels).unwrap()))
    );
    eprintln!(
        "upload bytes {:.1} ms",
        best(|| drop(gpu.upload_bytes(&bytes, w, h).unwrap()))
    );
    // A download waits for every step submitted before it, so a step's
    // time is its time with a download less the download's own.
    let download = best(|| drop(gpu.download(&held).unwrap()));
    eprintln!("download {download:.1} ms");
    let mut canvas = gpu.upload(&pixels).unwrap();
    let lay = best(|| {
        gpu.lay(
            &mut canvas,
            &held,
            BlendMode::Screen,
            &Amount::constant(0.8),
        )
        .unwrap();
        drop(gpu.download(&canvas).unwrap());
    }) - download;
    eprintln!("lay screen {lay:.1} ms");
    for (name, params) in [
        (
            "gaussian-blur",
            vec![("blurriness", ParamValue::Number(50.0))],
        ),
        ("glow", vec![("radius", ParamValue::Number(80.0))]),
    ] {
        let effect = Effect::new(name, &params).unwrap();
        let mut on = gpu.upload(&pixels).unwrap();
        let gpu_ms = best(|| {
            gpu.effect(&mut on, &effect, &space()).unwrap();
            drop(gpu.download(&on).unwrap());
        }) - download;
        let mut cpu = pixels.clone();
        let start = Instant::now();
        effect.apply(&mut cpu, &space());
        let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("{name}: GPU {gpu_ms:.1} ms, CPU {cpu_ms:.1} ms");
    }
}
