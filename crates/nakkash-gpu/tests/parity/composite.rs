//! Compositing inside effects: the glow combined with its layer by every
//! operation, and the solid composite by every mode, in and out of a box.

use nakkash_raster::kurbo::{Affine, Point, Rect};
use nakkash_raster::{Accelerator, Effect, ParamValue};

use super::{effect_matches, gpu, pattern, space};

/// The glow's menu of operations (`GLOW_OPERATIONS`): 22 rows.
const GLOW_OPERATIONS: u8 = 22;

#[test]
fn a_glow_runs_on_the_gpu_by_every_operation_and_composite() {
    let Some(gpu) = gpu() else { return };
    for operation in 0..GLOW_OPERATIONS {
        for composite in 0..3 {
            let glow = Effect::new(
                "glow",
                &[
                    ("operation", ParamValue::Choice(operation)),
                    ("composite", ParamValue::Choice(composite)),
                ],
            )
            .expect("a glow");
            assert!(
                gpu.runs(&glow),
                "operation {operation}, composite {composite}"
            );
        }
    }
}

#[test]
fn glows_combine_by_every_operation_as_on_the_cpu() {
    let pixels = pattern(181, 117, 40);
    for operation in 0..GLOW_OPERATIONS {
        // With the original, and behind it.
        for composite in [0u8, 2] {
            let glow = Effect::new(
                "glow",
                &[
                    ("threshold", ParamValue::Number(0.3)),
                    ("radius", ParamValue::Number(14.0)),
                    ("intensity", ParamValue::Number(1.7)),
                    ("composite", ParamValue::Choice(composite)),
                    ("operation", ParamValue::Choice(operation)),
                ],
            )
            .expect("a glow");
            // The dissolve divides the light by its alpha and keeps a
            // pixel or not by a threshold, so it sharpens differences.
            let name = match operation {
                3 => "glow-dissolve",
                _ => "glow",
            };
            effect_matches(
                name,
                &format!("operation {operation} composite {composite}"),
                &glow,
                &pixels,
                &space(Affine::scale(1.1)),
            );
        }
    }
}

#[test]
fn solid_composites_match_by_every_mode() {
    let pixels = pattern(203, 129, 41);
    // A box the layer's transform keeps upright, one it turns, and none.
    let upright = space(Affine::translate((17.5, -9.0)) * Affine::scale_non_uniform(0.45, 0.3));
    let mut turned = space(Affine::rotate_about(0.35, Point::new(90.0, 60.0)) * Affine::scale(0.4));
    turned.bounds = Some(Rect::new(30.0, 25.0, 390.0, 260.0));
    let mut everywhere = space(Affine::IDENTITY);
    everywhere.bounds = None;
    for mode in 0..17u8 {
        for (name, space) in [
            ("upright", &upright),
            ("turned", &turned),
            ("everywhere", &everywhere),
        ] {
            let solid = Effect::new(
                "solid-composite",
                &[
                    ("source-opacity", ParamValue::Number(0.8)),
                    (
                        "color",
                        ParamValue::Color(nakkash_raster::Color::rgba(0.2, 0.7, 0.9, 1.0)),
                    ),
                    ("opacity", ParamValue::Number(0.65)),
                    ("mode", ParamValue::Choice(mode)),
                ],
            )
            .expect("a solid composite");
            effect_matches(
                "solid-composite",
                &format!("mode {mode} {name}"),
                &solid,
                &pixels,
                space,
            );
        }
    }
}
