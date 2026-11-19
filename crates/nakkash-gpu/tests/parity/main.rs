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

use std::sync::{Arc, OnceLock};

mod blurs;
mod canvas;
mod channel;
mod colour;
mod composite;
mod distort;
mod effects;
mod frames;
mod generate;
mod masks;
mod pins;
mod planes;

use nakkash_gpu::Gpu;
use nakkash_raster::kurbo::{Affine, Point, Rect};
use nakkash_raster::{Accelerator, Color, Drawing, Effect, Input, ParamValue, Pixels, Space};

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
    ("glow-dissolve", 1e-4),
    ("solid-composite", 5e-5),
    ("drop-shadow", 1e-4),
    ("chain", 5e-5),
    ("plane", 1e-4),
    // A corner pin's box filter: a probe's count and its copy are whole
    // numbers chosen from a float, so a px on the edge of a choice reads
    // a neighbouring grid of probes.
    ("pin", 1e-4),
    ("mask", 1e-5),
    // The colour effects divide by alpha as the CPU rounds it, so most
    // differ by a last bit or two of their result.
    ("curves", 1e-6),
    ("tint", 1e-6),
    ("hue-saturation", 1e-6),
    ("invert", 1e-6),
    ("levels", 1e-6),
    // A range of no width: a threshold, which a level a bit off would
    // send the other way.
    ("levels-threshold", 1e-6),
    ("black-white", 1e-6),
    ("fill", 1e-6),
    ("sharpen", 1e-6),
    ("find-edges", 1e-6),
    // Powers are the GPU's own, less exact than the CPU's.
    ("exposure", 1e-5),
    ("shift-channels", 1e-6),
    // A hue jumps from 1 back to 0 at red, so a bit there would be a jump.
    ("shift-channels-hue", 1e-6),
    // The matte is resampled at points worked out in f32, as moving pixels
    // are.
    ("set-matte", 1e-4),
    ("color-cycle", 1e-5),
    // Held at each point's level, a cycle steps, so a phase a bit off at a
    // point would be a step.
    ("color-cycle-held", 5e-6),
    // The distortions read the pattern's hard edges at points worked out
    // in 32-bit floats, a few hundred thousandths of a px from the CPU's.
    ("motion-tile", 2e-4),
    ("wrap", 5e-5),
    ("mirror", 1e-4),
    ("polar-coordinates", 1e-4),
    ("lens-distortion", 3e-4),
    ("repeat-tile", 5e-5),
    ("turbulent-displace", 2e-4),
    ("displacement-map", 3e-4),
    // A map's colour read straight is divided by its alpha, rounding
    // and all, so a nearly transparent map pixel moves its pixel by more
    // (`distort.rs`).
    ("displacement-map-straight", 2e-2),
    ("bezier-warp", 1e-4),
    // 3,000 px out, where the CPU's own points are 32-bit floats a step
    // of 2.4e-4 px apart, so any arithmetic the GPU reorders moves a
    // point by about that.
    ("distort-far", 1e-3),
    // Measured 3.6e-5, at a contrast of 100, which multiplies every
    // difference; block noise meets the CPU's cells exactly.
    ("fractal-noise", 1.5e-4),
    // Layer points some 24 000 px from the noise's origin, where a cell's
    // coordinate rounds at a few 1e-5 of a cell: warping noise moves each
    // octave by the level so far, which carries the ease's rounding into
    // the next octave's point. Measured 8.5e-5.
    ("fractal-noise-far", 3e-4),
    // Measured 1.8e-7.
    ("ramp", 1e-6),
    // Measured 2.1e-5: the hue of four colours mixed to near grey, which
    // turns far for a small change; 3.8e-6 otherwise.
    ("four-color", 1e-4),
    // Measured 2.4e-6, by the hue mode.
    ("grid", 1e-5),
    // The edges, hard ones too, fall where the CPU's do: measured 1.2e-7,
    // the wipes and the iris alike.
    ("venetian-blinds", 1e-6),
    ("linear-wipe", 1e-6),
    ("iris-wipe", 1e-6),
    // The pen works in 64 bits, the kernel in 32 from a pixel near the
    // beam's start: measured 2.4e-5 along a beam across a 4K grid.
    ("beam", 1e-4),
    // The radial blurs sum their lines exactly and read them within a few
    // 10^-7 px of the CPU, so they differ by a float's rounding. Like the
    // CPU, a stretch whose weight is under a hundredth of a px takes the
    // layer at the pixel instead, a jump no limit holds a pixel to when
    // rounding puts it on the other side; the cases stay off that edge.
    ("radial-zoom", 3e-6),
    ("radial-spin", 3e-6),
    // The vector blur's streaks follow the slope's direction even where
    // it is all but flat, and take their samples up to 4,000 px away,
    // where a float holds a px to 2.4e-4.
    ("vector-blur", 2e-4),
    // With the map softened, its colour is divided by an alpha that fades
    // out, which magnifies a float's rounding of the Gaussian into the
    // slope: streak ends move, and in a few px the number of samples a
    // streak takes, a whole number by its length, changes by one. Every
    // other px stays within 1e-5.
    ("vector-blur-softened", 5e-3),
    // The lens blur's running sums round as the CPU's do, so its irises
    // differ by a float's rounding of their few small parts; a map's
    // level, worked out in 32-bit floats here and 64 there, moves a radius
    // by its rounding.
    ("lens-blur", 2e-5),
    // The bevel's distances are whole numbers, found exactly; its slopes
    // and light differ by a float's rounding.
    ("bevel", 5e-6),
];

