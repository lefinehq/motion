//! Unit tests for `src/diff.rs`: a scene against references drawn from
//! itself, moved, sampled between frames, and refused.

use std::fs;

use pretty_assertions::assert_eq;

use super::*;

/// A 40x20 scene at 10 fps lasting 1 s: a white square crossing a black
/// canvas, and a grey bar that a reference may leave out.
const SCENE: &str = r##"{"motion":1,"size":"40x20","fps":10,"dur":"1s","bg":"#000000","layers":[
{"id":"bar","rect":"40x4","fill":"#808080","at":"20 18"},
{"id":"box","rect":"10x10","fill":"#FFFFFF","anim":{"position":"5,10 -> 35,10 @0-10f"}}]}"##;

/// A session on `text`.
fn session(text: &str) -> Session {
    text.parse().expect("a valid scene")
}

/// A scratch folder for the test `name`, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-diff-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create a scratch folder");
    dir
}

/// Writes `session` drawn at each of `frames` into a new scratch folder.
fn references(name: &str, session: &Session, frames: &[f64]) -> Vec<PathBuf> {
    let dir = scratch(name);
    frames
        .iter()
        .enumerate()
        .map(|(i, frame)| {
            let path = dir.join(format!("f{i:03}.png"));
            let image = session.render(*frame).expect("draw a reference");
            fs::write(&path, image.to_png().expect("encode")).expect("write");
            path
        })
        .collect()
}

/// Options comparing with `frames` at `fps` from the start.
fn options(frames: Vec<PathBuf>, fps: f64) -> DiffOptions {
    DiffOptions {
        frames,
        fps,
        start: 0.0,
        tolerance: DiffOptions::TOLERANCE,
        images: None,
    }
}

#[test]
fn a_scene_matches_its_own_frames_exactly() {
    let scene = session(SCENE);
    let frames = references("own", &scene, &[0.0, 3.0, 6.0, 9.0]);
    let diff = scene.diff(&options(frames, 10.0 / 3.0)).unwrap();
    assert_eq!(diff.frames.len(), 4);
    let errors: Vec<(f64, f64)> = diff
        .frames
        .iter()
        .map(|f| (f.drawn.mean, f.union.failing))
        .collect();
    assert_eq!(errors, [(0.0, 0.0); 4]);
    assert!(diff.frames.iter().all(|f| f.drawn.psnr.is_none()));
    assert!((diff.frames[1].time - 0.3).abs() < 1e-12);
    assert!(diff.frames[0].drawn.pixels > 0);
    assert!(diff.frames[0].union.pixels > diff.frames[0].drawn.pixels);
}

#[test]
fn reference_frames_are_sampled_at_their_own_times() {
    let scene = session(SCENE);
    let halves: Vec<f64> = (0..20).map(|i| f64::from(i) * 0.5).collect();
    let frames = references("halves", &scene, &halves);
    let diff = scene.diff(&options(frames.clone(), 20.0)).unwrap();
    assert_eq!(diff.union_mean.worst, 0.0);
    let wrong = scene.diff(&options(frames, 10.0));
    assert_eq!(wrong.unwrap_err().kind, Kind::OutOfRange);
}

#[test]
fn a_misplaced_element_counts_over_all_the_scene_draws() {
    let scene = session(SCENE);
    let late = session(&SCENE.replace("5,10 -> 35,10", "15,10 -> 45,10"));
    let frames = references("late", &late, &[0.0, 1.0, 2.0]);
    let diff = scene.diff(&options(frames, 10.0)).unwrap();
    let first = &diff.frames[0];
    assert!(first.drawn.failing > 0.0, "{first:?}");
    assert!(first.union.pixels > first.drawn.pixels);
    assert!(diff.union_failing.worst > 0.0);
    assert_eq!(
        diff.union_failing.worst_frame,
        diff.frames
            .iter()
            .max_by(|a, b| a.union.failing.total_cmp(&b.union.failing))
            .map(|f| f.file.clone())
            .unwrap()
    );
}

#[test]
fn a_hidden_layer_is_neither_drawn_nor_measured() {
    let mut scene = session(SCENE);
    let without = session(&SCENE.replace(
        r##""fill":"#808080","##,
        r##""fill":"#808080","hidden":true,"##,
    ));
    let frames = references("hidden", &without, &[0.0, 5.0]);
    assert!(
        scene
            .diff(&options(frames.clone(), 2.0))
            .unwrap()
            .union_mean
            .worst
            > 0.0
    );
    scene.edit("bar hidden=true").unwrap();
    assert_eq!(
        scene.diff(&options(frames, 2.0)).unwrap().union_mean.worst,
        0.0
    );
}

#[test]
fn error_images_mark_the_failing_pixels() {
    let scene = session(SCENE);
    let late = session(&SCENE.replace("5,10 -> 35,10", "15,10 -> 45,10"));
    let dir = scratch("images");
    let mut with_images = options(references("images-ref", &late, &[0.0]), 10.0);
    with_images.images = Some(dir.clone());
    scene.diff(&with_images).unwrap();
    let image = Image::from_png(&fs::read(dir.join("f000.png")).unwrap()).unwrap();
    assert_eq!(image.pixel(5, 10), Some([255, 0, 0, 255]));
}

#[test]
fn refuses_frames_of_another_size_and_folders_without_frames() {
    let scene = session(SCENE);
    let dir = scratch("size");
    let small = Image::new(2, 2, vec![0; 16]).unwrap();
    let path = dir.join("f000.png");
    fs::write(&path, small.to_png().unwrap()).unwrap();
    let error = scene.diff(&options(vec![path], 10.0)).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(
        error.message.contains("is 2x2, the scene 40x20"),
        "{}",
        error.message
    );
    let empty = scratch("empty");
    assert_eq!(
        DiffOptions::frames_in(&empty).unwrap_err().kind,
        Kind::Missing
    );
}

#[test]
fn pooling_takes_the_mean_the_99th_percentile_and_the_worst_frame() {
    let frame = |file: &str, mean: f64| FrameDiff {
        file: file.into(),
        time: 0.0,
        drawn: Error {
            mean,
            ..Error::default()
        },
        union: Error::default(),
    };
    let frames: Vec<FrameDiff> = (0..100)
        .map(|i| frame(&format!("f{i}"), f64::from(i) / 100.0))
        .collect();
    let pooled = pooled(&frames, |f| f.drawn.mean);
    assert!((pooled.mean - 0.495).abs() < 1e-9);
    assert_eq!((pooled.p99, pooled.worst), (0.98, 0.99));
    assert_eq!(pooled.worst_frame, "f99");
    assert_eq!(super::pooled(&[], |f| f.drawn.mean), Pooled::default());
}
