//! Draws a [`Frame`] into an [`Image`] with `vello_cpu`: one render context
//! per call and no state kept between calls, so frames can be drawn on many
//! threads at once.

use nakkash_format::{Diagnostic, Kind, Result};
use std::sync::Arc;
use vello_cpu::kurbo::{Affine, Point, Rect, Shape};

use vello_cpu::peniko::{ImageAlphaType, ImageQuality, ImageSampler};
use vello_cpu::{
    ImageSource, Level, Mask, Pixmap, RasterizerSettings, RenderContext, RenderMode,
    RenderSettings, Resources, TargetInit,
};

use crate::blend::BlendMode;
use crate::effect::{Effect, EffectContext, Input, Move, Space};
use crate::homography::Homography;
use crate::image::Image;
use crate::list::{
    Blending, Clip, Frame, GlyphDraw, ImageDraw, Layer, LayerMask, Matte, MatteMode, Node,
    PathDraw, Projection,
};
use crate::par;
use crate::pixels::Pixels;
use crate::projected::{self, Buffer, Extent, Plan};
use crate::reads::Reads;
use crate::style::{Color, DrawStyle};

/// How the rasteriser computes colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// 32-bit float per channel while compositing: no banding from repeated
    /// blending. Pixels effects make stay floats to the end of the frame:
    /// filter layers, compositions, mattes and planes read them, and they
    /// are laid on a float canvas, rounded to 8 bits once. For delivered
    /// frames and golden images.
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
    /// [D12]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d12-what-repeatable-means
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
    let mut painter = Painter::new(w, h, options, Arc::new(Reads::of(&frame.nodes)));
    // The background is drawn as the bottom-most shape, not used to clear the
    // target: the rasteriser keeps a cleared target out of blending, and a
    // layer that multiplies must multiply with the background too.
    if let Some(background) = frame.background {
        painter.ctx.set_paint(background.to_vello());
        painter
            .ctx
            .fill_rect(&Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
    }
    painter.pending = true;
    painter.nodes(&frame.nodes, Affine::IDENTITY)?;
    let rgba = match painter.finish() {
        Drawn::Bytes(pixmap) => pixmap.take_rgba8(ImageAlphaType::Alpha),
        Drawn::Floats(pixels) => pixels.to_rgba8(),
    };
    Image::new(width, height, rgba)
}

/// What a painter drew: the rasteriser's 8-bit target when nothing it drew
/// made pixels of its own, or else the float canvas ([`Quality::Precise`]).
enum Drawn {
    /// As the rasteriser drew it.
    Bytes(Pixmap),
    /// The float canvas.
    Floats(Pixels),
}

impl Drawn {
    /// Its pixels, as floats.
    fn into_pixels(self) -> Pixels {
        match self {
            Drawn::Bytes(pixmap) => Pixels::from_pixmap(&pixmap),
            Drawn::Floats(pixels) => pixels,
        }
    }
}

/// Whether drawing `layer` lays pixels made by the rasteriser's own code
/// rather than the vello context: it adjusts what is below, is a plane, has
/// masks, motion blur of a drawing made once, or an effect that reads its
/// pixels, or a layer under it does. On the float canvas such a layer is
/// drawn alone and laid in floats, and so is every layer it sits in.
fn makes_pixels(layer: &Layer) -> bool {
    layer.adjust
        || layer.projection.is_some()
        || !layer.masks.is_empty()
        || !layer.motion.is_empty()
        || layer.effects.iter().any(Effect::reads_pixels)
        || children_make_pixels(layer)
}