pub(crate) fn tolerance(name: &str) -> f32 {
    TOLERANCES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("{name} has a tolerance"))
}

/// The GPU, opened once for every test; `None` where there is no hardware
/// adapter.
pub(crate) fn gpu() -> Option<&'static Gpu> {
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
pub(crate) struct Random(u64);

impl Random {
    pub(crate) fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }
}

/// `width` by `height` premultiplied pixels with what effects meet in
/// practice: hard edges, a soft disc, a ramp, transparent areas and noise.
pub(crate) fn pattern(width: u16, height: u16, seed: u64) -> Pixels {
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
pub(crate) fn largest_difference(a: &Pixels, b: &Pixels) -> f32 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    a.data()
        .iter()
        .zip(b.data())
        .flat_map(|(p, q)| p.iter().zip(q).map(|(x, y)| (x - y).abs()))
        .fold(0.0, f32::max)
}

pub(crate) fn check(name: &str, case: &str, cpu: &Pixels, gpu: &Pixels) {
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

pub(crate) fn space(transform: Affine) -> Space {
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

/// Runs `effect` on `pixels` on both, in `space`, and checks them.
pub(crate) fn effect_matches(
    name: &str,
    case: &str,
    effect: &Effect,
    pixels: &Pixels,
    space: &Space,
) {
    let Some(gpu) = gpu() else { return };
    assert!(gpu.runs(effect), "the GPU runs {name}");
    let mut cpu = pixels.clone();
    effect.apply(&mut cpu, space);
    let mut held = gpu.upload(pixels).expect("uploaded");
    gpu.effect(&mut held, effect, space).expect("ran");
    check(name, case, &cpu, &gpu.download(&held).expect("downloaded"));
    // The layer it reads drawn on the GPU, as a frame drawn there leaves it.
    if let Some(Input {
        drawn: Drawing::Cpu(input),
        bounds,
    }) = &space.input
    {
        let reading = Space {
            input: Some(Input {
                drawn: Drawing::Held(Arc::new(gpu.upload(input).expect("uploaded"))),
                bounds: *bounds,
            }),
            ..space.clone()
        };
        let mut held = gpu.upload(pixels).expect("uploaded");
        gpu.effect(&mut held, effect, &reading).expect("ran");
        let case = format!("{case}, what it reads held");
        check(name, &case, &cpu, &gpu.download(&held).expect("downloaded"));
    }
}
