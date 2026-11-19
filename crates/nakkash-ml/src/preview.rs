//! The contact sheet `nakkash ml preview` writes: frames of the clip with
//! the propagated mask over them, so a prompt on the wrong object is seen
//! before a long bake ([D87]).
//!
//! The sheet shows `count` frames spread evenly over the clip, the prompted
//! frames always among them, left to right and top to bottom in clip order,
//! each `tile` px wide. Inside the mask the frame is tinted; the mask's edge
//! is drawn solid; a prompt's box is drawn as a frame and its points as
//! squares, green on the object and red off it; followed points, when
//! given, as small squares.
//!
//! [D87]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d87-an-objects-matte-is-its-segmentation-refined-at-its-edge

use std::io::Cursor;

use nakkash_format::{Diagnostic, Kind, Result};

use crate::image::{Filter, Plane, Rgb8};
use crate::segment::{Frames, Prompt};

/// The tint inside the mask, and its weight.
const TINT: [u8; 3] = [255, 0, 200];
const TINT_WEIGHT: f32 = 0.45;
/// The mask's edge.
const EDGE: [u8; 3] = [255, 230, 0];
/// Prompts.
const ON: [u8; 3] = [0, 230, 80];
const OFF: [u8; 3] = [255, 40, 40];
/// Followed points.
const POINT: [u8; 3] = [0, 200, 255];
/// The gap between tiles.
const GAP: usize = 4;

/// The frames a sheet of `count` shows from a clip of `n`: spread evenly,
/// the prompted frames among them, in order.
pub fn chosen(n: usize, count: usize, prompted: &[usize]) -> Vec<usize> {
    let count = count.max(1).min(n);
    let mut frames: Vec<usize> = (0..count)
        .map(|i| {
            if count == 1 {
                0
            } else {
                i * (n - 1) / (count - 1)
            }
        })
        .collect();
    frames.extend(prompted.iter().copied().filter(|&p| p < n));
    frames.sort_unstable();
    frames.dedup();
    frames
}

/// The sheet of `frames` with `masks` (one plane of logits per clip frame)
/// over them, `prompts` and `points` (per clip frame, where each followed
/// point is seen) drawn, as PNG bytes; and the frames it shows.
///
/// # Errors
/// The clip's diagnostics, and a [`Kind::Invalid`] one when the sheet does
/// not encode.
pub fn sheet(
    frames: &mut dyn Frames,
    masks: &[Plane],
    prompts: &[Prompt],
    points: &[Vec<[f64; 2]>],
    count: usize,
    tile: usize,
) -> Result<(Vec<u8>, Vec<usize>)> {
    let n = frames.len();
    let prompted: Vec<usize> = prompts.iter().map(|p| p.frame).collect();
    let shown = chosen(n, count, &prompted);
    let (w, h) = frames.size();
    let scale = tile as f32 / w as f32;
    let tile_h = ((h as f32 * scale).round() as usize).max(1);
    let columns = (shown.len() as f64).sqrt().ceil() as usize;
    let rows = shown.len().div_ceil(columns);
    let (sheet_w, sheet_h) = (columns * (tile + GAP) + GAP, rows * (tile_h + GAP) + GAP);
    let mut sheet = Rgb8 {
        width: sheet_w,
        height: sheet_h,
        data: vec![24; sheet_w * sheet_h * 3],
    };
    for (k, &f) in shown.iter().enumerate() {
        let mut t = frames.frame(f)?.resized(tile, tile_h, Filter::Triangle);
        if let Some(mask) = masks.get(f) {
            overlay(&mut t, &mask.resized(tile, tile_h, Filter::Triangle));
        }
        for prompt in prompts.iter().filter(|p| p.frame == f) {
            if let Some([x0, y0, x1, y1]) = prompt.area {
                rectangle(
                    &mut t,
                    [x0 * scale, y0 * scale, x1 * scale, y1 * scale],
                    ON,
                    2,
                );
            }
            for (list, color) in [(&prompt.include, ON), (&prompt.exclude, OFF)] {
                for [x, y] in list {
                    dot(&mut t, x * scale, y * scale, 4, color);
                }
            }
        }
        if let Some(seen) = points.get(f) {
            for [x, y] in seen {
                dot(&mut t, *x as f32 * scale, *y as f32 * scale, 2, POINT);
            }
        }
        let (ox, oy) = (
            GAP + (k % columns) * (tile + GAP),
            GAP + (k / columns) * (tile_h + GAP),
        );
        for y in 0..tile_h {
            let from = y * tile * 3;
            let to = ((oy + y) * sheet_w + ox) * 3;
            sheet.data[to..to + tile * 3].copy_from_slice(&t.data[from..from + tile * 3]);
        }
    }
    Ok((encode(&sheet)?, shown))
}

/// Tints `tile` inside the mask of `logits` and draws its edge.
fn overlay(tile: &mut Rgb8, logits: &Plane) {
    let (w, h) = (tile.width, tile.height);
    let inside = |x: usize, y: usize| logits.data[y * w + x] > 0.0;
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            let edge = x == 0
                || y == 0
                || x + 1 == w
                || y + 1 == h
                || !inside(x - 1, y)
                || !inside(x + 1, y)
                || !inside(x, y - 1)
                || !inside(x, y + 1);
            let p = &mut tile.data[(y * w + x) * 3..(y * w + x) * 3 + 3];
            for c in 0..3 {
                p[c] = match edge {
                    true => EDGE[c],
                    false => (f32::from(p[c]) * (1.0 - TINT_WEIGHT)
                        + f32::from(TINT[c]) * TINT_WEIGHT)
                        .round() as u8,
                };
            }
        }
    }
}

/// Sets the pixel at `(x, y)` of `tile` when inside it.
fn put(tile: &mut Rgb8, x: i64, y: i64, color: [u8; 3]) {
    if x >= 0 && y >= 0 && (x as usize) < tile.width && (y as usize) < tile.height {
        let i = (y as usize * tile.width + x as usize) * 3;
        tile.data[i..i + 3].copy_from_slice(&color);
    }
}

/// A rectangle's outline, `thick` px.
fn rectangle(tile: &mut Rgb8, [x0, y0, x1, y1]: [f32; 4], color: [u8; 3], thick: i64) {
    let (x0, y0, x1, y1) = (
        x0.round() as i64,
        y0.round() as i64,
        x1.round() as i64,
        y1.round() as i64,
    );
    for t in 0..thick {
        for x in x0..=x1 {
            put(tile, x, y0 + t, color);
            put(tile, x, y1 - t, color);
        }
        for y in y0..=y1 {
            put(tile, x0 + t, y, color);
            put(tile, x1 - t, y, color);
        }
    }
}

/// A filled square `2 r + 1` px wide around `(x, y)`.
fn dot(tile: &mut Rgb8, x: f32, y: f32, r: i64, color: [u8; 3]) {
    let (cx, cy) = (x.round() as i64, y.round() as i64);
    for dy in -r..=r {
        for dx in -r..=r {
            put(tile, cx + dx, cy + dy, color);
        }
    }
}

/// `sheet` as PNG bytes.
fn encode(sheet: &Rgb8) -> Result<Vec<u8>> {
    let failed = |e: png::EncodingError| {
        Diagnostic::new(Kind::Invalid, format!("a sheet that does not encode: {e}"))
    };
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            Cursor::new(&mut bytes),
            sheet.width as u32,
            sheet.height as u32,
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(failed)?
            .write_image_data(&sheet.data)
            .map_err(failed)?;
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "../tests/unit/preview.rs"]
mod tests;
