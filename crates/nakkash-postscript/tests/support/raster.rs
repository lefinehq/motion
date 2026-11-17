//! A minimal rasteriser for pages in tests: fills and strokes through the
//! CPU rasteriser, inside their clips, at a given number of pixels a point.
//! Pictures are checked as items, not drawn here.

use kurbo::{Affine, BezPath, Rect};
use nakkash_postscript::{FillRule, Item, Page, Rgb};
use vello_cpu::peniko::{Color, Fill};
use vello_cpu::{Pixmap, RenderContext};

/// A drawn page: straight-alpha RGBA pixels, row by row.
pub struct Drawn {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Drawn {
    /// The pixel at `(x, y)`.
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) is off the page"
        );
        self.pixels[(y * self.width + x) as usize]
    }

    /// The pixel at the point `(x, y)` of page space, at `scale`.
    pub fn at_point(&self, scale: f64, x: f64, y: f64) -> [u8; 4] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        self.at((x * scale) as u32, (y * scale) as u32)
    }
}

fn rule(rule: FillRule) -> Fill {
    match rule {
        FillRule::NonZero => Fill::NonZero,
        FillRule::EvenOdd => Fill::EvenOdd,
    }
}

fn colour(c: Rgb) -> Color {
    Color::new([c.r, c.g, c.b, 1.0])
}

/// The chain of clips from the outermost to `clip`.
fn chain(page: &Page, clip: Option<usize>) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = clip;
    while let Some(index) = at {
        out.push(index);
        at = page.clips[index].parent;
    }
    out.reverse();
    out
}

/// Draws `page` at `scale` pixels a point.
pub fn draw(page: &Page, scale: f64) -> Drawn {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (
        (page.size.width * scale).ceil().max(1.0) as u16,
        (page.size.height * scale).ceil().max(1.0) as u16,
    );
    let mut ctx = RenderContext::new(w, h);
    let base = Affine::scale(scale);
    let mut open: Vec<usize> = Vec::new();
    for item in &page.items {
        let clip = match item {
            Item::Fill { clip, .. } | Item::Stroke { clip, .. } | Item::Image { clip, .. } => *clip,
        };
        let wanted = chain(page, clip);
        let common = open.iter().zip(&wanted).take_while(|(a, b)| a == b).count();
        while open.len() > common {
            ctx.pop_layer();
            open.pop();
        }
        for &index in &wanted[common..] {
            let c = &page.clips[index];
            ctx.set_transform(base);
            ctx.set_fill_rule(rule(c.rule));
            ctx.push_clip_layer(&c.path);
            open.push(index);
        }
        match item {
            Item::Fill {
                path,
                rule: r,
                colour: c,
                ..
            } => {
                ctx.set_transform(base);
                ctx.set_fill_rule(rule(*r));
                ctx.set_paint(colour(*c));
                ctx.fill_path(path);
            }
            Item::Stroke {
                path,
                style,
                transform,
                colour: c,
                ..
            } => {
                ctx.set_transform(base * *transform);
                ctx.set_paint(colour(*c));
                ctx.set_stroke(style.clone());
                ctx.stroke_path(path);
            }
            Item::Image { .. } => {}
        }
    }
    while open.pop().is_some() {
        ctx.pop_layer();
    }
    ctx.flush();
    let mut pixmap = Pixmap::new(w, h);
    ctx.render(&mut pixmap, &mut vello_cpu::Resources::new());
    let pixels = pixmap
        .data()
        .iter()
        .map(|p| {
            if p.a == 0 {
                return [0, 0, 0, 0];
            }
            let un =
                |v: u8| ((u32::from(v) * 255 + u32::from(p.a) / 2) / u32::from(p.a)).min(255) as u8;
            [un(p.r), un(p.g), un(p.b), p.a]
        })
        .collect();
    Drawn {
        width: u32::from(w),
        height: u32::from(h),
        pixels,
    }
}

/// A rectangle as a path, for comparisons.
#[allow(dead_code)]
pub fn rect_path(r: Rect) -> BezPath {
    kurbo::Shape::to_path(&r, 0.01)
}
