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

use serde_json::json;

use nakkash_baked::{Manifest, Matte, Store};
use nakkash_format::Sha256;
use nakkash_ml::bake::{Baker, Cutout};
use nakkash_ml::frames::PngFolder;
use nakkash_ml::image::{Filter, Plane};
use nakkash_ml::matte::Edges;
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

/// The box `shot` was prompted with on frame 0 when it was measured.
fn first_box(shot: &Path) -> [f32; 4] {
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(shot.join("meta.json")).unwrap()).unwrap();
    let b = &meta["gt_box0"];
    std::array::from_fn(|i| b[i].as_f64().unwrap() as f32)
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

/// The segmentation's hard masks of `shot` at its size, the seconds a
/// frame took and the seconds opening the graphs took.
fn segment(shot: &Path, prompts: &[Prompt]) -> (Vec<Plane>, Vec<Plane>, f64, f64) {
    let lock = ModelLock::builtin();
    let store = store();
    let backend = Onnx::load(&store, &lock).unwrap();
    let opening = Instant::now();
    let mut segmenter = Segmenter::open(&backend, &store, &lock, threads()).unwrap();
    let open = opening.elapsed().as_secs_f64();
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
    (logits, hard, per_frame, open)
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
    let (_, hard, per_frame, open) = segment(&shot, &boxed(first_box(&shot)));
    let (iou, least, sad, dtssd) = measure(&hard, &truth);
    println!(
        "{}",
        serde_json::json!({"gate": "segment", "shot": "product_synth", "iou": iou, "iou_min": least, "sad": sad, "dtssd": dtssd, "s_per_frame": per_frame, "open_s": open, "threads": threads()})
    );
    assert!(iou >= 0.98 && least >= 0.94, "IoU {iou} (least {least})");
    assert!(sad <= 7.0, "SAD {sad}");
    assert!(dtssd <= 55.0, "dtSSD {dtssd}");
}

/// Limits on a matte: least mean IoU, least IoU of a frame, most SAD and
/// most dtSSD.
struct Limits {
    iou: f64,
    least: f64,
    sad: f64,
    dtssd: f64,
}

/// Bakes the refined matte of `name` through the whole bake, into a
/// scratch store, reads it back and holds it to `limits`.
fn fused(name: &str, limits: &Limits) {
    let shot = shots().join(name);
    let truth = truth(&shot);
    let lock = ModelLock::builtin();
    let store = store();
    let backend = Onnx::load(&store, &lock).unwrap();
    let scratch = std::env::temp_dir().join(format!("nakkash-gate-{name}-{}", std::process::id()));
    let baked = Store::new(scratch.clone());
    let baker = Baker::new(&backend, &store, &lock, &baked, threads(), |m| {
        eprintln!("{m}")
    });
    let mut frames = PngFolder::open(&shot.join("full")).unwrap();
    let request = json!({"gate": name});
    let clip = Sha256::of(name.as_bytes());
    let cutout = Cutout {
        prompts: boxed(first_box(&shot)),
        edges: Edges::Refine,
        band: 12.0,
        agree: 0.9,
        smooth: None,
        request: request.clone(),
    };
    let done = baker.cutout(&mut frames, &cutout, &clip).unwrap();
    let manifest = Manifest::<Matte>::open(&baked, &done.digest, &request, &clip).unwrap();
    let alphas: Vec<Plane> = (0..manifest.frames())
        .map(|f| {
            let a = manifest.alpha(&baked, f).unwrap().unwrap();
            Plane {
                width: a.width as usize,
                height: a.height as usize,
                data: a.values.iter().map(|&v| f32::from(v) / 65535.0).collect(),
            }
        })
        .collect();
    let _ = std::fs::remove_dir_all(&scratch);
    let (iou, least, sad, dtssd) = measure(&alphas, &truth);
    println!(
        "{}",
        json!({"gate": "matte", "shot": name, "iou": iou, "iou_min": least, "sad": sad, "dtssd": dtssd,
               "refined": done.refined, "frames": done.frames, "segmentation_kept": done.segmentation_kept,
               "s_per_frame": done.seconds_per_frame, "threads": threads()})
    );
    assert!(
        iou >= limits.iou && least >= limits.least,
        "IoU {iou} (least {least})"
    );
    assert!(sad <= limits.sad, "SAD {sad}");
    assert!(dtssd <= limits.dtssd, "dtSSD {dtssd}");
}

