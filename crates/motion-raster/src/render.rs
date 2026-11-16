//! Draws a [`Frame`] into an [`Image`] with `vello_cpu`: one render context
//! per call and no state kept between calls, so frames can be drawn on many
//! threads at once.

use motion_format::{Diagnostic, Kind, Result};
use std::sync::Arc;
use vello_cpu::kurbo::{Affine, Point, Rect};

use vello_cpu::peniko::{ImageAlphaType, ImageQuality, ImageSampler};
use vello_cpu::{
    ImageSource, Level, Mask, Pixmap, RasterizerSettings, RenderContext, RenderMode,
    RenderSettings, Resources, TargetInit,
};

use crate::effect::{Effect, EffectContext, Input, Move, Space};
use crate::homography::Homography;
use crate::image::Image;
use crate::list::{
    Blending, Clip, Frame, GlyphDraw, ImageDraw, Layer, LayerMask, Matte, MatteMode, Node,
    PathDraw, Projection,
};
use crate::pixels::Pixels;
use crate::projected::{self, Extent, Plan};
use crate::style::{Color, DrawStyle};

/// How the rasteriser computes colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// 32-bit float per channel while compositing: no banding from repeated
    /// blending. For delivered frames and golden images.
    #[default]
    Precise,
    /// 8- and 16-bit integers: faster, for previews.
    Fast,
}

/// Which instruction set the rasteriser uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Simd {
    /// The fastest the processor has. Frames are identical on one kind of
    /// processor, and may differ in the last bit between kinds ([D12]).
    ///
    /// [D12]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d12-what-repeatable-means
    #[default]
    Native,
    /// Plain scalar code: slower, and the same bits on every processor, so
    /// golden images compare exactly across machines.
    Portable,
}

/// How [`render_with`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderOptions {
    /// How colours are computed.
    pub quality: Quality,
    /// Which instruction set is used.
    pub simd: Simd,
}

impl RenderOptions {
    /// Precise and portable: what golden images are drawn with.
    pub const GOLDEN: RenderOptions = RenderOptions {
        quality: Quality::Precise,
        simd: Simd::Portable,
    };

    fn level(self) -> Level {
        match self.simd {
            Simd::Native => Level::try_detect().unwrap_or(Level::baseline()),
            Simd::Portable => Level::fallback(),
        }
    }

    fn mode(self) -> RenderMode {
        match self.quality {
            Quality::Precise => RenderMode::OptimizeQuality,
            Quality::Fast => RenderMode::OptimizeSpeed,
        }
    }
}

/// Draws `frame` into a `width` by `height` image with the default options.
///
/// # Errors
/// See [`render_with`].
pub fn render(frame: &Frame, width: u32, height: u32) -> Result<Image> {
    render_with(frame, width, height, RenderOptions::default())
}

/// Draws `frame` into a `width` by `height` image.
///
/// # Errors
/// An out-of-range diagnostic for a size of zero or above 65535 pixels, an
/// invalid-value diagnostic for linear-light blending, which the rasteriser
/// cannot do yet, one for glyphs a font cannot draw, and a conflict for a
/// projected layer that also has affine motion-blur moves.
pub fn render_with(
    frame: &Frame,
    width: u32,
    height: u32,
    options: RenderOptions,
) -> Result<Image> {
    let (w, h) = (side(width, "width")?, side(height, "height")?);
    if frame.blending == Blending::Linear {
        return Err(Diagnostic::new(
            Kind::Invalid,
            "linear-light blending is not supported yet: the rasteriser blends gamma-encoded values",
        )
        .expected("gamma blending"));
    }
    let mut painter = Painter::new(w, h, options);
    // The background is drawn as the bottom-most shape, not used to clear the
    // target: the rasteriser keeps a cleared target out of blending, and a
    // layer that multiplies must multiply with the background too.
    if let Some(background) = frame.background {
        painter.ctx.set_paint(background.to_vello());
        painter
            .ctx
            .fill_rect(&Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
    }
    painter.nodes(&frame.nodes, Affine::IDENTITY)?;
    let pixmap = painter.finish();
    Image::new(width, height, pixmap.take_rgba8(ImageAlphaType::Alpha))
}

/// One side of the image, which the rasteriser holds in 16 bits.
fn side(pixels: u32, name: &str) -> Result<u16> {
    match u16::try_from(pixels) {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("an image {name} of {pixels} pixels"),
        )
        .expected("1 to 65535 pixels")),
    }
}

