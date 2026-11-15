//! Unit tests for `src/particles.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::style::Color;

use ParamValue::{Choice, Number, Point as At};

/// A particle system at the middle of a 64 by 64 frame at `time` seconds.
fn system(rate: f32, velocity: f32, gravity: f32, animation: u8, time: f64) -> Pixels {
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
