//! A frame's work recorded at once: many operations submitted together,
//! buffers let go of and taken again before the GPU has run what reads
//! them, several threads recording into one GPU, and a frame read back as
//! the bytes it is written in; and layers reading a layer the GPU drew.

use std::sync::Arc;

use nakkash_raster::kurbo::{Affine, Rect, Shape};
use nakkash_raster::{
    Accelerator, Amount, BlendMode, Color, DrawStyle, Effect, FillRule, Frame, Layer, Node, Paint,
    ParamValue, PathDraw, Pixels, RenderOptions, Share, render_on,
};

use super::{Random, check, gpu, pattern, space};

/// A chain of `steps` operations on a canvas, on the CPU and on the GPU,
/// with nothing read back until the end: each step lays a layer made for
/// it and then let go of, so the pool hands its buffer to the next step
/// while the work that reads it is still only recorded. Every fifth step
/// blurs the canvas, every seventh mixes it with a cut copy.
fn chain(gpu: &nakkash_gpu::Gpu, seed: u64, steps: usize) -> (Pixels, Pixels) {
    let (width, height) = (96u16, 64u16);
    let mut random = Random(seed);
    let blur = Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(3.0))])
        .expect("an effect");
    let modes = [BlendMode::Normal, BlendMode::Screen, BlendMode::Multiply];
    let mut cpu = pattern(width, height, seed);
    let mut canvas = gpu.upload(&cpu).expect("uploaded");
    for step in 0..steps {
        let shift = Affine::translate((
            f64::from(random.next()) * 20.0 - 10.0,
            f64::from(random.next()) * 20.0 - 10.0,
        ));
        let layer = pattern(width, height, seed * 1000 + step as u64).transformed(shift);
        let mode = modes[step % modes.len()];
        let opacity = 0.3 + 0.5 * random.next();
        cpu.lay(&layer, mode, |_| opacity);
        let held = gpu.upload(&layer).expect("uploaded");
        gpu.lay(&mut canvas, &held, mode, &Amount::constant(opacity))
            .expect("laid");
        drop(held);
        if step % 5 == 4 {
            blur.apply(&mut cpu, &space(Affine::IDENTITY));
            gpu.effect(&mut canvas, &blur, &space(Affine::IDENTITY))
                .expect("blurred");
        }
        if step % 7 == 6 {
            let cut: Vec<f32> = (0..cpu.data().len()).map(|_| random.next()).collect();
            let other = cpu.transformed(Affine::translate((3.0, 1.0)));
            cpu.mix(&other, |i| cut[i] * 0.5);
            let moved = gpu
                .transformed(&canvas, Affine::translate((3.0, 1.0)))
                .expect("moved");
            gpu.mix(
                &mut canvas,
                &moved,
                &Amount::constant(0.5).and(Share::Values(&cut)),
            )
            .expect("mixed");
        }
    }
    (cpu, gpu.download(&canvas).expect("downloaded"))
}

#[test]
fn a_long_chain_recorded_at_once_matches() {
    let Some(gpu) = gpu() else { return };
    let before = gpu.tally().submissions;
    // Past the operations one submission takes, more than once.
    let (cpu, on_gpu) = chain(gpu, 21, 700);
    check("chain", "700 steps", &cpu, &on_gpu);
    let submissions = gpu.tally().submissions - before;
    assert!(
        submissions >= 2,
        "a long chain is submitted in parts ({submissions})"
    );
}

#[test]
fn work_stays_recorded_until_its_pixels_are_read() {
    let Some(gpu) = gpu() else { return };
    let pixels = pattern(64, 48, 22);
    let layer = pattern(64, 48, 23);
    // Other tests share the GPU, so this one counts only while it holds
    // the recorder alone: a short chain read back once.
    let mut canvas = gpu.upload(&pixels).expect("uploaded");
    let held = gpu.upload(&layer).expect("uploaded");
    for _ in 0..10 {
        gpu.lay(
            &mut canvas,
            &held,
            BlendMode::Screen,
            &Amount::constant(0.2),
        )
        .expect("laid");
    }
    let before = gpu.tally();
    let mut cpu = pixels.clone();
    for _ in 0..10 {
        cpu.lay(&layer, BlendMode::Screen, |_| 0.2);
    }
    let back = gpu.download(&canvas).expect("downloaded");
    check("chain", "ten lays", &cpu, &back);
    let after = gpu.tally();
    assert!(
        after.submissions > before.submissions,
        "a read submits what is recorded"
    );
}

#[test]
fn threads_recording_at_once_each_get_their_own_pixels() {
    let Some(gpu) = gpu() else { return };
    let results: Vec<(Pixels, Pixels)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|t| scope.spawn(move || chain(gpu, 30 + t, 300)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a thread"))
            .collect()
    });
    for (t, (cpu, on_gpu)) in results.iter().enumerate() {
        check("chain", &format!("thread {t}"), cpu, on_gpu);
    }
}

/// `Pixels::to_rgba8`'s rounding, as the CPU writes a frame.
fn rgba8(pixels: &Pixels) -> Vec<u8> {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    pixels
        .data()
        .iter()
        .flat_map(|&[r, g, b, a]| {
            let a = a.clamp(0.0, 1.0);
            match byte(a) {
                0 => [0; 4],
                alpha => [byte(r / a), byte(g / a), byte(b / a), alpha],
            }
        })
        .collect()
}

