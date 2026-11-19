//! Projected layers: a plane's buffer, as the rasteriser drew it or as
//! floats, laid onto a grid through its landings by the GPU and by the CPU
//! reference ([`project_on_cpu`]), seen at its own size, shrunk, in steep
//! perspective, turned, through several maps at once and crossing the
//! camera.

use nakkash_raster::kurbo::Affine;
use nakkash_raster::{Accelerator, Homography, Landing, Pixels, Texels, project_on_cpu};

use super::{Random, check, gpu, pattern};

/// The grid laid on.
const WIDTH: u16 = 233;
const HEIGHT: u16 = 171;

/// `map`, from the buffer's px to the grid's, as a landing over the whole
/// grid.
fn landing(map: Homography) -> Landing {
    Landing {
        to_buffer: map.inverse().expect("an invertible map"),
        landed: [0, 0, usize::from(WIDTH), usize::from(HEIGHT)],
    }
}

/// A map from the buffer to the grid through a perspective: `tilt` leans
/// the plane back, `scale` shrinks it, `turn` rotates it, `shift` moves it.
fn map(scale: f64, turn: f64, tilt: f64, shift: (f64, f64)) -> Homography {
    let affine = Affine::translate(shift) * Affine::rotate(turn) * Affine::scale(scale);
    let [a, b, c, d, e, f] = affine.as_coeffs();
    Homography::new([[a, c, e], [b, d, f], [0.0, tilt, 1.0]])
}

/// The cases: each a name and its landings.
fn cases() -> Vec<(&'static str, Vec<Landing>)> {
    let mut shifted = landing(map(1.0, 0.0, 0.0, (7.0, -3.0)));
    // Lands on part of the grid only.
    shifted.landed = [20, 10, 200, 150];
    vec![
        ("whole px", vec![shifted]),
        ("shrunk", vec![landing(map(0.23, 0.0, 0.0, (40.0, 30.0)))]),
        (
            "enlarged",
            vec![landing(map(2.7, 0.2, 0.0, (-60.0, -20.0)))],
        ),
        (
            "perspective",
            vec![landing(map(0.9, 0.1, 0.004, (30.0, 10.0)))],
        ),
        ("steep", vec![landing(map(1.3, -0.4, 0.02, (50.0, 5.0)))]),
        // A plane whose far side passes behind the camera.
        (
            "crossing the camera",
            vec![landing(map(1.0, 0.0, -0.012, (10.0, 90.0)))],
        ),
        (
            "three maps",
            vec![
                landing(map(0.8, 0.0, 0.002, (20.0, 20.0))),
                landing(map(0.82, 0.03, 0.002, (24.0, 18.0))),
                landing(map(0.84, 0.06, 0.002, (28.0, 16.0))),
            ],
        ),
    ]
}

/// A buffer the rasteriser drew: premultiplied 8-bit px of `pattern`.
fn bytes(pixels: &Pixels) -> Vec<u8> {
    pixels
        .data()
        .iter()
        .flat_map(|p| p.map(|c| (c * 255.0).round() as u8))
        .collect()
}

#[test]
fn planes_lay_as_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let floats = pattern(301, 197, 50);
    let mut random = Random(51);
    // Noise down to single px, so the halved copies differ from the base.
    let mut speckled = floats.clone();
    for p in speckled.data_mut().iter_mut() {
        let k = random.next();
        *p = p.map(|c| c * k);
    }
    let rgba = bytes(&speckled);
    let held = gpu.upload(&speckled).expect("uploaded");
    for (name, landings) in cases() {
        let share = 1.0 / landings.len() as f32;
        let buffers = [
            (
                "bytes",
                Texels::Bytes {
                    rgba: &rgba,
                    width: 301,
                    height: 197,
                },
            ),
            ("floats", Texels::Floats(&speckled)),
            ("held", Texels::Held(&held)),
        ];
        for (kind, buffer) in buffers {
            let cpu = project_on_cpu(gpu, buffer, &landings, share, WIDTH, HEIGHT)
                .and_then(|r| gpu.download(&r))
                .expect("laid on the CPU");
            let on_gpu = gpu
                .project(buffer, &landings, share, WIDTH, HEIGHT)
                .and_then(|r| gpu.download(&r))
                .expect("laid on the GPU");
            check("plane", &format!("{name} {kind}"), &cpu, &on_gpu);
        }
    }
}

#[test]
fn a_plane_far_from_the_origin_of_a_4k_grid_lays_as_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let buffer = pattern(301, 197, 52);
    let (width, height) = (3840u16, 2160u16);
    // Seen in perspective near the far corner, where 32-bit floats keep
    // the fewest bits of a px's position.
    let map = Homography::new([
        [0.9, 0.05, 3480.0],
        [-0.04, 0.85, 1930.0],
        [0.0, 0.0002, 1.0],
    ]);
    let landings = [Landing {
        to_buffer: map.inverse().expect("an invertible map"),
        landed: [3400, 1850, 3840, 2160],
    }];
    let cpu = project_on_cpu(gpu, Texels::Floats(&buffer), &landings, 1.0, width, height)
        .and_then(|r| gpu.download(&r))
        .expect("laid on the CPU");
    let on_gpu = gpu
        .project(Texels::Floats(&buffer), &landings, 1.0, width, height)
        .and_then(|r| gpu.download(&r))
        .expect("laid on the GPU");
    check("plane", "far corner of 4K", &cpu, &on_gpu);
}
