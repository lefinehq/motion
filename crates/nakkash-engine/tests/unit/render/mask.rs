//! Unit tests for `src/render/mask.rs`: the scene's masks as the rasteriser's.

use nakkash_format::item::MASKS;

use super::*;

#[test]
fn every_mode_of_the_format_has_its_own_in_the_rasteriser() {
    let choices = MASKS[0].setting("mode").expect("a mode").choices;
    let modes: Vec<MaskMode> = choices.iter().map(|c| mode(c)).collect();
    for (i, a) in modes.iter().enumerate() {
        assert!(
            modes[i + 1..].iter().all(|b| b != a),
            "`{}` shares {a:?}",
            choices[i]
        );
    }
}