/// The render context of one frame and what drawing it needs.
struct Painter {
    ctx: RenderContext,
    resources: Resources,
    width: u16,
    height: u16,
    options: RenderOptions,
}

impl Painter {
    /// A painter for a `width` by `height` target, drawing with `options`.
    fn new(width: u16, height: u16, options: RenderOptions) -> Painter {
        Painter {
            ctx: RenderContext::new_with(
                width,
                height,
                RenderSettings {
                    level: options.level(),
                    num_threads: 0,
                },
            ),
            resources: Resources::new(),
            width,
            height,
            options,
        }
    }

    /// Renders what was drawn into a cleared pixmap of the target's size.
    fn finish(mut self) -> Pixmap {
        self.ctx.flush();
        let mut pixmap = Pixmap::new(self.width, self.height);
        self.ctx.render_with(
            &mut pixmap,
            &mut self.resources,
            RasterizerSettings {
                render_mode: self.options.mode(),
                target_init: TargetInit::Clear(Color::TRANSPARENT.to_vello()),
                ..RasterizerSettings::default()
            },
        );
        pixmap
    }

    /// The mask `matte` makes: its source drawn on its own, in coordinates
    /// `parent` maps to the frame, each pixel read as `matte.mode` says.
    fn mask(&self, matte: &Matte, parent: Affine) -> Result<Mask> {
        let data = self.matte_values(matte, parent)?;
        Ok(Mask::from_parts(data, self.width, self.height))
    }

