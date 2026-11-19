use std::cell::Cell;

use super::*;

fn image(v: u8) -> Image {
    Image::new(1, 1, vec![v; 4]).expect("an image")
}

#[test]
fn a_frame_of_an_identity_drawn_before_is_its_image() {
    let held = Held::new(4, 2);
    let drawn = Cell::new(0);
    let draw = |v: u8| {
        drawn.set(drawn.get() + 1);
        Ok(image(v))
    };
    assert_eq!(held.image(1, || draw(1)).expect("drawn"), image(1));
    assert_eq!(held.image(1, || draw(9)).expect("held"), image(1));
    assert_eq!(held.image(2, || draw(2)).expect("drawn"), image(2));
    assert_eq!(drawn.get(), 2);
}

#[test]
fn only_the_most_recent_identities_are_kept() {
    // Two threads: four images at most.
    let held = Held::new(4, 2);
    for id in 0..5 {
        held.image(id, || Ok(image(0))).expect("drawn");
    }
    let drawn = Cell::new(false);
    held.image(0, || {
        drawn.set(true);
        Ok(image(0))
    })
    .expect("drawn again");
    assert!(drawn.get(), "the oldest was let go");
}

#[test]
fn a_failure_is_the_failure_of_every_frame_of_its_identity() {
    let held = Held::new(4, 1);
    let failed = || {
        Err(nakkash_format::Diagnostic::new(
            nakkash_format::Kind::Invalid,
            "no",
        ))
    };
    assert!(held.image(7, failed).is_err());
    assert!(held.image(7, || Ok(image(1))).is_err());
}