/// The refined matte of the synthetic product: measured in the first
/// phase at IoU 0.979 (least 0.920), SAD 7.7, dtSSD 46.9.
#[test]
#[ignore = "needs the models and the shots"]
fn the_products_matte_keeps_its_edge() {
    fused(
        "product_synth",
        &Limits {
            iou: 0.97,
            least: 0.90,
            sad: 9.0,
            dtssd: 55.0,
        },
    );
}

/// The refined matte of the man before a green screen: measured at IoU
/// 0.990 (least 0.966), SAD 2.8, dtSSD 20.1.
#[test]
#[ignore = "needs the models and the shots"]
fn the_mans_matte_keeps_his_hair() {
    fused(
        "man_green",
        &Limits {
            iou: 0.98,
            least: 0.95,
            sad: 3.5,
            dtssd: 25.0,
        },
    );
}

/// The refined matte of the cat before a green screen: measured at IoU
/// 0.996 (least 0.996), SAD 1.1, dtSSD 3.8.
#[test]
#[ignore = "needs the models and the shots"]
fn the_cats_matte_keeps_its_fur() {
    fused(
        "cat_green",
        &Limits {
            iou: 0.99,
            least: 0.99,
            sad: 1.5,
            dtssd: 6.0,
        },
    );
}

/// One frame of the product's exact ground truth: its face's corners,
/// its surface points and which of them are seen.
#[derive(serde::Deserialize)]
struct TruthFrame {
    corners: [[f64; 2]; 4],
    points: Vec<[f64; 2]>,
    visible: Vec<bool>,
}

/// The product's ground truth, frame by frame.
fn track_truth(shot: &Path) -> Vec<TruthFrame> {
    #[derive(serde::Deserialize)]
    struct Truth {
        gt: Vec<TruthFrame>,
    }
    let truth: Truth =
        serde_json::from_slice(&std::fs::read(shot.join("gt.json")).unwrap()).unwrap();
    truth.gt
}

/// The alpha-weighted centroid of `alpha`, pixel centres at whole numbers.
fn centroid(alpha: &Plane) -> [f64; 2] {
    let (mut sx, mut sy, mut s) = (0.0, 0.0, 0.0);
    for (i, &a) in alpha.data.iter().enumerate() {
        let a = f64::from(a);
        sx += a * (i % alpha.width) as f64;
        sy += a * (i / alpha.width) as f64;
        s += a;
    }
    [sx / s, sy / s]
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    match v.len() {
        0 => f64::NAN,
        n if n % 2 == 1 => v[n / 2],
        n => (v[n / 2 - 1] + v[n / 2]) / 2.0,
    }
}

/// The mean corner error of `track` against `truth` over `frames`, and its
/// median.
fn corner_error(
    track: &nakkash_baked::Track,
    truth: &[TruthFrame],
    frames: std::ops::Range<usize>,
) -> (f64, f64) {
    let errors: Vec<f64> = frames
        .flat_map(|f| {
            let fit = track.frames[f].fit.expect("a fit on every frame");
            (0..4).map(move |c| distance(fit.corners[c], truth[f].corners[c]))
        })
        .collect();
    (
        errors.iter().sum::<f64>() / errors.len() as f64,
        median(errors),
    )
}