/// Whether a layer among `layer`'s children makes pixels ([`makes_pixels`]).
fn children_make_pixels(layer: &Layer) -> bool {
    layer
        .children
        .iter()
        .any(|child| matches!(child, Node::Layer(layer) if makes_pixels(layer)))
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
///
/// With [`Quality::Fast`] everything is drawn by the vello context, and
/// pixels effects make are laid into it as 8-bit pictures. With
/// [`Quality::Precise`] they are laid on a float canvas instead, the
/// painter's `canvas`: what the context drew is laid on it first, then the
/// context goes on drawing over a clear target. Laying source-over is
/// associative, so a run of paths and normal layers drawn by the context
/// and then laid gives what drawing them on the canvas would; a layer that
/// blends otherwise with what the canvas holds is drawn alone and laid in
/// floats.
struct Painter {
    ctx: RenderContext,
    resources: Resources,
    width: u16,
    height: u16,
    options: RenderOptions,
    /// The float canvas, once something was laid on it.
    canvas: Option<Pixels>,
    /// Whether the context drew anything since it was last laid on the
    /// canvas.
    pending: bool,
    /// How many layers are open in the context.
    open: usize,
    /// The drawings of read layers, shared by the frame's painters.
    reads: Arc<Reads>,
}

impl Painter {
    /// A painter for a `width` by `height` target, drawing with `options`
    /// and sharing the frame's `reads`.
    fn new(width: u16, height: u16, options: RenderOptions, reads: Arc<Reads>) -> Painter {
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
            canvas: None,
            pending: false,
            open: 0,
            reads,
        }
    }

    /// A painter of this one's size and options, to draw something alone.
    fn alone(&self) -> Painter {
        Painter::new(
            self.width,
            self.height,
            self.options,
            Arc::clone(&self.reads),
        )
    }

    /// Whether pixels are laid on a float canvas.
    fn floats(&self) -> bool {
        self.options.quality == Quality::Precise
    }

    /// Renders what the context drew into a cleared pixmap of the target's
    /// size.
    fn rendered(&mut self) -> Pixmap {
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

    /// What was drawn: the float canvas with what the context drew since
    /// laid on it, or what the context drew when nothing was laid.
    fn finish(mut self) -> Drawn {
        match self.canvas.is_some() {
            true => {
                self.canvas_mut();
                Drawn::Floats(self.canvas.take().expect("the canvas was laid on"))
            }
            false => Drawn::Bytes(self.rendered()),
        }
    }

    /// The float canvas, with what the context drew since laid on it; a
    /// clear one if nothing was.
    fn canvas_mut(&mut self) -> &mut Pixels {
        debug_assert_eq!(self.open, 0, "the canvas is laid on outside layers");
        if self.pending {
            let drawn = Pixels::from_pixmap(&self.rendered());
            self.ctx.reset();
            self.pending = false;
            match &mut self.canvas {
                Some(canvas) => canvas.lay(&drawn, BlendMode::Normal, |_| 1.0),
                None => self.canvas = Some(drawn),
            }
        }
        self.canvas
            .get_or_insert_with(|| Pixels::transparent(self.width, self.height))
    }

    /// The mask `matte` makes: its source drawn on its own, in coordinates
    /// `parent` maps to the frame, each pixel read as `matte.mode` says.
    fn mask(&self, matte: &Matte, parent: Affine) -> Result<Mask> {
        let data = self
            .matte_values(matte, parent)?
            .into_iter()
            .map(|v| (v * 255.0).round() as u8)
            .collect();
        Ok(Mask::from_parts(data, self.width, self.height))
    }

    /// How much of a layer `matte` lets show at each pixel, 0 to 1.
    fn matte_values(&self, matte: &Matte, parent: Affine) -> Result<Vec<f32>> {
        let mut painter = self.alone();
        painter.nodes(std::slice::from_ref(&*matte.source), parent)?;
        let pixels = painter.finish().into_pixels();
        let data = par::map(pixels.data(), |_, &[r, g, b, a]| {
            // Premultiplied, so transparent pixels read as black; Rec.
            // 709 weights, as the CSS masking module uses.
            let luma = || (r * 0.2126 + g * 0.7152 + b * 0.0722).clamp(0.0, 1.0);
            let a = a.clamp(0.0, 1.0);
            match matte.mode {
                MatteMode::Alpha => a,
                MatteMode::AlphaInverted => 1.0 - a,
                MatteMode::Luma => luma(),
                MatteMode::LumaInverted => 1.0 - luma(),
            }
        });
        Ok(data)
    }

    /// How much of a layer its `masks` keep at each pixel, in coordinates
    /// `transform` maps to the frame; `None` when they cut nothing.
    fn cut(&self, masks: &[LayerMask], transform: Affine) -> Option<Vec<f32>> {
        if masks.is_empty() {
            return None;
        }
        crate::mask::coverage(masks, transform, self.width, self.height, |mask| {
            let mut painter = self.alone();
            painter.ctx.set_transform(transform);
            painter.ctx.set_paint(Color::WHITE.to_vello());
            painter.ctx.set_fill_rule(vello_cpu::peniko::Fill::NonZero);
            painter.ctx.fill_path(&mask.path);
            par::map(painter.rendered().data(), |_, p| f32::from(p.a) / 255.0)
        })
    }

    /// How much of each pixel of the grid lies inside `clip`, drawn in
    /// coordinates `transform` takes to the grid.
    fn coverage(&self, clip: &Clip, transform: Affine) -> Vec<f32> {
        let (path, rule) = clip.outline();
        let mut painter = self.alone();
        painter.ctx.set_transform(transform);
        painter.ctx.set_paint(Color::WHITE.to_vello());
        painter.ctx.set_fill_rule(rule.to_vello());
        painter.ctx.fill_path(&path);
        par::map(painter.rendered().data(), |_, p| f32::from(p.a) / 255.0)
    }

    /// Draws `nodes` on a canvas of their own and returns its pixels.
    fn drawn_alone(&self, nodes: &[Node], transform: Affine) -> Result<Pixels> {
        let mut painter = self.alone();
        painter.nodes(nodes, transform)?;
        Ok(painter.finish().into_pixels())
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
                let bounds = match &**node {
                    Node::Layer(layer) => layer.bounds,
                    _ => None,
                };
                // Drawn once a frame for each transform it is read at.
                self.reads
                    .read(node, transform, (self.width, self.height), || {
                        self.drawn_alone(std::slice::from_ref(&**node), transform)
                    })
                    .map(|pixels| Input { pixels, bounds })
            })
            .transpose()?;
        Ok(Space {
            transform,
            bounds,
            input,
            time: layer.time,
            sound: effect.sound().cloned(),
            outline: effect.outline().cloned(),
            totals: effect.totals().to_vec(),
        })
    }

    /// Lays `pixels`, the size of the frame, over what is drawn so far: on
    /// the float canvas, or as a picture the context draws.
    fn draw_pixels(&mut self, pixels: &Pixels) {
        if self.floats() {
            self.canvas_mut().lay(pixels, BlendMode::Normal, |_| 1.0);
            return;
        }
        self.pending = true;
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
                self.pending = true;
                self.path(path, transform);
                Ok(())
            }
            Node::Glyphs(glyphs) => {
                self.pending = true;
                self.glyphs(glyphs, transform)
            }
            Node::Image(image) => {
                self.pending = true;
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
                            let mut whole = sub.alone();
                            whole.ctx.set_transform(plan.local);
                            whole.ctx.set_paint(Color::WHITE.to_vello());
                            whole.ctx.fill_rect(&filter.bounds.unwrap_or(plan.region));
                            Pixels::from_pixmap(&whole.rendered())
                        }
                        false => sub.drawn_alone(&filter.children, plan.local)?,
                    };
                    if let Some(cut) = sub.cut(&filter.masks, plan.local) {
                        par::each_with(pixels.data_mut(), &cut, |_, pixel, &kept| {
                            *pixel = pixel.map(|c| c * kept);
                        });
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
                * matte.as_ref().map_or(1.0, |m| m[i])
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
        let steps = Steps::of(layer, self.floats() && children_make_pixels(layer));
        // A transparent layer draws nothing, effects included.
        let opacity = layer.opacity * steps.moved.opacity;
        if opacity <= 0.0 {
            return Ok(());
        }
        if let Some(projection) = &layer.projection {
            return self.projected(layer, projection, &steps, opacity, parent);
        }
        if let Some(flat) = self.whole_box(layer, parent) {
            return self.projected(layer, &flat, &steps, opacity, parent);
        }
        let transform = parent * layer.transform;
        if !layer.is_isolated() {
            return self.nodes(&layer.children, transform);
        }
        // On the float canvas, a layer that makes pixels is laid in floats,
        // and so is one that blends otherwise than source-over with what
        // the canvas holds; one inside a layer open in the context blends
        // with that layer's buffer, which the context holds.
        let blends_with_canvas =
            self.canvas.is_some() && self.open == 0 && layer.blend != BlendMode::Normal;
        if self.floats() && (makes_pixels(layer) || blends_with_canvas) {
            return self.floated(layer, &steps, opacity, parent);
        }
        self.pending = true;
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
        self.open += 1;
        let drawn = self.content(layer, &steps, transform, parent);
        // Pop even after a failure, so the context's layers stay balanced.
        self.ctx.pop_layer();
        self.open -= 1;
        drawn
    }

    /// The plane a flat layer is drawn on when its effects read anywhere in
    /// its box and the box leaves the canvas: its own transform, as a
    /// projection, so it draws on a buffer of its whole box as a projected
    /// layer does. On the canvas's grid, the part of the box off the canvas
    /// would never be drawn, and an effect that reads it (a polar
    /// conversion, a wrap, a lens) would read nothing there. `None` for any
    /// other layer, and for one with motion-blur moves, which a projected
    /// layer takes as samples of its projection instead.
    fn whole_box(&self, layer: &Layer, parent: Affine) -> Option<Projection> {
        if !layer.motion.is_empty() || projected::read_reach(layer).is_some() {
            return None;
        }
        let landed = (parent * layer.transform).transform_rect_bbox(layer.bounds?);
        let canvas = Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height));
        let inside = landed.union(canvas) == canvas;
        (!inside).then(|| Projection::new(Homography::from(layer.transform)))
    }

    /// Draws an isolated flat layer on the float canvas: its content and
    /// effects alone, then laid in floats with its opacity, clip, matte
    /// and blend mode, as the context would lay its buffer.
    fn floated(
        &mut self,
        layer: &Layer,
        steps: &Steps<'_>,
        opacity: f32,
        parent: Affine,
    ) -> Result<()> {
        let transform = parent * layer.transform;
        let mut alone = self.alone();
        alone.content(layer, steps, transform, parent)?;
        let drawn = alone.finish().into_pixels();
        let clip = layer
            .shown_clip()
            .map(|clip| self.coverage(&clip, transform));
        let matte = layer
            .matte
            .as_ref()
            .map(|matte| self.matte_values(matte, parent))
            .transpose()?;
        self.canvas_mut().lay(&drawn, layer.blend, |i| {
            opacity * clip.as_ref().map_or(1.0, |c| c[i]) * matte.as_ref().map_or(1.0, |m| m[i])
        });
        Ok(())
    }

    /// Draws a projected layer ([D58]): its clipped content, masks and
    /// effects on a buffer of its own in its own coordinates, laid onto the
    /// canvas through each of its maps, then blended with `opacity`, its
    /// blend mode and its matte, as a flat layer is.
    ///
    /// [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
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
            if sub.floats() && makes_pixels(layer) {
                // Its content is laid on the buffer's float canvas, cut
                // to the clip there.
                let mut alone = sub.alone();
                alone.content(layer, steps, plan.local, Affine::IDENTITY)?;
                let drawn = alone.finish().into_pixels();
                let clip = sub.coverage(&Clip::Path(path, rule), plan.local);
                sub.canvas_mut().lay(&drawn, BlendMode::Normal, |i| clip[i]);
                return Ok(());
            }
            sub.pending = true;
            sub.ctx.set_transform(plan.local);
            sub.ctx.set_fill_rule(rule.to_vello());
            sub.ctx.push_layer(Some(&path), None, None, None, None);
            sub.open += 1;
            let drawn = sub.content(layer, steps, plan.local, Affine::IDENTITY);
            sub.ctx.pop_layer();
            sub.open -= 1;
            drawn
        })?;
        if self.floats() {
            let matte = layer
                .matte
                .as_ref()
                .map(|matte| self.matte_values(matte, parent))
                .transpose()?;
            self.canvas_mut().lay(&laid, layer.blend, |i| {
                opacity * matte.as_ref().map_or(1.0, |m| m[i])
            });
            return Ok(());
        }
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
        let mut sub = Painter::new(
            plan.width,
            plan.height,
            self.options,
            Arc::clone(&self.reads),
        );
        draw(&mut sub)?;
        let (width, height) = (self.width, self.height);
        Ok(match sub.finish() {
            Drawn::Bytes(pixmap) => {
                projected::lay(Buffer::Bytes(&pixmap), plan, maps, width, height)
            }
            Drawn::Floats(pixels) => {
                projected::lay(Buffer::Floats(&pixels), plan, maps, width, height)
            }
        })
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
            self.pending = true;
            let drawn = self.nodes(&layer.children, transform * moved);
            let mut effects = EffectContext {
                ctx: &mut self.ctx,
                width: self.width,
                height: self.height,
                bounds: layer.bounds.map(|b| transform * b.to_path(0.1)),
            };
            for effect in steps.drawing {
                effect.paint_over(&mut effects);
            }
            return drawn;
        }
        let at = transform * moved;
        let mut pixels = match &layer.shared {
            // Layers with the same children draw them once at each
            // transform.
            Some(shared) => self
                .reads
                .shared(shared, at, (self.width, self.height), || {
                    self.drawn_alone(&layer.children, at)
                })?,
            None => self.drawn_alone(&layer.children, at)?,
        };
        // An effect that draws beyond the box grows the clip the layer
        // shows within, so the content is cut to its own clip first.
        let clipped = match (&layer.clip, layer.effects_grow()) {
            (Some(clip), true) => Some(self.coverage(clip, transform)),
            _ => None,
        };
        for cut in [&cut, &clipped].into_iter().flatten() {
            par::each_with(pixels.data_mut(), cut, |_, pixel, &kept| {
                *pixel = pixel.map(|c| c * kept);
            });
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
        self.set_paint(&draw.paint);
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
        self.ctx.reset_paint_transform();
    }

    /// Paints what is drawn next with `paint`, placed in the coordinates
    /// of the leaf drawn; the caller resets the paint transform after.
    fn set_paint(&mut self, paint: &crate::style::Paint) {
        let (paint, placed) = paint.to_vello();
        self.ctx.set_paint_transform(placed);
        self.ctx.set_paint(paint);
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
        self.set_paint(&draw.paint);
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
        self.ctx.reset_paint_transform();
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
    /// With `floated_children`, its children make pixels on the float
    /// canvas ([`makes_pixels`]), which the context cannot paint over, so
    /// its effects run on its pixels too.
    fn of(layer: &Layer, floated_children: bool) -> Steps<'_> {
        let reads = layer
            .effects
            .iter()
            .position(Effect::reads_pixels)
            .unwrap_or(layer.effects.len());
        let on_canvas = !layer.masks.is_empty() || !layer.motion.is_empty() || floated_children;
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
        if ys.is_empty() || xs.is_empty() {
            continue;
        }
        let rows = &mut sum.data_mut()[ys.start * width..ys.end * width];
        par::rows(
            rows,
            width,
            || (),
            |row, data, ()| {
                let y = ys.start + row;
                for x in xs.clone() {
                    let from = back * Point::new(x as f64 + 0.5, y as f64 + 0.5);
                    let pixel = pixels.sample(from.x, from.y);
                    let total = &mut data[x];
                    for c in 0..4 {
                        total[c] += pixel[c] * share;
                    }
                }
            },
        );
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
