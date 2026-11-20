//! The memory budget changes how fast a frame is drawn, never what is
//! drawn ([D90]): frames drawn with no room to keep or pool anything, under
//! memory pressure, and with every pooled grid clobbered, are the frames
//! drawn with room for everything, bit for bit.
//!
//! The budget is the process's, so these tests take turns.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use nakkash_raster::kurbo::{Affine, Circle, Rect, Shape};
use nakkash_raster::memory::{self, Pressure};
use nakkash_raster::{
    Color, DrawStyle, Effect, FillRule, Frame, Image, Layer, Matte, MatteMode, Node, Paint,
    ParamValue, PathDraw, RenderOptions, render_with,
};

/// One test at a time: the budget is the process's.
fn turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;

fn fill(path: impl Shape, color: [f32; 4]) -> Node {
    Node::Path(PathDraw {
        path: path.to_path(0.1),
        paint: Paint::Solid(Color::rgba(color[0], color[1], color[2], color[3])),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

fn effect(name: &str, params: &[(&str, ParamValue)]) -> Effect {
    Effect::new(name, params).expect("a known effect")
}

/// A frame whose layers read one blurred layer three times each, glow,
/// wrap and are matted: drawings kept for later reads, grids pooled, and
/// copies the budget can refuse.
fn heavy() -> Frame {
    let mut map = Layer::new(vec![
        fill(Circle::new((200.0, 150.0), 90.0), [0.9, 0.2, 0.1, 1.0]),
        fill(Rect::new(40.0, 40.0, 160.0, 120.0), [0.1, 0.8, 0.3, 0.8]),
    ]);
    map.effects = vec![effect(
        "gaussian-blur",
        &[("blurriness", ParamValue::Number(12.0))],
    )];
    let map: Arc<Node> = Arc::new(Node::Layer(map));
    let layers = (0..4)
        .map(|i| {
            let x = 30.0 + 80.0 * f64::from(i);
            let mut layer = Layer::new(vec![
                fill(Rect::new(x, 30.0, x + 140.0, 270.0), [0.2, 0.3, 0.9, 1.0]),
                fill(Circle::new((x + 70.0, 150.0), 50.0), [1.0, 1.0, 0.6, 1.0]),
            ]);
            layer.effects = vec![
                effect(
                    "displacement-map",
                    &[
                        ("horizontal", ParamValue::Number(14.0)),
                        ("vertical", ParamValue::Number(9.0)),
                    ],
                )
                .with_input(Arc::clone(&map)),
                effect("glow", &[("radius", ParamValue::Number(10.0))]),
                effect("wrap", &[("blend", ParamValue::Number(0.3))]),
                effect("set-matte", &[]).with_input(Arc::clone(&map)),
                effect(
                    "displacement-map",
                    &[("horizontal", ParamValue::Number(-6.0))],
                )
                .with_input(Arc::clone(&map)),
            ];
            layer.opacity = 0.85;
            layer.matte = Some(Matte {
                source: Box::new(fill(
                    Circle::new((200.0, 150.0), 140.0),
                    [1.0, 1.0, 1.0, 1.0],
                )),
                mode: MatteMode::Alpha,
            });
            layer.transform = Affine::rotate_about(0.05 * f64::from(i), (200.0, 150.0));
            Node::Layer(layer)
        })
        .collect();
    Frame {
        background: Some(Color::rgba(0.97, 0.96, 0.92, 1.0)),
        nodes: layers,
        ..Frame::default()
    }
}

fn draw(frame: &Frame, options: RenderOptions) -> Image {
    render_with(frame, WIDTH, HEIGHT, options).expect("drawn")
}

#[test]
fn a_frame_drawn_with_no_room_is_the_frame_drawn_with_room() {
    let _turn = turn();
    let frame = heavy();
    for options in [RenderOptions::default(), RenderOptions::GOLDEN] {
        memory::set_clobber(true);
        memory::set_budget(u64::MAX);
        let roomy = draw(&frame, options);
        // Again with room: grids now come from the pool, clobbered.
        let pooled = memory::stats().reused;
        let again = draw(&frame, options);
        assert!(memory::stats().reused > pooled, "grids were reused");
        assert!(again == roomy, "a frame drawn from pooled grids changed");
        // No room at all: nothing pooled, nothing kept, read layers drawn
        // again at every read.
        memory::set_budget(0);
        let evicted = memory::stats().evictions;
        let tight = draw(&frame, options);
        assert!(memory::stats().evictions > evicted, "drawings were let go");
        assert!(tight == roomy, "a frame drawn with no room changed");
        // A budget of one grid: some kept, some let go.
        memory::set_budget(u64::from(WIDTH * HEIGHT) * 16);
        assert!(
            draw(&frame, options) == roomy,
            "a frame drawn with one grid's room changed"
        );
        memory::set_budget(memory::DEFAULT_BUDGET);
        memory::set_clobber(false);
    }
}

#[test]
fn pressure_lowers_the_budget_until_it_has_been_calm() {
    let _turn = turn();
    memory::set_budget(1 << 30);
    let frame = heavy();
    let roomy = draw(&frame, RenderOptions::default());
    memory::note_pressure(Pressure::Critical);
    assert_eq!(memory::stats().effective, 0);
    // Normal right after is not yet calm. Checked before drawing, which
    // may itself take longer than the calm on a busy machine.
    memory::note_pressure(Pressure::Normal);
    assert_eq!(memory::stats().effective, 0);
    assert!(draw(&frame, RenderOptions::default()) == roomy);
    std::thread::sleep(memory::CALM + Duration::from_millis(50));
    memory::note_pressure(Pressure::Normal);
    assert_eq!(memory::stats().effective, 1 << 30);
    // A warning keeps at most three quarters of what is held.
    memory::note_pressure(Pressure::Warning);
    assert!(memory::stats().effective < 1 << 30);
    memory::set_budget(memory::DEFAULT_BUDGET);
}

#[test]
fn what_is_held_stays_within_the_budget_but_for_the_grids_being_worked() {
    let _turn = turn();
    let grid = u64::from(WIDTH * HEIGHT) * 16;
    // Room for three grids, which the pool and kept drawings share.
    memory::set_budget(3 * grid);
    let frame = heavy();
    for _ in 0..3 {
        draw(&frame, RenderOptions::default());
    }
    let stats = memory::stats();
    assert!(stats.hits > 0, "{stats:?}");
    // Pooled grids and kept drawings together never pass the budget.
    assert!(stats.peak <= stats.peak_working + 3 * grid, "{stats:?}");
    memory::set_budget(memory::DEFAULT_BUDGET);
}

#[test]
fn grids_of_a_new_size_let_the_pool_go_rather_than_pass_the_budget() {
    let _turn = turn();
    let small = u64::from(WIDTH * HEIGHT) * 16;
    memory::set_budget(4 * small);
    let frame = heavy();
    // The pool fills with grids of one size...
    draw(&frame, RenderOptions::default());
    memory::reset_peaks();
    let trimmed = memory::stats().trimmed;
    // ...and a frame twice as wide and tall makes grids of another.
    render_with(&frame, 2 * WIDTH, 2 * HEIGHT, RenderOptions::default()).expect("drawn");
    let stats = memory::stats();
    // What is held passes the budget only by what is being worked and
    // kept, never by pooled grids beside them.
    assert!(
        stats.peak <= stats.budget.max(stats.peak_working + stats.budget / 2),
        "{stats:?}"
    );
    assert!(stats.trimmed > trimmed, "{stats:?}");
    memory::set_budget(memory::DEFAULT_BUDGET);
}

#[test]
fn what_is_resident_beside_the_grids_leaves_them_less_room() {
    let _turn = turn();
    let grid = u64::from(WIDTH * HEIGHT) * 16;
    memory::set_budget(8 * grid);
    let frame = heavy();
    let roomy = draw(&frame, RenderOptions::default());
    draw(&frame, RenderOptions::default());
    // Nothing is being drawn: what is held is the pool.
    let pooled = memory::stats().held;
    assert!(pooled > grid, "the pool holds grids: {pooled}");
    let trimmed = memory::stats().trimmed;
    // The process holds seven grids' worth beside the grids: they have
    // room for one.
    memory::note_resident(pooled + 7 * grid);
    let stats = memory::stats();
    assert_eq!(stats.outside, 7 * grid, "{stats:?}");
    assert!(stats.trimmed > trimmed, "{stats:?}");
    assert!(stats.held <= grid, "{stats:?}");
    // A smaller reading later does not give the room back: the most seen
    // stands for what the process may hold again.
    memory::note_resident(grid);
    assert_eq!(memory::stats().outside, 7 * grid);
    // Drawn again in that room: the same pixels, and the pool keeps
    // within it once the frame's grids are let go.
    assert!(draw(&frame, RenderOptions::default()) == roomy);
    assert!(memory::stats().held <= grid, "{:?}", memory::stats());
    memory::reset_peaks();
    assert_eq!(memory::stats().outside, 0);
    memory::set_budget(memory::DEFAULT_BUDGET);
}
