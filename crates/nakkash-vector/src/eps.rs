//! EPS files ([D71]): their PostScript program is run by
//! `nakkash-postscript`, our own interpreter, and what it paints is laid on
//! the [`Canvas`] like any other format's drawing.
//!
//! - **Space.** The drawing's space is the whole-point bounding box, in
//!   points, y down, from its top-left corner; the precise bounding box,
//!   when the file gives one, is its art.
//! - **Colour.** The interpreter gives sRGB: device grey and RGB as they
//!   are, device CMYK by the language's own rule, `1 − min(1, c + k)`,
//!   which is what reference renders of EPS footage show, and indexed,
//!   separation and n-channel colours through their definitions.
//! - **Clips** are a tree in the interpreter's output; here each item opens
//!   the clips it needs below the ones it shares with the item before.
//! - **Line width 0**, the thinnest line a device can draw, is drawn one
//!   point wide, as PDF's is.
//! - **What is not drawn** (text, smooth shading, patterns and the rest the
//!   interpreter names) is said with [`Canvas::unsupported`], and so is an
//!   error or a safety limit that ended the program early.
//!
//! [D71]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d71-eps-footage-is-drawn-by-our-own-postscript-interpreter

use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_postscript::{Item, NoteKind, Page};
use nakkash_raster::kurbo::{self, Affine};
use nakkash_raster::{Cap, Color, Dashes, FillRule, Image, Join, Paint, Picture, StrokeStyle};

use crate::{Canvas, Drawing};

/// Whether a file starting with `head` is a PostScript program or a binary
/// EPS file.
pub(crate) fn recognise(head: &[u8]) -> bool {
    head.starts_with(b"%!PS") || head.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6])
}

/// Runs an EPS file and lays what it paints on a canvas.
pub(crate) fn read(bytes: &[u8]) -> Result<Drawing> {
    let page = nakkash_postscript::read(bytes).map_err(|error| {
        Diagnostic::new(Kind::Invalid, error.to_string()).expected("an EPS file")
    })?;
    let mut canvas = Canvas::new();
    paint(&page, &mut canvas)?;
    Ok(Drawing::new("EPS", page.size, page.art, canvas))
}

/// The clips from the outermost to `clip`.
fn chain(page: &Page, clip: Option<usize>) -> Vec<usize> {
    let mut chain = Vec::new();
    let mut at = clip;
    while let Some(index) = at {
        chain.push(index);
        at = page.clips[index].parent;
    }
    chain.reverse();
    chain
}

fn rule(rule: nakkash_postscript::FillRule) -> FillRule {
    match rule {
        nakkash_postscript::FillRule::NonZero => FillRule::NonZero,
        nakkash_postscript::FillRule::EvenOdd => FillRule::EvenOdd,
    }
}

fn solid(colour: nakkash_postscript::Rgb) -> Paint {
    Paint::Solid(Color {
        r: colour.r,
        g: colour.g,
        b: colour.b,
        a: 1.0,
    })
}

/// The canvas's stroke style for a pen.
fn style(pen: &kurbo::Stroke, transform: Affine) -> StrokeStyle {
    let cap = match pen.start_cap {
        kurbo::Cap::Butt => Cap::Butt,
        kurbo::Cap::Round => Cap::Round,
        kurbo::Cap::Square => Cap::Square,
    };
    let join = match pen.join {
        kurbo::Join::Bevel => Join::Bevel,
        kurbo::Join::Miter => Join::Miter,
        kurbo::Join::Round => Join::Round,
    };
    // The thinnest line is one point wide, measured in the drawing.
    let width = if pen.width > 0.0 {
        pen.width
    } else {
        let scale = transform.determinant().abs().sqrt();
        if scale > 0.0 { 1.0 / scale } else { 1.0 }
    };
    StrokeStyle {
        width,
        cap,
        join,
        miter_limit: pen.miter_limit,
        dashes: (!pen.dash_pattern.is_empty()).then(|| Dashes {
            offset: pen.dash_offset,
            pattern: pen.dash_pattern.to_vec(),
        }),
    }
}

fn paint(page: &Page, canvas: &mut Canvas) -> Result<()> {
    let mut open: Vec<usize> = Vec::new();
    for item in &page.items {
        let (Item::Fill { clip, .. } | Item::Stroke { clip, .. } | Item::Image { clip, .. }) = item;
        let wanted = chain(page, *clip);
        let shared = open.iter().zip(&wanted).take_while(|(a, b)| a == b).count();
        while open.len() > shared {
            canvas.pop();
            open.pop();
        }
        for &index in &wanted[shared..] {
            let clip = &page.clips[index];
            canvas.push_clip(clip.path.clone(), rule(clip.rule));
            open.push(index);
        }
        match item {
            Item::Fill {
                path,
                rule: r,
                colour,
                ..
            } => canvas.fill(path, Affine::IDENTITY, solid(*colour), rule(*r)),
            Item::Stroke {
                path,
                style: pen,
                transform,
                colour,
                ..
            } => canvas.stroke(path, *transform, solid(*colour), &style(pen, *transform)),
            Item::Image {
                raster, transform, ..
            } => {
                let image = Image::new(raster.width, raster.height, raster.pixels.clone())?;
                canvas.picture(Picture::from_image(&image)?, *transform);
            }
        }
    }
    while open.pop().is_some() {
        canvas.pop();
    }
    for note in &page.notes {
        match note.kind {
            NoteKind::Unsupported => canvas.unsupported(note.message.clone()),
            NoteKind::Error | NoteKind::Limit => {
                canvas.unsupported(format!("EPS footage cut short: {}", note.message));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/eps.rs"]
mod tests;