/// Tracks of the synthetic product: its mask's centroid through the
/// occlusion (measured in the first phase at mean 1.4 px, max 5.4), the
/// surface points followed from frame 0 (median error per frame 0.8 to 2.5
/// px up to frame 21), its guarded corners (mean 4.8 px up to frame 21),
/// and the drift after frame 36 corrected by an anchor there (2.4 px
/// median).
#[test]
#[ignore = "needs the models and the shots"]
fn the_products_track_holds_through_the_occlusion() {
    use nakkash_ml::track::{self, FollowPoints, PointTracker, TrackClip, TrackSpec};

    let name = "product_synth";
    let shot = shots().join(name);
    let alphas = truth(&shot);
    let truth = track_truth(&shot);
    let lock = ModelLock::builtin();
    let store = store();
    let backend = Onnx::load(&store, &lock).unwrap();
    let scratch = std::env::temp_dir().join(format!("nakkash-gate-track-{}", std::process::id()));
    let baked = Store::new(scratch.clone());
    let baker = Baker::new(&backend, &store, &lock, &baked, threads(), |m| {
        eprintln!("{m}")
    });
    let clip = Sha256::of(name.as_bytes());
    let mut frames = PngFolder::open(&shot.join("full")).unwrap();
    let size = frames.size();
    let n = frames.len();
    let mut made = baker.made();
    let (logits, _) = baker
        .segmentation(&mut frames, &boxed(first_box(&shot)), &clip, &mut made)
        .unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
    let times: Vec<f64> = (0..n).map(|f| f as f64 / 24.0).collect();
    let open = || {
        PointTracker::open(
            &backend,
            &store,
            &lock,
            threads(),
            Box::new(PngFolder::open(&shot.join("full")).unwrap()),
        )
        .unwrap()
    };

    // The points the ground truth sees on frame 0, followed.
    let start = Instant::now();
    let mut tracker = open();
    let seen0: Vec<usize> = (0..truth[0].points.len())
        .filter(|&p| truth[0].visible[p])
        .collect();
    let queries: Vec<[f64; 2]> = seen0.iter().map(|&p| truth[0].points[p]).collect();
    let followed = track::follow_all(&mut tracker, 0, &queries).unwrap();
    let follow_seconds = start.elapsed().as_secs_f64();
    let per_frame: Vec<f64> = (0..n)
        .map(|f| {
            median(
                seen0
                    .iter()
                    .zip(&followed)
                    .filter(|(p, _)| truth[f].visible[**p])
                    .map(|(&p, t)| distance(t.at[f], truth[f].points[p]))
                    .collect(),
            )
        })
        .collect();
    let points_early = per_frame[..=21].iter().copied().fold(0.0, f64::max);
    drop(tracker);

    let bake = |anchors: Vec<track::Anchor>| {
        let mut tracker = open();
        assert_eq!(tracker.frames(), n);
        let mut pictures = PngFolder::open(&shot.join("full")).unwrap();
        let mut frame = |f: usize| pictures.frame(f);
        let spec = TrackSpec {
            prompt_frame: 0,
            corners: Some(truth[0].corners),
            anchors,
        };
        track::bake(
            &spec,
            TrackClip {
                size,
                times: &times,
                logits: &logits,
                frame: &mut frame,
            },
            &mut tracker,
        )
        .unwrap()
    };
    let start = Instant::now();
    let plain = bake(vec![]);
    let track_seconds = start.elapsed().as_secs_f64();
    let anchored = bake(vec![track::Anchor {
        frame: 36,
        corners: truth[36].corners,
    }]);

    let centroids: Vec<f64> = (0..n)
        .map(|f| {
            distance(
                plain.frames[f]
                    .mask
                    .expect("the object on every frame")
                    .centroid,
                centroid(&alphas[f]),
            )
        })
        .collect();
    let centroid_mean = centroids.iter().sum::<f64>() / n as f64;
    let centroid_max = centroids.iter().copied().fold(0.0, f64::max);
    let (corners_early, _) = corner_error(&plain, &truth, 0..22);
    let (corners_all, corners_all_median) = corner_error(&plain, &truth, 0..n);
    let (_, drift_plain) = corner_error(&plain, &truth, 37..n);
    let (_, drift_anchored) = corner_error(&anchored, &truth, 37..n);
    println!(
        "{}",
        json!({"gate": "track", "shot": name, "centroid_mean": centroid_mean, "centroid_max": centroid_max,
               "points_median_max_to_21": points_early, "points_median": per_frame,
               "corners_mean_to_21": corners_early, "corners_mean": corners_all, "corners_median": corners_all_median,
               "drift_after_36_median": drift_plain, "drift_after_36_anchored_median": drift_anchored,
               "follow_s": follow_seconds, "track_s": track_seconds, "threads": threads()})
    );
    assert!(
        centroid_mean <= 2.0 && centroid_max <= 7.0,
        "centroid {centroid_mean} (max {centroid_max})"
    );
    assert!(points_early <= 3.0, "points {points_early}");
    assert!(corners_early <= 6.0, "corners {corners_early}");
    assert!(
        drift_anchored <= 4.0 && drift_anchored < drift_plain,
        "drift {drift_anchored} against {drift_plain}"
    );
}