    /// How much of a layer `matte` lets show at each pixel, 0 to 255.
    fn matte_values(&self, matte: &Matte, parent: Affine) -> Result<Vec<u8>> {
        let mut painter = Painter::new(self.width, self.height, self.options);
        painter.nodes(std::slice::from_ref(&*matte.source), parent)?;
        let pixmap = painter.finish();
        let data = pixmap
            .data()
            .iter()
            .map(|pixel| {
                // Premultiplied, so transparent pixels read as black.
                let luma = || {
                    let [r, g, b] = [pixel.r, pixel.g, pixel.b].map(f32::from);
                    // Rec. 709 weights, as the CSS masking module uses.
                    (r * 0.2126 + g * 0.7152 + b * 0.0722)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                match matte.mode {
                    MatteMode::Alpha => pixel.a,
                    MatteMode::AlphaInverted => 255 - pixel.a,
                    MatteMode::Luma => luma(),
                    MatteMode::LumaInverted => 255 - luma(),
                }
            })
            .collect();
        Ok(data)
    }

    /// How much of a layer its `masks` keep at each pixel, in coordinates
    /// `transform` maps to the frame; `None` when they cut nothing.
    fn cut(&self, masks: &[LayerMask], transform: Affine) -> Option<Vec<f32>> {
        if masks.is_empty() {
            return None;
        }
        crate::mask::coverage(masks, transform, self.width, self.height, |mask| {
            let mut painter = Painter::new(self.width, self.height, self.options);
            painter.ctx.set_transform(transform);
            painter.ctx.set_paint(Color::WHITE.to_vello());
            painter.ctx.set_fill_rule(vello_cpu::peniko::Fill::NonZero);
            painter.ctx.fill_path(&mask.path);
            painter
                .finish()
                .data()
                .iter()
                .map(|p| f32::from(p.a) / 255.0)
                .collect()
        })
    }

    /// How much of each pixel of the grid lies inside `clip`, drawn in
    /// coordinates `transform` takes to the grid.
    fn coverage(&self, clip: &Clip, transform: Affine) -> Vec<f32> {
        let (path, rule) = clip.outline();
        let mut painter = Painter::new(self.width, self.height, self.options);
        painter.ctx.set_transform(transform);
        painter.ctx.set_paint(Color::WHITE.to_vello());
        painter.ctx.set_fill_rule(rule.to_vello());
        painter.ctx.fill_path(&path);
        painter
            .finish()
            .data()
            .iter()
            .map(|p| f32::from(p.a) / 255.0)
            .collect()
    }

    /// Draws `nodes` on a canvas of their own and returns its pixels.
    fn drawn_alone(&self, nodes: &[Node], transform: Affine) -> Result<Pixels> {
        let mut painter = Painter::new(self.width, self.height, self.options);
        painter.nodes(nodes, transform)?;
        Ok(Pixels::from_pixmap(&painter.finish()))
    }

    /// Where `effect` runs on a layer whose own coordinates `transform`
    /// takes to the frame: with the drawing of the layer it reads, if any,
    /// laid on those coordinates.
    fn space(
        &self,
        effect: &Effect,
        transform: Affine,
        layer: &Layer,
        bounds: Option<Rect>,
    ) -> Result<Space> {
        let input = effect
            .input()
            .map(|node| {
                let bounds = match node {
                    Node::Layer(layer) => layer.bounds,
                    _ => None,
                };
                self.drawn_alone(std::slice::from_ref(node), transform)
                    .map(|pixels| Input { pixels, bounds })
            })
            .transpose()?;
        Ok(Space {
            transform,
            bounds,
            input,
            time: layer.time,
            sound: effect.sound().cloned(),
        })
    }

    /// Lays `pixels`, the size of the frame, over what is drawn so far.
    fn draw_pixels(&mut self, pixels: &Pixels) {
        let paint = vello_cpu::Image {
            image: ImageSource::Pixmap(Arc::new(pixels.to_pixmap())),
            // Pixel for pixel: the grid lines up with the frame's.
            sampler: ImageSampler::new().with_quality(ImageQuality::Low),
        };
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.reset_paint_transform();
        self.ctx.set_paint(paint);
        self.ctx.fill_rect(&Rect::new(
            0.0,
            0.0,
            f64::from(self.width),
            f64::from(self.height),
        ));
    }

    /// Draws `nodes` back to front, in coordinates `transform` maps to the
    /// frame. A filter layer among them applies its effects to what the
    /// nodes before it drew.
    fn nodes(&mut self, nodes: &[Node], transform: Affine) -> Result<()> {
        let filter = nodes
            .iter()
            .rposition(|node| matches!(node, Node::Layer(layer) if layer.adjust));
        if let Some(at) = filter {
            let Node::Layer(filter) = &nodes[at] else {
                unreachable!("the position of a filter layer");
            };
            self.filter(&nodes[..at], filter, transform)?;
            return self.nodes(&nodes[at + 1..], transform);
        }
        nodes.iter().try_for_each(|node| match node {
            Node::Layer(layer) => self.layer(layer, transform),
            Node::Path(path) => {
                self.path(path, transform);
                Ok(())
            }
            Node::Glyphs(glyphs) => self.glyphs(glyphs, transform),
            Node::Image(image) => {
                self.image(image, transform);
                Ok(())
            }
        })
    }

    /// Draws `below`, then `filter`'s effects on it: what they give is
    /// mixed in by the filter's opacity, its matte, and the alpha of what
    /// its children draw, its region; without children, everywhere.
    ///
    /// A projected filter's region is drawn on its plane, masks included,
    /// and laid onto the canvas; its effects run on the canvas, measured by
    /// the affine map that matches its projection where the canvas sees
    /// the middle of it.
    fn filter(&mut self, below: &[Node], filter: &Layer, parent: Affine) -> Result<()> {
        let drawn = self.drawn_alone(below, parent)?;
        if filter.opacity <= 0.0 || filter.effects.is_empty() {
            self.draw_pixels(&drawn);
            return Ok(());
        }
        let (transform, region, cut) = match &filter.projection {
            None => {
                let transform = parent * filter.transform;
                // Where it draws, by its alpha; a filter layer drawing
                // nothing applies everywhere.
                let region = match filter.children.is_empty() {
                    true => None,
                    false => Some(self.drawn_alone(&filter.children, transform)?),
                };
                (transform, region, self.cut(&filter.masks, transform))
            }
            Some(projection) => {
                let maps = [Homography::from(parent) * projection.matrix];
                let extent = match (filter.children.is_empty(), filter.bounds) {
                    (false, _) => projected::nodes_extent(&filter.children),
                    (true, Some(bounds)) => Extent::Box(bounds),
                    (true, None) => Extent::Everywhere,
                };
                let reach = projected::mask_reach(&filter.masks);
                let Some(plan) = Plan::new(extent, Some(reach), &maps, self.width, self.height)
                else {
                    // None of it is seen, so it applies nowhere.
                    self.draw_pixels(&drawn);
                    return Ok(());
                };
                let region = self.laid(&plan, &maps, |sub| {
                    let mut pixels = match filter.children.is_empty() {
                        true => {
                            let mut whole = Painter::new(sub.width, sub.height, sub.options);
                            whole.ctx.set_transform(plan.local);
                            whole.ctx.set_paint(Color::WHITE.to_vello());
                            whole.ctx.fill_rect(&filter.bounds.unwrap_or(plan.region));
                            Pixels::from_pixmap(&whole.finish())
                        }
                        false => sub.drawn_alone(&filter.children, plan.local)?,
                    };
                    if let Some(cut) = sub.cut(&filter.masks, plan.local) {
                        for (pixel, kept) in pixels.data_mut().iter_mut().zip(cut) {
                            *pixel = pixel.map(|c| c * kept);
                        }
                    }
                    sub.draw_pixels(&pixels);
                    Ok(())
                })?;
                let transform = maps[0]
                    .tangent(plan.seen)
                    .unwrap_or(parent * filter.transform);
                (transform, Some(region), None)
            }
        };
        let mut changed = drawn.clone();
        let mut bounds = filter.bounds;
        for effect in &filter.effects {
            let space = self.space(effect, transform, filter, bounds)?;
            effect.apply(&mut changed, &space);
            bounds = bounds.map(|b| effect.grown(b));
        }
        let matte = filter
            .matte
            .as_ref()
            .map(|matte| self.matte_values(matte, parent))
            .transpose()?;
        let mut mixed = drawn;
        mixed.mix(&changed, |i| {
            cut.as_ref().map_or(1.0, |c| c[i])
                * filter.opacity
                * matte.as_ref().map_or(1.0, |m| f32::from(m[i]) / 255.0)
                * region.as_ref().map_or(1.0, |r| r.data()[i][3])
        });
        self.draw_pixels(&mixed);
        Ok(())
    }

    /// Draws a layer: its children, in a buffer of their own when it is
    /// isolated, then its effects on that buffer, then lays it on what is
    /// below with its opacity, blend mode, clip and matte. A projected
    /// layer's buffer is in its own coordinates and laid through its
    /// projection.
    fn layer(&mut self, layer: &Layer, parent: Affine) -> Result<()> {
        let steps = Steps::of(layer);
        // A transparent layer draws nothing, effects included.
        let opacity = layer.opacity * steps.moved.opacity;
        if opacity <= 0.0 {
            return Ok(());
        }
        if let Some(projection) = &layer.projection {
            return self.projected(layer, projection, &steps, opacity, parent);
        }
        let transform = parent * layer.transform;
        if !layer.is_isolated() {
            return self.nodes(&layer.children, transform);
        }
        // The matte is drawn in the coordinates the layer's parent gives, as
        // the layer's siblings are; the rasteriser applies masks untransformed.
        let mask = layer
            .matte
            .as_ref()
            .map(|matte| self.mask(matte, parent))
            .transpose()?;
        self.ctx.set_transform(transform);
        let clip = layer.shown_clip().as_ref().map(Clip::outline);
        if let Some((_, rule)) = &clip {
            self.ctx.set_fill_rule(rule.to_vello());
        }
        self.ctx.push_layer(
            clip.as_ref().map(|(path, _)| path),
            Some(layer.blend.to_vello()),
            Some(opacity),
            mask,
            None,
        );
        let drawn = self.content(layer, &steps, transform, parent);
        // Pop even after a failure, so the context's layers stay balanced.
        self.ctx.pop_layer();
        drawn
    }

    /// Draws a projected layer ([D58]): its clipped content, masks and
    /// effects on a buffer of its own in its own coordinates, laid onto the
    /// canvas through each of its maps, then blended with `opacity`, its
    /// blend mode and its matte, as a flat layer is.
    ///
    /// [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
    fn projected(
        &mut self,
        layer: &Layer,
        projection: &Projection,
        steps: &Steps<'_>,
        opacity: f32,
        parent: Affine,
    ) -> Result<()> {
        if !layer.motion.is_empty() {
            return Err(Diagnostic::new(
                Kind::Conflict,
                "a projected layer also has affine motion-blur moves",
            )
            .expected("motion blur of a projected layer as `Projection::samples`"));
        }
        let to_canvas = Homography::from(parent);
        let maps: Vec<Homography> = match projection.samples.is_empty() {
            true => vec![to_canvas * projection.matrix],
            false => projection.samples.iter().map(|s| to_canvas * *s).collect(),
        };
        let extent = projected::extent(layer);
        let reach = projected::read_reach(layer);
        let Some(plan) = Plan::new(extent, reach, &maps, self.width, self.height) else {
            return Ok(());
        };
        let laid = self.laid(&plan, &maps, |sub| {
            // The buffer holds the layer alone, so only a clip needs a
            // layer of its own in it: what effects paint over the content
            // composes with the buffer's own px either way.
            let Some((path, rule)) = layer.shown_clip().as_ref().map(Clip::outline) else {
                return sub.content(layer, steps, plan.local, Affine::IDENTITY);
            };
            sub.ctx.set_transform(plan.local);
            sub.ctx.set_fill_rule(rule.to_vello());
            sub.ctx.push_layer(Some(&path), None, None, None, None);
            let drawn = sub.content(layer, steps, plan.local, Affine::IDENTITY);
            sub.ctx.pop_layer();
            drawn
        })?;
        let mask = layer
            .matte
            .as_ref()
            .map(|matte| self.mask(matte, parent))
            .transpose()?;
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.push_layer(
            None,
            Some(layer.blend.to_vello()),
            Some(opacity),
            mask,
            None,
        );
        self.draw_pixels(&laid);
        self.ctx.pop_layer();
        Ok(())
    }

    /// What `draw` paints on a painter of `plan`'s buffer, laid onto this
    /// painter's grid through each of `maps` and averaged.
    fn laid(
        &self,
        plan: &Plan,
        maps: &[Homography],
        draw: impl FnOnce(&mut Painter) -> Result<()>,
    ) -> Result<Pixels> {
        let mut sub = Painter::new(plan.width, plan.height, self.options);
        draw(&mut sub)?;
        let buffer = sub.finish();
        Ok(projected::lay(&buffer, plan, maps, self.width, self.height))
    }

    /// Draws a layer's content inside the buffer it was given, in
    /// coordinates `transform` maps to this painter's grid: its children,
    /// then its effects. Moves before the first effect that reads pixels
    /// apply as the children are drawn; from that effect on, the children
    /// are drawn on a canvas of their own and every effect runs in order
    /// on its pixels. `parent` places its motion-blur moves.
    fn content(
        &mut self,
        layer: &Layer,
        steps: &Steps<'_>,
        transform: Affine,
        parent: Affine,
    ) -> Result<()> {
        let moved = steps.moved.transform;
        let cut = self.cut(&layer.masks, transform * moved);
        if steps.on_pixels.is_empty() && cut.is_none() && layer.motion.is_empty() {
            let drawn = self.nodes(&layer.children, transform * moved);
            let mut effects = EffectContext {
                ctx: &mut self.ctx,
                width: self.width,
                height: self.height,
            };
            for effect in steps.drawing {
                effect.paint_over(&mut effects);
            }
            return drawn;
        }
        let mut pixels = self.drawn_alone(&layer.children, transform * moved)?;
        // An effect that draws beyond the box grows the clip the layer
        // shows within, so the content is cut to its own clip first.
        let clipped = match (&layer.clip, layer.effects_grow()) {
            (Some(clip), true) => Some(self.coverage(clip, transform)),
            _ => None,
        };
        for cut in [&cut, &clipped].into_iter().flatten() {
            for (pixel, kept) in pixels.data_mut().iter_mut().zip(cut) {
                *pixel = pixel.map(|c| c * kept);
            }
        }
        // Each effect sees the box the ones before it grew.
        let mut bounds = layer.bounds;
        for effect in steps.on_pixels {
            let space = self.space(effect, transform, layer, bounds)?;
            effect.apply(&mut pixels, &space);
            bounds = bounds.map(|b| effect.grown(b));
        }
        if !layer.motion.is_empty() {
            pixels = moved_average(&pixels, &layer.motion, parent);
        }
        self.draw_pixels(&pixels);
        Ok(())
    }

    fn path(&mut self, draw: &PathDraw, transform: Affine) {
        self.ctx.set_transform(transform);
        self.ctx.set_paint(draw.paint.to_vello());
        match &draw.style {
            DrawStyle::Fill(rule) => {
                self.ctx.set_fill_rule(rule.to_vello());
                self.ctx.fill_path(&draw.path);
            }
            DrawStyle::Stroke(stroke) => {
                self.ctx.set_stroke(stroke.to_vello());
                self.ctx.stroke_path(&draw.path);
            }
        }
    }

    /// Fills the draw's rectangle with its picture, whose pixels the paint
    /// transform stretches over it, sampled from the copy that suits the
    /// size it lands at.
    fn image(&mut self, draw: &ImageDraw, transform: Affine) {
        let rect = draw.rect;
        let stretch = |width: u32, height: u32| {
            Affine::translate((rect.x0, rect.y0))
                * Affine::scale_non_uniform(
                    rect.width() / f64::from(width),
                    rect.height() / f64::from(height),
                )
        };
        // How far one picture pixel reaches on the target, along whichever
        // of its sides is drawn longer.
        let [a, b, c, d, _, _] =
            (transform * stretch(draw.picture.width(), draw.picture.height())).as_coeffs();
        let (paint, width, height) = draw.picture.paint(a.hypot(b).max(c.hypot(d)));
        self.ctx.set_transform(transform);
        self.ctx.set_paint_transform(stretch(width, height));
        self.ctx.set_paint(paint);
        self.ctx.fill_rect(&rect);
        // Later paints are placed by their geometry alone.
        self.ctx.reset_paint_transform();
    }

    fn glyphs(&mut self, draw: &GlyphDraw, transform: Affine) -> Result<()> {
        self.ctx.set_transform(transform);
        self.ctx.set_paint(draw.paint.to_vello());
        match &draw.style {
            DrawStyle::Fill(rule) => self.ctx.set_fill_rule(rule.to_vello()),
            DrawStyle::Stroke(stroke) => self.ctx.set_stroke(stroke.to_vello()),
        }
        let mut run = self
            .ctx
            .glyph_run(&mut self.resources, &draw.font)
            .font_size(draw.size)
            .hint(false)
            .normalized_coords(&draw.coords);
        if let Some(glyph_transform) = draw.glyph_transform {
            run = run.glyph_transform(glyph_transform);
        }
        let glyphs = draw.glyphs.iter().map(|g| vello_cpu::Glyph {
            id: g.id,
            x: g.x,
            y: g.y,
        });
        let drawn = match &draw.style {
            DrawStyle::Fill(_) => run.fill_glyphs(glyphs),
            DrawStyle::Stroke(_) => run.stroke_glyphs(glyphs),
        };
        drawn.map_err(|error| {
            Diagnostic::new(Kind::Invalid, format!("glyphs could not be drawn: {error}"))
                .expected("glyph ids the font holds")
        })
    }
}

/// How a layer's effects split between drawing its content and running on
/// its pixels after.
struct Steps<'a> {
    /// The effects applied as the content is drawn: moves, and changes
    /// painted over it.
    drawing: &'a [Effect],
    /// The effects run in order on the content's pixels.
    on_pixels: &'a [Effect],
    /// What the moves among `drawing` do together.
    moved: Move,
}

