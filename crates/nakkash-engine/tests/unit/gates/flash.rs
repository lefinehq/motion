//! Unit tests for `src/gates/flash.rs`.

use super::*;
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::str::FromStr;

/// Black and white, swapping every `every` frames, for `frames` frames.
fn blinking(every: usize, frames: usize) -> Vec<f64> {
    (0..frames)
        .map(|f| {
            if (f / every).is_multiple_of(2) {
                0.0
            } else {
                1.0
            }
        })
        .collect()
}

#[rstest]
#[case::each_swap_is_a_change(blinking(3, 12), vec![3, 6, 9])]
#[case::a_slow_fade_is_one_change(
    (0..30).map(|f| f64::from(f) / 29.0).collect(),
    vec![3]
)]
#[case::flicker_under_a_tenth_is_none(
    (0..12).map(|f| if f % 2 == 0 { 0.4 } else { 0.45 }).collect(),
    vec![]
)]
#[case::bright_flicker_is_none(
    (0..12).map(|f| if f % 2 == 0 { 0.85 } else { 1.0 }).collect(),
    vec![]
)]
fn changes_count_from_the_last_turn(#[case] series: Vec<f64>, #[case] at: Vec<usize>) {
    assert_eq!(turns(series), at);
}

/// `flashing` cells blinking every 3 frames, the rest black.
fn frames(flashing: usize) -> Vec<[f64; CELLS]> {
    let blink = blinking(3, 60);
    (0..60)
        .map(|f| std::array::from_fn(|c| if c < flashing { blink[f] } else { 0.0 }))
        .collect()
}

#[test]
fn a_quarter_of_the_frame_blinking_flashes_five_times_a_second() {
    // A change every 3 frames at 30 fps: 10 a second, 5 flashes.
    let rates = rates(&frames(QUARTER), 30.0);
    assert_eq!((rates[2], rates[59]), (0.0, 5.0));
    let fewer = rates_of(QUARTER - 1);
    assert!(fewer.iter().all(|&r| r == 0.0), "{fewer:?}");
}

fn rates_of(flashing: usize) -> Vec<f64> {
    rates(&frames(flashing), 30.0)
}

#[test]
fn runs_join_consecutive_frames_over_the_limit() {
    assert_eq!(
        runs(&[0.0, 4.0, 5.0, 3.0, 4.0]),
        vec![(1, 2, 5.0), (4, 4, 4.0)]
    );
}

#[test]
fn a_blinking_full_frame_is_an_error_on_the_scene() {
    let keys: Vec<String> = (0..10)
        .map(|k| format!(r#"["{}f",{},"hold"]"#, k * 3, k % 2))
        .collect();
    let json = format!(
        r##"{{"nakkash":1,"size":"320x180","fps":30,"dur":"1s","bg":"#000000","layers":[
            {{"id":"white","rect":"320x180","fill":"#FFFFFF","at":"center center","anim":{{"opacity":[{}]}}}}]}}"##,
        keys.join(",")
    );
    let report = check(&Session::from_str(&json).expect("scene")).expect("flash");
    // Nine changes in the second: four flashes from frame 24 on.
    assert_eq!(report.to_string(), "E flash scene 24-29f 4/s >3\nOK 0");
    let still = r##"{"nakkash":1,"size":"320x180","dur":"1s","layers":[{"id":"white","rect":"320x180","fill":"#FFFFFF","at":"center center"}]}"##;
    let calm = check(&Session::from_str(still).expect("scene")).expect("flash");
    assert_eq!(calm.to_string(), "OK 1");
}
