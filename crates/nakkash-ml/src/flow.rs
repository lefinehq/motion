//! Smoothing a matte along the motion of its clip ([D87]): never by
//! default, since it lowers flicker by lagging, only when a cutout asks.
//!
//! The flow model (`flow.dense`) gives the motion of every pixel between
//! two 1920 by 1080 frames; a clip of another size is resampled to it and
//! its flow brought back. For each frame after the first, the previous
//! frame's smoothed matte is carried onto it by the backward flow, and the
//! two are blended where the forward and backward flows agree, by
//! `amount · exp(−|carried − matte| / tolerance)`: a large difference, an
//! edge that really moved, keeps the frame's own matte.
//!
//! [D87]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d87-an-objects-matte-is-its-segmentation-refined-at-its-edge

use nakkash_baked::GraphRun;
use nakkash_format::Result;

use crate::image::{Filter, Plane, Rgb8};
use crate::{Backend, Graph, ModelLock, ModelStore, Tensor};

/// The flow model's key in the lock.
pub const MODEL: &str = "flow.dense";

/// The frame size the flow graphs take.
pub const SIZE: (usize, usize) = (1920, 1080);

/// The motion of every pixel of one frame to another, in pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Across.
    pub dx: Plane,
    /// Down.
    pub dy: Plane,
}

/// The flow model, open.
pub struct Flow {
    graph: Box<dyn Graph>,
    run: Option<GraphRun>,
}

impl std::fmt::Debug for Flow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flow")
            .field("run", &self.run)
            .finish_non_exhaustive()
    }
}

impl Flow {
    /// Opens the flow model's full graph (twelve refinements) from `store`.
    ///
    /// # Errors
    /// The store's and the backend's diagnostics.
    pub fn open(
        backend: &dyn Backend,
        store: &ModelStore,
        lock: &ModelLock,
        threads: usize,
    ) -> Result<Self> {
        let graph = store.graph(lock, MODEL, "flow", threads)?;
        Ok(Self {
            run: Some(graph.run()),
            graph: backend.open(&graph)?,
        })
    }

    /// A flow model on a graph already open.
    pub fn from_graph(graph: Box<dyn Graph>) -> Self {
        Self { graph, run: None }
    }

    /// The graph it runs, for a bake's record.
    pub fn run_record(&self) -> Option<&GraphRun> {
        self.run.as_ref()
    }

    /// The motion from `from` to `to`, two frames of one size, at their
    /// size.
    ///
    /// # Errors
    /// The graph's diagnostic.
    pub fn between(&mut self, from: &Rgb8, to: &Rgb8) -> Result<Field> {
        let (w, h) = (from.width, from.height);
        let (fw, fh) = SIZE;
        let nchw = |frame: &Rgb8| {
            let frame = match (frame.width, frame.height) == SIZE {
                true => frame.clone(),
                false => frame.resized(fw, fh, Filter::Triangle),
            };
            let plane = fw * fh;
            let mut out = vec![0f32; 3 * plane];
            for (i, pixel) in frame.data.chunks_exact(3).enumerate() {
                for c in 0..3 {
                    out[c * plane + i] = f32::from(pixel[c]);
                }
            }
            Tensor::f32([1, 3, fh, fw], out)
        };
        let mut out = self.graph.run(vec![
            ("image1".into(), nchw(from)),
            ("image2".into(), nchw(to)),
        ])?;
        let flow = out.floats("flow")?;
        let plane = fw * fh;
        let part = |i: usize, scale: f32| {
            let p = Plane {
                width: fw,
                height: fh,
                data: flow[i * plane..(i + 1) * plane]
                    .iter()
                    .map(|v| v * scale)
                    .collect(),
            };
            match (w, h) == SIZE {
                true => p,
                false => p.resized(w, h, Filter::Triangle),
            }
        };
        Ok(Field {
            dx: part(0, w as f32 / fw as f32),
            dy: part(1, h as f32 / fh as f32),
        })
    }
}

/// `plane` read at each pixel moved by `field`, bilinearly, the edges held.
pub fn warp(plane: &Plane, field: &Field) -> Plane {
    let (w, h) = (plane.width, plane.height);
    let mut out = Plane::filled(w, h, 0.0);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            out.data[i] = sample(
                plane,
                x as f32 + field.dx.data[i],
                y as f32 + field.dy.data[i],
            );
        }
    }
    out
}

/// `plane` at `(x, y)`, bilinearly, clamped inside it.
fn sample(plane: &Plane, x: f32, y: f32) -> f32 {
    let (w, h) = (plane.width, plane.height);
    let x = x.clamp(0.0, (w as f32 - 1.001).max(0.0));
    let y = y.clamp(0.0, (h as f32 - 1.001).max(0.0));
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (ax, ay) = (x - x0 as f32, y - y0 as f32);
    let at = |xx: usize, yy: usize| plane.data[yy * w + xx];
    let top = at(x0, y0) * (1.0 - ax) + at(x1, y0) * ax;
    let bottom = at(x0, y1) * (1.0 - ax) + at(x1, y1) * ax;
    top * (1.0 - ay) + bottom * ay
}

/// Where `backward` (frame t to t−1) and `forward` (t−1 to t) agree: the
/// round trip lands within 1 % of the motion plus half a pixel.
pub fn consistent(backward: &Field, forward: &Field) -> Vec<bool> {
    let fx = warp(&forward.dx, backward);
    let fy = warp(&forward.dy, backward);
    (0..backward.dx.data.len())
        .map(|i| {
            let (bx, by) = (backward.dx.data[i], backward.dy.data[i]);
            let err = (bx + fx.data[i]).powi(2) + (by + fy.data[i]).powi(2);
            let mag = bx * bx + by * by + fx.data[i].powi(2) + fy.data[i].powi(2);
            err < 0.01 * mag + 0.5
        })
        .collect()
}

/// Frame `t`'s matte blended with `previous`, the frame before's smoothed
/// matte, carried by `backward` where `valid`.
pub fn blend(
    matte: &Plane,
    previous: &Plane,
    backward: &Field,
    valid: &[bool],
    amount: f32,
    tolerance: f32,
) -> Plane {
    let carried = warp(previous, backward);
    let data = matte
        .data
        .iter()
        .zip(&carried.data)
        .zip(valid)
        .map(|((&a, &c), &ok)| match ok {
            true => a + amount * (-(c - a).abs() / tolerance).exp() * (c - a),
            false => a,
        })
        .collect();
    Plane {
        width: matte.width,
        height: matte.height,
        data,
    }
}

#[cfg(test)]
#[path = "../tests/unit/flow.rs"]
mod tests;