impl Steps<'_> {
    /// How `layer`'s effects split. Masks cut the content before any
    /// effect but moves, on its pixels, and a drawing moved for motion
    /// blur is made on pixels too, so with either only the moves before
    /// the first effect that reads pixels apply as it is drawn.
    fn of(layer: &Layer) -> Steps<'_> {
        let reads = layer
            .effects
            .iter()
            .position(Effect::reads_pixels)
            .unwrap_or(layer.effects.len());
        let on_canvas = !layer.masks.is_empty() || !layer.motion.is_empty();
        let (drawing, on_pixels) = match reads < layer.effects.len() || on_canvas {
            true => {
                let moves = layer.effects[..reads]
                    .iter()
                    .take_while(|e| e.movement().is_some())
                    .count();
                layer.effects.split_at(moves)
            }
            false => (layer.effects.as_slice(), &[][..]),
        };
        // Moving effects apply as the content is drawn, in the order they
        // stand: each moves what the ones before it moved.
        let moved = drawing.iter().filter_map(Effect::movement).fold(
            Move {
                transform: Affine::IDENTITY,
                opacity: 1.0,
            },
            |done, next| Move {
                transform: next.transform * done.transform,
                opacity: done.opacity * next.opacity,
            },
        );
        Steps {
            drawing,
            on_pixels,
            moved,
        }
    }
}