#[test]
fn a_frame_read_back_as_bytes_rounds_as_the_cpu_does() {
    let Some(gpu) = gpu() else { return };
    let mut pixels = pattern(313, 97, 24);
    // Values past the ends too, as a sum of lays can leave them.
    let mut random = Random(25);
    for p in pixels.data_mut().iter_mut().step_by(11) {
        *p = p.map(|c| c * (1.0 + random.next()) - 0.1 * random.next());
    }
    let cpu = rgba8(&pixels);
    let held = gpu.upload(&pixels).expect("uploaded");
    let bytes = gpu.download_rgba8(&held).expect("downloaded");
    assert_eq!(bytes.len(), cpu.len());
    let differ = cpu.iter().zip(&bytes).filter(|(a, b)| a != b).count();
    let largest = cpu
        .iter()
        .zip(&bytes)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    eprintln!(
        "pack 313x97: largest {largest} levels, {differ} of {} channels",
        cpu.len()
    );
    assert!(largest <= 1, "a frame's bytes differ by {largest} levels");
    // A level flips only where a quotient sits on a rounding boundary.
    assert!(differ * 1000 <= cpu.len(), "{differ} channels differ");
}

/// A square of `colour` from `(x, y)`, `side` px wide.
fn square(x: f64, y: f64, side: f64, colour: Color) -> Node {
    Node::Path(PathDraw {
        path: Rect::new(x, y, x + side, y + side).to_path(0.1),
        paint: Paint::Solid(colour),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// A layer of `children` with `effects`.
fn layer(children: Vec<Node>, effects: Vec<Effect>) -> Node {
    let mut layer = Layer::new(children);
    layer.effects = effects;
    Node::Layer(layer)
}

#[test]
fn layers_reading_a_layer_drawn_on_the_gpu_match_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let blur = |r: f32| {
        Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(r))]).expect("a blur")
    };
    // Blurred, so the frame draws it on the GPU and leaves it there.
    let map = Arc::new(layer(
        vec![
            square(10.0, 10.0, 60.0, Color::rgba(0.9, 0.2, 0.1, 1.0)),
            square(50.0, 40.0, 70.0, Color::rgba(0.1, 0.6, 0.9, 0.8)),
        ],
        vec![blur(6.0)],
    ));
    let reading = |name: &str| {
        Effect::new(name, &[])
            .expect("an effect")
            .with_input(Arc::clone(&map))
    };
    let content = || {
        vec![
            square(0.0, 0.0, 160.0, Color::rgba(0.3, 0.8, 0.4, 1.0)),
            square(30.0, 20.0, 50.0, Color::rgba(1.0, 1.0, 0.2, 1.0)),
        ]
    };
    // Read by kernels on the GPU, and by one on the CPU.
    for name in ["displacement-map", "set-matte", "contour-strokes"] {
        let frame = Frame {
            background: Some(Color::rgba(1.0, 1.0, 1.0, 1.0)),
            nodes: vec![layer(content(), vec![blur(1.0), reading(name)])],
            ..Frame::default()
        };
        let (width, height) = (160, 120);
        let cpu = render_on(&frame, width, height, RenderOptions::default(), None).expect("drawn");
        let on_gpu = render_on(&frame, width, height, RenderOptions::default(), Some(gpu))
            .expect("drawn on the GPU");
        let largest = cpu
            .pixels()
            .iter()
            .zip(on_gpu.pixels())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        eprintln!("{name} reading a layer drawn on the GPU: largest {largest} levels");
        assert!(largest <= 1, "{name}: frames differ by {largest} levels");
    }
}

#[test]
fn a_frame_with_nothing_for_the_gpu_does_not_touch_it() {
    if gpu().is_none() {
        return;
    }
    // A GPU of its own, so no other test's work is counted.
    let gpu = nakkash_gpu::Gpu::open().expect("a GPU, as the shared one opened");
    let mut faded = Layer::new(vec![square(
        20.0,
        10.0,
        60.0,
        Color::rgba(0.2, 0.4, 0.9, 1.0),
    )]);
    faded.opacity = 0.5;
    let blur =
        Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(3.0))]).expect("a blur");
    let frame = Frame {
        background: Some(Color::rgba(1.0, 1.0, 1.0, 1.0)),
        nodes: vec![
            square(0.0, 0.0, 50.0, Color::rgba(0.9, 0.2, 0.1, 1.0)),
            Node::Layer(faded),
        ],
        ..Frame::default()
    };
    let drawn = render_on(&frame, 96, 64, RenderOptions::default(), Some(&gpu)).expect("drawn");
    let cpu = render_on(&frame, 96, 64, RenderOptions::default(), None).expect("drawn");
    assert_eq!(drawn.pixels(), cpu.pixels(), "drawn on the CPU, to the bit");
    let tally = gpu.tally();
    assert_eq!(
        (tally.uploads.0, tally.downloads.0, tally.submissions),
        (0, 0, 0),
        "{tally:?}"
    );
    // One effect it runs, and the frame is drawn there.
    let mut blurred = Layer::new(vec![square(
        20.0,
        10.0,
        60.0,
        Color::rgba(0.2, 0.4, 0.9, 1.0),
    )]);
    blurred.effects = vec![blur];
    let frame = Frame {
        nodes: vec![Node::Layer(blurred)],
        ..frame
    };
    render_on(&frame, 96, 64, RenderOptions::default(), Some(&gpu)).expect("drawn");
    let tally = gpu.tally();
    assert!(tally.submissions > 0, "{tally:?}");
}
