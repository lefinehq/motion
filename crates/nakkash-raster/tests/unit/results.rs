//! Unit tests for `src/results.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::effect::{Effect, Space};
use crate::oracle::{assert_same_bits, every_effect_set, on_threads, textured};

fn space(transform: Affine) -> Space {
    Space {
        transform,
        bounds: None,
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
    }
}

#[test]
fn a_kept_result_has_the_bits_of_working_it_out() {
    let sets: Vec<(String, Effect)> = every_effect_set()
        .into_iter()
        .filter(|(_, e)| KEPT.contains(&e.spec().name))
        .collect();
    assert_eq!(
        sets.len(),
        2 * KEPT.len(),
        "every kept effect, at two sets of values"
    );
    for transform in [Affine::IDENTITY, Affine::scale_non_uniform(1.5, 0.75)] {
        let space = space(transform);
        for (what, effect) in &sets {
            let source = textured();
            // Without room to keep anything, every result is worked out.
            memory::set_budget(0);
            let mut want = source.clone();
            effect.apply(&mut want, &space);
            memory::set_budget(1 << 30);
            let before = effects_reused();
            let mut first = source.clone();
            effect.apply(&mut first, &space);
            let mut again = source.clone();
            effect.apply(&mut again, &space);
            assert_same_bits(&format!("{what}, worked out"), want.data(), first.data());
            assert_same_bits(&format!("{what}, kept"), want.data(), again.data());
            assert!(effects_reused() > before, "{what}: the second run is kept");
            let_go();
        }
    }
    memory::set_budget(memory::DEFAULT_BUDGET);
}

#[test]
fn a_key_covers_every_bit_it_reads() {
    let values = [ParamValue::Number(10.0), ParamValue::Choice(0)];
    let pixels = textured();
    let base = key("glow", &values, Affine::IDENTITY, &pixels);
    let mut changed = Vec::new();
    for (at, value) in [
        (0, [-0.0f32; 4]),
        (17, [f32::from_bits(0x7FC0_0001); 4]),
        (300, [0.5; 4]),
    ] {
        let mut other = pixels.clone();
        other.data_mut()[at][1] = value[1];
        changed.push(key("glow", &values, Affine::IDENTITY, &other));
    }
    changed.push(key("box-blur", &values, Affine::IDENTITY, &pixels));
    changed.push(key(
        "glow",
        &[ParamValue::Number(10.5), ParamValue::Choice(0)],
        Affine::IDENTITY,
        &pixels,
    ));
    changed.push(key("glow", &values, Affine::translate((1.0, 0.0)), &pixels));
    let wide = Pixels::from_data(pixels.height(), pixels.width(), pixels.data().to_vec());
    changed.push(key("glow", &values, Affine::IDENTITY, &wide));
    for (i, k) in changed.iter().enumerate() {
        assert_ne!(*k, base, "change {i} keeps the key");
    }
}

#[test]
fn a_key_is_the_same_however_the_work_is_split() {
    let pixels = textured();
    let values = [ParamValue::Number(3.0)];
    let want = key("gaussian-blur", &values, Affine::IDENTITY, &pixels);
    for threads in [1, 3, 8] {
        let got = on_threads(threads, || {
            key("gaussian-blur", &values, Affine::IDENTITY, &pixels)
        });
        assert_eq!(got, want, "{threads} threads");
    }
}

#[test]
fn results_stay_within_their_share_of_the_budget() {
    let pixels = textured();
    let bytes = held(&pixels);
    // Room for two results, each as large as the largest kept: a third
    // lets the least recently used go.
    memory::set_budget(16 * 2 * bytes);
    let effect = Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(4.0))]).unwrap();
    for shift in 0..3 {
        let mut p = pixels.clone();
        effect.apply(&mut p, &space(Affine::translate((f64::from(shift), 0.0))));
    }
    assert_eq!(store().as_ref().map_or(0, |s| s.results.len()), 2);
    assert!(store().as_ref().unwrap().bytes <= share());
    // Pressure, a lower budget, lets them go at the next look.
    memory::set_budget(bytes);
    let mut p = pixels.clone();
    effect.apply(&mut p, &space(Affine::IDENTITY));
    assert_eq!(store().as_ref().map_or(0, |s| s.results.len()), 0);
    let_go();
    memory::set_budget(memory::DEFAULT_BUDGET);
}
