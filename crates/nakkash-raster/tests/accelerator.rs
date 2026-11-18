//! The painter's hand-over to an accelerator, checked without a GPU: a
//! mirror that holds pixels as the CPU's own and runs every operation with
//! the CPU's own functions must draw every frame bit for bit as
//! [`render_with`] does ([D74]). It claims every effect, so pixels move
//! into it as early as the painter lets them, and it counts what it was
//! handed, so a frame that never reached it fails too.
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

use std::sync::Mutex;

use nakkash_format::Result;
use nakkash_raster::kurbo::{Affine, BezPath, Circle, Rect, Shape};
use nakkash_raster::{
    Accelerator, Amount, BLEND_MODES, BlendMode, Color, DrawStyle, Effect, FillRule, Frame, Layer,
    LayerMask, MaskMode, Matte, MatteMode, Node, ParamValue, PathDraw, Pixels, RenderOptions,
    Resident, Share, Space, Tally, render_on, render_with,
};
use rstest::rstest;

/// An accelerator that is the CPU: what it holds is `Pixels`.
#[derive(Default)]
struct Mirror {
    counts: Mutex<Tally>,
}

fn pixels(held: &Resident) -> &Pixels {
    held.held::<Pixels>().expect("the mirror holds pixels")
}

fn pixels_mut(held: &mut Resident) -> &mut Pixels {
    held.held_mut::<Pixels>().expect("the mirror holds pixels")
}

fn hold(pixels: Pixels) -> Resident {
    Resident::new(pixels.width(), pixels.height(), Box::new(pixels))
}

/// As the painter reads a matte.
fn matte_value(p: [f32; 4], mode: MatteMode) -> f32 {
    let [r, g, b, a] = p;
    let luma = (r * 0.2126 + g * 0.7152 + b * 0.0722).clamp(0.0, 1.0);
    let a = a.clamp(0.0, 1.0);
    match mode {
        MatteMode::Alpha => a,
        MatteMode::AlphaInverted => 1.0 - a,
        MatteMode::Luma => luma,
        MatteMode::LumaInverted => 1.0 - luma,
    }
}

/// The amount at pixel `i`, multiplied in the order the shares stand.
fn amount_at(amount: &Amount<'_>, i: usize) -> f32 {
    amount
        .shares
        .iter()
        .fold(amount.opacity, |k, share| match share {
            Share::Values(values) => k * values[i],
            Share::Alpha(held) => k * pixels(held).data()[i][3],
            Share::Matte(held, mode) => k * matte_value(pixels(held).data()[i], *mode),
        })
}

impl Mirror {
    fn count(&self, f: impl FnOnce(&mut Tally)) {
        f(&mut self.counts.lock().unwrap());
    }
}

impl Accelerator for Mirror {
    fn holds(&self, _: u16, _: u16) -> bool {
        true
    }

    fn upload(&self, pixels: &Pixels) -> Result<Resident> {
        self.count(|t| t.uploads.0 += 1);
        Ok(hold(pixels.clone()))
    }

    fn upload_bytes(&self, rgba: &[u8], width: u16, height: u16) -> Result<Resident> {
        self.count(|t| t.uploads.0 += 1);
        let data = rgba
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]].map(|c| f32::from(c) / 255.0))
            .collect();
        Ok(hold(Pixels::from_data(width, height, data)))
    }

    fn transparent(&self, width: u16, height: u16) -> Result<Resident> {
        Ok(hold(Pixels::transparent(width, height)))
    }

    fn duplicate(&self, held: &Resident) -> Result<Resident> {
        Ok(hold(pixels(held).clone()))
    }

    fn download(&self, held: &Resident) -> Result<Pixels> {
        self.count(|t| t.downloads.0 += 1);
        Ok(pixels(held).clone())
    }

    fn lay(
        &self,
        canvas: &mut Resident,
        source: &Resident,
        mode: BlendMode,
        amount: &Amount<'_>,
    ) -> Result<()> {
        self.count(|t| t.operations += 1);
        pixels_mut(canvas).lay(pixels(source), mode, |i| amount_at(amount, i));
        Ok(())
    }

    fn mix(&self, held: &mut Resident, other: &Resident, amount: &Amount<'_>) -> Result<()> {
        self.count(|t| t.operations += 1);
        pixels_mut(held).mix(pixels(other), |i| amount_at(amount, i));
        Ok(())
    }

    fn cut(&self, held: &mut Resident, kept: &[f32]) -> Result<()> {
        self.count(|t| t.operations += 1);
        for (p, k) in pixels_mut(held).data_mut().iter_mut().zip(kept) {
            *p = p.map(|c| c * k);
        }
        Ok(())
    }

    fn transformed(&self, held: &Resident, transform: Affine) -> Result<Resident> {
        Ok(hold(pixels(held).transformed(transform)))
    }

    fn runs(&self, _: &Effect) -> bool {
        true
    }

    fn effect(&self, held: &mut Resident, effect: &Effect, space: &Space) -> Result<()> {
        let name = effect.spec().name.to_owned();
        self.count(|t| t.accelerated.push((name, 1)));
        effect.apply(pixels_mut(held), space);
        Ok(())
    }

    fn fell_back(&self, _: &Effect) {}

    fn tally(&self) -> Tally {
        self.counts.lock().unwrap().clone()
    }
}

