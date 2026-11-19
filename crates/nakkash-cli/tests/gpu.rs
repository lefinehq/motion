//! Whole frames drawn with the GPU against the CPU reference ([D74]): each
//! example scene drawn both ways, where every 8-bit channel of every pixel
//! must stay within D74's frame tolerance of the CPU's.
//!
//! On a machine without a hardware GPU (CI's Linux runners) every case says
//! so and passes without checking anything.
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

use std::path::{Path, PathBuf};

use nakkash_engine::{Session, gpu};
use rstest::rstest;

/// D74's tolerance for a whole frame: one level of an 8-bit channel.
const FRAME_LEVELS: u8 = 1;

fn example(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(path)
}

#[rstest]
#[case::product_launch("showcase/product-launch.json")]
#[case::brand_system("showcase/brand-system.json")]
#[case::data_story("showcase/data-story.json")]
#[case::ui_reveal("advanced/ui-reveal.json")]
#[case::camera_depth("advanced/camera-depth.json")]
#[case::kinetic_type("advanced/kinetic-type.json")]
#[case::screen_pin("advanced/screen-pin.json")]
#[case::social("social-916.json")]
#[case::title_card("title-card.json")]
fn a_frame_drawn_with_the_gpu_matches_the_cpu(#[case] scene: &str) {
    let opened = match gpu::open() {
        Ok(opened) => opened,
        Err(refusal) => {
            eprintln!("skipped: {refusal}");
            return;
        }
    };
    let mut session = Session::open(example(scene)).expect("an example opens");
    let frames = session.frames().expect("frames");
    let frame = f64::from(frames.start + (frames.end - frames.start) / 2);
    let cpu = session.render(frame).expect("drawn on the CPU");
    session.set_accelerator(Some(opened.accelerator));
    let gpu = session.render(frame).expect("drawn with the GPU");
    assert_eq!((cpu.width(), cpu.height()), (gpu.width(), gpu.height()));
    let (mut largest, mut differing) = (0u8, 0usize);
    for (a, b) in cpu.pixels().iter().zip(gpu.pixels()) {
        let difference = a.abs_diff(*b);
        largest = largest.max(difference);
        differing += usize::from(difference > 0);
    }
    let tally = session.tally().expect("a tally");
    eprintln!(
        "{scene} frame {frame}: largest {largest} levels, {differing} channels differ; on the GPU {:?}, on the CPU {:?}",
        tally.accelerated, tally.fell_back
    );
    assert!(
        largest <= FRAME_LEVELS,
        "{scene}: the GPU's frame differs by {largest} levels"
    );
}
