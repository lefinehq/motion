//! Unit tests for `src/cycle.rs`.

use rstest::rstest;
use vello_cpu::kurbo::Affine;

use crate::effect::Input;

use super::*;

use ParamValue::{Choice, Number};

const ONE: Space = Space {
    transform: Affine::IDENTITY,
    bounds: None,
    input: None,
    time: 0.0,
    sound: None,
    outline: None,
    totals: Vec::new(),
    draft: false,
};

/// A cycle from black at 0 to white at 0.5, opaque: a grey that rises
/// to the middle and falls back round the turn.
fn up_and_down() -> [Curve; 4] {
    let rise = Curve::new(&[[0.0, 0.0], [0.5, 1.0]]).expect("a curve");
    let opaque = Curve::new(&[[0.0, 1.0]]).expect("a curve");
    [rise, rise, rise, opaque]
}

/// The row's values for `cycle` with these settings by name, the numbers
/// at rest.
fn values(cycle: [Curve; 4], settings: [&str; 6]) -> Vec<ParamValue> {
    let spec = crate::EFFECTS
        .iter()
        .find(|spec| spec.name == "color-cycle")
        .expect("a color-cycle row");
    let mut values: Vec<ParamValue> = cycle.into_iter().map(ParamValue::Curve).collect();
    values.extend([Number(0.0), Number(1.0), Number(0.0)]);
    values.extend(spec.params[7..].iter().zip(settings).map(|(param, name)| {
        let i = param.choices.iter().position(|c| *c == name);
        Choice(i.expect("a choice of the row") as u8)
    }));
    values
}

const AT_REST: [&str; 6] = ["intensity", "intensity", "wrap", "all", "on", "on"];

/// One opaque pixel of `grey`, cycled.
fn cycled(grey: f32, values: &[ParamValue]) -> [f32; 4] {
    let mut pixels = Pixels::transparent(1, 1);
    pixels.data_mut()[0] = [grey, grey, grey, 1.0];
    color_cycle(&mut pixels, values, &ONE);
    pixels.get(0, 0)
}

fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

#[rstest]
#[case::at_the_first_stop(0.0, 0.0)]
#[case::between(0.25, 0.5)]
#[case::at_the_second_stop(0.5, 1.0)]
// Round the turn from the last stop back to the first.
#[case::closing_the_turn(0.75, 0.5)]
fn the_cycle_runs_straight_between_stops_and_round_the_turn(#[case] grey: f32, #[case] level: f32) {
    let got = cycled(grey, &values(up_and_down(), AT_REST));
    assert!(near(got, [level, level, level, 1.0]), "{got:?}");
}

#[test]
fn without_interpolation_each_stop_holds_until_the_next() {
    let mut settings = AT_REST;
    settings[4] = "off";
    let held = values(up_and_down(), settings);
    assert!(near(cycled(0.25, &held), [0.0, 0.0, 0.0, 1.0]));
    assert!(near(cycled(0.75, &held), [1.0; 4]));
}

#[test]
fn shift_and_repetitions_move_the_phase() {
    let mut moved = values(up_and_down(), AT_REST);
    // A quarter turn on: 0 reads the cycle at 0.25.
    moved[4] = Number(90.0);
    assert!(near(cycled(0.0, &moved), [0.5, 0.5, 0.5, 1.0]));
    moved[4] = Number(0.0);
    // Twice over: 0.25 reads it at 0.5.
    moved[5] = Number(2.0);
    assert!(near(cycled(0.25, &moved), [1.0; 4]));
}

#[test]
fn modify_replaces_only_its_channels() {
    let red = Curve::new(&[[0.0, 1.0]]).expect("a curve");
    let none = Curve::new(&[[0.0, 0.0]]).expect("a curve");
    let cycle = [red, none, none, Curve::new(&[[0.0, 1.0]]).expect("a curve")];
    let mut settings = AT_REST;
    settings[3] = "red";
    let got = cycled(0.5, &values(cycle, settings));
    assert!(near(got, [1.0, 0.5, 0.5, 1.0]), "{got:?}");
    // Hue alone: the grey has no saturation, so it stays grey.
    settings[3] = "hue";
    let got = cycled(0.5, &values(cycle, settings));
    assert!(near(got, [0.5, 0.5, 0.5, 1.0]), "{got:?}");
    settings[3] = "none";
    assert!(near(
        cycled(0.5, &values(cycle, settings)),
        [0.5, 0.5, 0.5, 1.0]
    ));
}

#[test]
fn the_cycles_alpha_multiplies_the_pixels_unless_told_not_to() {
    let half = Curve::new(&[[0.0, 0.5]]).expect("a curve");
    let [r, g, b, _] = up_and_down();
    let mut settings = AT_REST;
    let got = cycled(0.5, &values([r, g, b, half], settings));
    assert!(near(got, [0.5, 0.5, 0.5, 0.5]), "{got:?}");
    settings[5] = "off";
    assert!(near(
        cycled(0.5, &values([r, g, b, half], settings)),
        [1.0; 4]
    ));
}

#[test]
fn blend_keeps_some_of_the_original() {
    let mut blended = values(up_and_down(), AT_REST);
    blended[6] = Number(0.5);
    // White from the cycle, mid grey kept by half.
    assert!(near(cycled(0.5, &blended), [0.75, 0.75, 0.75, 1.0]));
}

#[test]
fn an_added_layer_moves_the_phase_by_its_mode() {
    let mut pixels = Pixels::transparent(1, 1);
    pixels.data_mut()[0] = [0.25, 0.25, 0.25, 1.0];
    let mut added = Pixels::transparent(1, 1);
    added.data_mut()[0] = [0.5, 0.5, 0.5, 1.0];
    let space = Space {
        input: Some(Input::new(added, None)),
        ..ONE
    };
    for (mode, phase) in [
        ("wrap", 0.75),
        ("clamp", 0.75),
        ("average", 0.375),
        ("screen", 0.625),
    ] {
        let mut settings = AT_REST;
        settings[2] = mode;
        let mut got = pixels.clone();
        color_cycle(&mut got, &values(up_and_down(), settings), &space);
        let want = if phase <= 0.5 {
            phase * 2.0
        } else {
            (1.0 - phase) * 2.0
        };
        assert!(
            near(got.get(0, 0), [want, want, want, 1.0]),
            "{mode}: {:?}",
            got.get(0, 0)
        );
    }
}

#[test]
fn empty_curves_are_the_phase_opaque_and_empty_pixels_stay_empty() {
    let empty = [Curve::IDENTITY; 4];
    assert!(near(
        cycled(0.25, &values(empty, AT_REST)),
        [0.25, 0.25, 0.25, 1.0]
    ));
    let mut pixels = Pixels::transparent(1, 1);
    color_cycle(&mut pixels, &values(up_and_down(), AT_REST), &ONE);
    assert_eq!(pixels.get(0, 0), [0.0; 4]);
}