fn solid(r: f32, g: f32, b: f32, a: f32) -> nakkash_raster::Paint {
    nakkash_raster::Paint::Solid(Color::rgba(r, g, b, a))
}

fn fill(path: BezPath, paint: nakkash_raster::Paint) -> Node {
    Node::Path(PathDraw {
        path,
        paint,
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

fn shapes() -> Vec<Node> {
    vec![
        fill(
            Rect::new(10.0, 8.0, 60.0, 50.0).to_path(0.1),
            solid(0.9, 0.3, 0.1, 1.0),
        ),
        fill(
            Circle::new((55.0, 40.0), 22.0).to_path(0.1),
            solid(0.1, 0.5, 0.9, 0.7),
        ),
    ]
}

fn effect(name: &str, params: &[(&str, ParamValue)]) -> Effect {
    Effect::new(name, params).expect("an effect")
}

fn glow() -> Effect {
    effect(
        "glow",
        &[
            ("threshold", ParamValue::Number(0.3)),
            ("radius", ParamValue::Number(12.0)),
        ],
    )
}

fn blur() -> Effect {
    effect("gaussian-blur", &[("blurriness", ParamValue::Number(8.0))])
}

/// A frame of a dark background and `nodes`.
fn frame(nodes: Vec<Node>) -> Frame {
    Frame {
        background: Some(Color::rgba(0.05, 0.05, 0.1, 1.0)),
        nodes,
        ..Frame::default()
    }
}

/// Layers whose pixels the painter hands over in every way it can.
fn scenes() -> Vec<(&'static str, Frame)> {
    let matte = |mode| Matte {
        source: Box::new(Node::Layer(Layer::new(vec![fill(
            Circle::new((48.0, 36.0), 26.0).to_path(0.1),
            solid(0.8, 0.8, 0.8, 0.9),
        )]))),
        mode,
    };
    let blended: Vec<Node> = BLEND_MODES
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            Node::Layer(Layer {
                blend: spec.mode,
                transform: Affine::translate((i as f64 * 3.0, i as f64)),
                effects: vec![blur()],
                ..Layer::new(shapes())
            })
        })
        .collect();
    vec![
        (
            "an effect",
            frame(vec![Node::Layer(Layer {
                effects: vec![glow()],
                ..Layer::new(shapes())
            })]),
        ),
        (
            "effects then a move",
            frame(vec![Node::Layer(Layer {
                effects: vec![
                    blur(),
                    effect(
                        "transform",
                        &[
                            ("position", ParamValue::Point([6.0, -3.0])),
                            ("rotate", ParamValue::Number(10.0)),
                            ("opacity", ParamValue::Number(0.7)),
                        ],
                    ),
                    effect("invert", &[]),
                    effect("drop-shadow", &[("softness", ParamValue::Number(6.0))]),
                ],
                ..Layer::new(shapes())
            })]),
        ),
        ("every blend mode over a held canvas", frame(blended)),
        (
            "a matte and masks",
            frame(vec![
                Node::Layer(Layer {
                    effects: vec![blur()],
                    ..Layer::new(shapes())
                }),
                Node::Layer(Layer {
                    matte: Some(matte(MatteMode::Luma)),
                    masks: vec![LayerMask {
                        path: Circle::new((40.0, 30.0), 25.0).to_path(0.1),
                        mode: MaskMode::Add,
                        inverted: false,
                        opacity: 0.8,
                        feather: [3.0, 3.0],
                        expand: 0.0,
                    }],
                    blend: BlendMode::Screen,
                    effects: vec![glow()],
                    ..Layer::new(shapes())
                }),
            ]),
        ),
        (
            "a filter layer with a region and a matte",
            frame(vec![
                Node::Layer(Layer {
                    effects: vec![glow()],
                    ..Layer::new(shapes())
                }),
                Node::Layer(Layer {
                    adjust: true,
                    opacity: 0.8,
                    effects: vec![blur(), effect("invert", &[])],
                    matte: Some(matte(MatteMode::Alpha)),
                    ..Layer::new(vec![fill(
                        Rect::new(20.0, 10.0, 80.0, 60.0).to_path(0.1),
                        solid(1.0, 1.0, 1.0, 0.6),
                    )])
                }),
            ]),
        ),
    ]
}

#[rstest]
#[case::precise(RenderOptions::GOLDEN)]
#[case::fast(RenderOptions { quality: nakkash_raster::Quality::Fast, ..RenderOptions::GOLDEN })]
fn a_mirror_of_the_cpu_draws_every_frame_as_the_cpu_does(#[case] options: RenderOptions) {
    for (name, frame) in scenes() {
        let mirror = Mirror::default();
        let reference = render_with(&frame, 96, 72, options).expect("drawn");
        let handed = render_on(&frame, 96, 72, options, Some(&mirror)).expect("drawn");
        assert!(reference == handed, "{name}: the mirror's frame differs");
        let tally = mirror.tally();
        assert!(
            !tally.accelerated.is_empty(),
            "{name}: no effect was handed over"
        );
    }
}
