//! The quality gates of D86, on the measured shots with exact ground
//! truth. They need the models and the runtime in the store and the shots
//! on disk, so they are ignored: run them where the models are,
//!
//! ```text
//! NAKKASH_ML_SHOTS=<folder of shots> cargo test -p nakkash-onnx --test gates -- --ignored --nocapture
//! ```
//!
//! A shot is a folder with `full/` (its frames, PNG) and `gt_alpha/` (the
//! exact alpha of each frame, 16-bit greyscale PNG). Each gate prints one
//! JSON line of what it measured, then holds it to the limits below: the
//! values measured when the gates were set, with a margin.
//! `NAKKASH_ML_THREADS` sets the threads a graph runs on (2 by default).

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Instant;

use nakkash_ml::frames::PngFolder;
use nakkash_ml::image::{Filter, Plane};
use nakkash_ml::segment::{Frames, Prompt, Segmenter};
use nakkash_ml::{ModelLock, ModelStore, quality};
use nakkash_onnx::Onnx;

/// The shots' folder.
fn shots() -> PathBuf {
    PathBuf::from(std::env::var("NAKKASH_ML_SHOTS").expect("set NAKKASH_ML_SHOTS"))
}

fn threads() -> usize {
    std::env::var("NAKKASH_ML_THREADS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(2)
}

fn store() -> ModelStore {
    ModelStore::new(nakkash_fonts::Cache::user().unwrap().root())
}

/// The 16-bit greyscale alpha at `path`, from 0 to 1.
fn alpha(path: &Path) -> Plane {
    let mut reader = png::Decoder::new(BufReader::new(File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Grayscale, png::BitDepth::Sixteen)
    );
    let (width, height) = (info.width as usize, info.height as usize);
    Plane {
        width,
        height,
        data: data[..width * height * 2]
            .chunks_exact(2)
            .map(|p| f32::from(u16::from_be_bytes([p[0], p[1]])) / 65535.0)
            .collect(),
    }
}

/// The ground truth of `shot`.
fn truth(shot: &Path) -> Vec<Plane> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(shot.join("gt_alpha"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    paths.iter().map(|p| alpha(p)).collect()
}

/// A box prompt on frame 0.
fn boxed(area: [f32; 4]) -> Vec<Prompt> {
    vec![Prompt {
        frame: 0,
        area: Some(area),
        include: vec![],
        exclude: vec![],
    }]
}

/// The segmentation's hard masks of `shot` at its size, and the seconds a
/// frame took.
fn segment(shot: &Path, prompts: &[Prompt]) -> (Vec<Plane>, Vec<Plane>, f64) {
    let lock = ModelLock::builtin();
    let store = store();
    let backend = Onnx::load(&store, &lock).unwrap();
    let mut segmenter = Segmenter::open(&backend, &store, &lock, threads()).unwrap();
    let mut frames = PngFolder::open(&shot.join("full")).unwrap();
    let (width, height) = frames.size();
    let start = Instant::now();
    let logits = segmenter
        .segment(&mut frames, prompts, |f| {
            eprintln!(
                "segmented frame {f} at {:.1} s",
                start.elapsed().as_secs_f64()
            )
        })
        .unwrap();
    let per_frame = start.elapsed().as_secs_f64() / frames.len() as f64;
    let up: Vec<Plane> = logits
        .iter()
        .map(|l| l.resized(width, height, Filter::Triangle))
        .collect();
    let hard = up
        .iter()
        .map(|l| Plane {
            width,
            height,
            data: l
                .data
                .iter()
                .map(|&v| f32::from(u8::from(v > 0.0)))
                .collect(),
        })
        .collect();
    (logits, hard, per_frame)
}

/// IoU (mean and least), SAD and dtSSD of `alphas` against `truth`.
fn measure(alphas: &[Plane], truth: &[Plane]) -> (f64, f64, f64, f64) {
    let ious: Vec<f64> = alphas
        .iter()
        .zip(truth)
        .map(|(a, t)| quality::iou(a, t))
        .collect();
    let sads: Vec<f64> = alphas
        .iter()
        .zip(truth)
        .map(|(a, t)| quality::sad(a, t))
        .collect();
    (
        ious.iter().sum::<f64>() / ious.len() as f64,
        ious.iter().copied().fold(1.0, f64::min),
        sads.iter().sum::<f64>() / sads.len() as f64,
        quality::dtssd(alphas, truth),
    )
}

/// The segmentation alone on the synthetic product shot: measured in the
/// first phase at IoU 0.989 (least 0.949), SAD 6.2, dtSSD 48.1.
#[test]
#[ignore = "needs the models and the shots"]
fn segmentation_follows_the_product_through_the_occlusion() {
    let shot = shots().join("product_synth");
    let truth = truth(&shot);
    let (_, hard, per_frame) = segment(&shot, &boxed([212.0, 351.0, 662.0, 935.0]));
    let (iou, least, sad, dtssd) = measure(&hard, &truth);
    println!(
        "{}",
        serde_json::json!({"gate": "segment", "shot": "product_synth", "iou": iou, "iou_min": least, "sad": sad, "dtssd": dtssd, "s_per_frame": per_frame, "threads": threads()})
    );
    assert!(iou >= 0.98 && least >= 0.94, "IoU {iou} (least {least})");
    assert!(sad <= 7.0, "SAD {sad}");
    assert!(dtssd <= 55.0, "dtSSD {dtssd}");
}
