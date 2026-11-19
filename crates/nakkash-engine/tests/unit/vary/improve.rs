use serde_json::json;

use super::*;

#[test]
fn constant_speed_keys_get_a_curve_and_eased_ones_keep_theirs() {
    let mut keys = json!([[0, 100], [10, 200, "out-back"], [20, 300]]);
    assert!(ease(&mut keys));
    assert_eq!(
        keys,
        json!([[0, 100, "in-out-cubic"], [10, 200, "out-back"], [20, 300]])
    );
    let mut track = json!("0 -> 200 @0-30f");
    assert!(ease(&mut track));
    assert_eq!(track, json!("0 -> 200 @0-30f in-out-cubic"));
    let mut eased = json!("0 -> 200 @0-30f out-cubic");
    assert!(!ease(&mut eased));
}

#[test]
fn a_repair_of_judder_turns_motion_blur_on() {
    let mut layer = serde_json::Map::new();
    assert!(repair_layer(&mut layer, &Repair::Blur));
    assert!(!repair_layer(&mut layer, &Repair::Blur));
}
