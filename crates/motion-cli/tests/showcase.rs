//! The showcase sequences in `examples/showcase/`: long, dense films written
//! from scratch, each of which passes `motion check` with nothing found.

use motion_engine::Session;

/// The product launch film, `examples/showcase/product-launch.json`.
const LAUNCH_SCENE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/showcase/product-launch.json"
);

/// The product launch film passes every rule of `motion check` over all of
/// its 1172 frames with nothing found.
///
/// The pixel gates (`contrast`, `flash`) are left out here: they draw the
/// film, and in a debug build, where our own crates are not optimised, that
/// takes about 16 minutes for this film. `motion check` in a release build
/// runs them too and prints `OK 742`, as the README of `examples/showcase/`
/// says.
#[test]
fn the_product_launch_film_passes_its_rules_with_nothing_found() {
    let session = Session::open(LAUNCH_SCENE).expect("open the product launch film");
    let report = session.check().expect("check the product launch film");
    assert!(
        report.violations.is_empty(),
        "product-launch.json:\n{report}"
    );
    assert!(report.passed > 0, "the rules ran on nothing");
}
