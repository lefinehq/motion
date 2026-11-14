//! How long a story-sized scene takes per frame, step by step: run with
//! `cargo test --release -p motion-engine --test timing -- --ignored --nocapture`.
//! Ignored by default, since debug builds and shared CI machines time nothing useful.
//!
//! The scene (`tests/fixtures/story.json`) is 1080x1920 with ten layers like a
//! story template's: a full-frame plate, two large rotated squares, three
//! lines of stroked text under a line animator, two nested compositions with
//! the fill effect, two recoloured dots, a caption and a badge.

use std::time::Instant;

use motion_engine::{Quality, RenderOptions, Session};

const STORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/story.json");

/// The median time of `step` over `frames`, in milliseconds.
fn median(frames: &[f64], mut step: impl FnMut(f64)) -> f64 {
    let mut times: Vec<f64> = frames
        .iter()
        .map(|&frame| {
            let start = Instant::now();
            step(frame);
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

#[test]
#[ignore = "a timing, not a check"]
fn story_frames() {
    let session = Session::open(STORY).unwrap();
    session.render(0.0).unwrap();
    // Frames through the animations: lines rising, compositions fading in.
    let frames: Vec<f64> = (0..40).map(|i| f64::from(i) * 2.5).collect();
    let evaluate = median(&frames, |f| drop(session.frame(f).unwrap()));
    let display = median(&frames, |f| drop(session.display_list(f).unwrap()));
    println!("evaluate: {evaluate:.3} ms; display list: {display:.3} ms (evaluate included)");
    for (name, options) in [
        ("precise", RenderOptions::default()),
        (
            "fast",
            RenderOptions {
                quality: Quality::Fast,
                ..RenderOptions::default()
            },
        ),
        ("golden", RenderOptions::GOLDEN),
    ] {
        let ms = median(&frames, |f| drop(session.render_with(f, options).unwrap()));
        println!("{name}: {ms:.1} ms per 1080x1920 frame, end to end");
    }
    let range = 0..100;
    let start = Instant::now();
    session
        .render_frames(range.clone(), RenderOptions::default(), |_, _| Ok(()))
        .unwrap();
    let wall = start.elapsed().as_secs_f64() * 1000.0 / f64::from(range.end);
    println!(
        "precise, every thread: {wall:.1} ms per frame over {} frames",
        range.end
    );
}