/// `pixels` laid at each of `moves`, given in the coordinates `parent`
/// takes to the frame, and averaged: motion blur of a drawing made once.
/// Only the pixels where a move can bring something are worked out.
fn moved_average(pixels: &Pixels, moves: &[Affine], parent: Affine) -> Pixels {
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let mut sum = Pixels::transparent(pixels.width(), pixels.height());
    let Some(drawn) = drawn_box(pixels) else {
        return sum;
    };
    let share = 1.0 / moves.len() as f32;
    for &step in moves {
        let to_frame = parent * step * parent.inverse();
        let back = to_frame.inverse();
        // Where the drawn box lands, a pixel wider for the bilinear reach.
        let landed = to_frame.transform_rect_bbox(drawn).inflate(1.0, 1.0);
        let xs =
            (landed.x0.floor().max(0.0) as usize)..(landed.x1.ceil().min(width as f64) as usize);
        let ys =
            (landed.y0.floor().max(0.0) as usize)..(landed.y1.ceil().min(height as f64) as usize);
        let data = sum.data_mut();
        for y in ys {
            for x in xs.clone() {
                let from = back * Point::new(x as f64 + 0.5, y as f64 + 0.5);
                let pixel = pixels.sample(from.x, from.y);
                let total = &mut data[y * width + x];
                for c in 0..4 {
                    total[c] += pixel[c] * share;
                }
            }
        }
    }
    sum
}

/// The box of the pixels of `pixels` that are not clear, in pixels of the
/// grid, or `None` when every one is.
fn drawn_box(pixels: &Pixels) -> Option<Rect> {
    let width = usize::from(pixels.width());
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, pixel) in pixels.data().iter().enumerate() {
        if pixel[3] > 0.0 {
            let (x, y) = (i % width, i / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x0 < x1).then(|| Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64))
}

#[cfg(test)]
#[path = "../tests/unit/render.rs"]
mod tests;
