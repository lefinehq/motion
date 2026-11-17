//! Unit tests for `src/particles.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::style::Color;
use crate::total::TotalPiece;

use ParamValue::{Choice, Number, Point as At};

/// A particle system at the middle of a 64 by 64 frame at `time` seconds.
fn system(rate: f32, velocity: f32, gravity: f32, animation: u8, time: f64) -> Pixels {
    counted(rate, velocity, gravity, animation, time, None)
}

/// [`system`], its births following `births` when given.
fn counted(
    rate: f32,
    velocity: f32,
    gravity: f32,
    animation: u8,
    time: f64,
    births: Option<Total>,
) -> Pixels {
    let values = [
        Number(rate),
        Number(1.0),
        At([32.0, 32.0]),
        Number(0.0),
        Number(0.0),
        Number(velocity),
        Number(gravity),
        Number(0.0),
        Number(90.0),
        Number(0.0),
        Number(4.0),
        Number(4.0),
        Number(0.0),
        Number(1.0),
        ParamValue::Color(Color::rgba(1.0, 1.0, 1.0, 1.0)),
        ParamValue::Color(Color::rgba(1.0, 1.0, 1.0, 1.0)),
        Number(0.0),
        Choice(animation),
        Choice(1),
        Choice(1),
        Choice(0),
    ];
    let mut pixels = Pixels::transparent(64, 64);
    pixels.data_mut().fill([0.5, 0.5, 0.5, 1.0]);
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: None,
        input: None,
        time,
        sound: None,
        outline: None,
        totals: births
            .into_iter()
            .map(|total| (0, std::sync::Arc::new(total)))
            .collect(),
    };
    particles(&mut pixels, &values, &space);
    pixels
}

#[test]
fn particles_replace_the_layer_and_none_are_born_before_time_starts() {
    let empty = system(10.0, 0.0, 0.0, 0, -1.0);
    assert!(empty.data().iter().all(|p| *p == [0.0; 4]));
}

#[test]
fn a_still_particle_sits_on_its_producer() {
    let still = system(10.0, 0.0, 0.0, 0, 0.5);
    assert!(still.get(32, 32)[3] > 0.9);
    assert_eq!(still.get(5, 5), [0.0; 4]);
}

#[test]
fn particles_fly_their_direction_and_fall_with_gravity() {
    // One particle a second, born at 0, flying right at 20 px a second.
    let right = system(1.0, 20.0, 0.0, 1, 0.5);
    assert!(right.get(42, 32)[3] > 0.9, "{:?}", right.get(42, 32));
    assert_eq!(right.get(32, 32), [0.0; 4]);
    // Still, but falling at 40 px a second each second: 5 px after 0.5 s.
    let falling = system(1.0, 0.0, 40.0, 1, 0.5);
    assert!(falling.get(32, 37)[3] > 0.9, "{:?}", falling.get(32, 37));
}

#[test]
fn the_same_frame_draws_the_same_particles() {
    assert_eq!(
        system(30.0, 15.0, 5.0, 0, 0.7),
        system(30.0, 15.0, 5.0, 0, 0.7)
    );
}

#[test]
fn resistance_slows_a_particle_towards_where_it_would_stop() {
    let at = flight([0.0, 0.0], [10.0, 0.0], 0.0, 2.0, 100.0);
    assert!((at[0] - 5.0).abs() < 1e-3, "{at:?}");
}

/// 1000 a second for 0.1 s, then none: a burst of 100.
fn burst() -> Total {
    Total::new(vec![
        TotalPiece {
            start: 0.0,
            total: 0.0,
            rate: 1000.0,
            slope: 0.0,
        },
        TotalPiece {
            start: 0.1,
            total: 100.0,
            rate: 0.0,
            slope: 0.0,
        },
    ])
}

/// A rate dropped to 0 stops births only: the burst lives out its second
/// of `longevity`, then is gone, whatever `rate` reads now.
#[rstest::rstest]
#[case::after_the_drop(0.5, true)]
#[case::near_its_end(1.05, true)]
#[case::after_its_longevity(1.2, false)]
fn a_burst_lives_out_its_longevity(#[case] time: f64, #[case] lit: bool) {
    let frame = counted(0.0, 0.0, 0.0, 0, time, Some(burst()));
    assert_eq!(frame.get(32, 32)[3] > 0.5, lit, "{:?}", frame.get(32, 32));
}

/// A total of a rate that holds draws what the rate alone draws.
#[test]
fn a_held_total_draws_as_the_rate_alone() {
    assert_eq!(
        counted(30.0, 15.0, 5.0, 0, 0.7, Some(Total::constant(30.0))),
        system(30.0, 15.0, 5.0, 0, 0.7)
    );
}
