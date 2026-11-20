//! The pixel gates of the craft lint ([D59]): `contrast` and `flash`.
//! They measure what is drawn, so they draw the scene, at a quarter of its
//! size; `nakkash-query` reads the scene and draws nothing.
//!
//! [D59]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d59-craft-lint-measures-motion-from-the-scene-and-reports-numbers

mod contrast;
mod flash;

use std::sync::LazyLock;

use nakkash_format::Result;
use nakkash_query::Report;
use nakkash_raster::kurbo::Affine;
use nakkash_raster::{BlendMode, Frame, Image, Layer, Node, RenderOptions};

use crate::session::Session;

/// How much smaller than the canvas the gates draw it, on each side.
const SHRINK: f64 = 0.25;

/// Both gates over the whole scene: `flash`, then `contrast`.
pub(crate) fn gates(session: &Session) -> Result<Report> {
    let mut report = flash::check(session)?;
    // `flash` draws every frame whole, unless the scene has one frame.
    let drawn_whole = session.frames()?.len() >= 2;
    report.join(contrast::check(session, drawn_whole)?);
    Ok(report)
}

/// `list`, a frame of `session`'s scene, drawn at [`SHRINK`] of its size.
fn shrunk(session: &Session, list: Frame) -> Result<Image> {
    shrunk_by(session, list, SHRINK, RenderOptions::default())
}

/// `list`, a frame of `session`'s scene, drawn at `scale` of its size with
/// `options`: a picture of the whole frame that costs a fraction of a
/// render, for measures that need no full resolution.
pub(crate) fn shrunk_by(
    session: &Session,
    list: Frame,
    scale: f64,
    options: RenderOptions,
) -> Result<Image> {
    let (width, height) = session.canvas()?;
    let side = |px: u32| ((f64::from(px) * scale).ceil() as u32).max(1);
    let small = Frame {
        background: list.background,
        blending: list.blending,
        nodes: vec![Node::Layer(Layer {
            transform: Affine::scale(scale),
            opacity: 1.0,
            blend: BlendMode::Normal,
            clip: None,
            masks: Vec::new(),
            effects: Vec::new(),
            children: list.nodes,
            isolate: false,
            matte: None,
            adjust: false,
            bounds: None,
            time: 0.0,
            motion: Vec::new(),
            projection: None,
            shared: None,
        })],
    };
    // On the render's threads, as every drawing is: off them, the rows
    // would go to the thread pool library's own pool, one thread per
    // processor whatever `--threads` says.
    crate::session::on_pool(|| {
        nakkash_raster::render_with(&small, side(width), side(height), options)
    })
}

/// Frame `frame` of `session`'s scene drawn at `shrink` of its size: what
/// the sheets of `vary` show and its reframing reads ([D78]).
///
/// [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error
pub(crate) fn small(session: &Session, frame: f64, shrink: f64) -> Result<Image> {
    shrunk_by(
        session,
        session.display_list(frame)?,
        shrink,
        RenderOptions::default(),
    )
}

/// Linear light of each 8-bit sRGB level.
static LINEAR: LazyLock<[f64; 256]> = LazyLock::new(|| {
    std::array::from_fn(|level| {
        let c = level as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
});

/// The relative luminance of an sRGB colour, 0 for black to 1 for white,
/// as WCAG 2 defines it.
fn luminance([r, g, b]: [u8; 3]) -> f64 {
    let lin = |c: u8| LINEAR[usize::from(c)];
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// The colour a straight-alpha RGBA8 px shows over black, blended on the
/// gamma-encoded values as the scene's layers are.
fn over_black(px: &[u8]) -> [u8; 3] {
    let alpha = f64::from(px[3]) / 255.0;
    std::array::from_fn(|c| (f64::from(px[c]) * alpha).round() as u8)
}

#[cfg(test)]
#[path = "../../tests/unit/gates/mod.rs"]
mod tests;
