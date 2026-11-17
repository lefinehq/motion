//! PDF pages, and the `.ai` files that are PDF inside ([D70]).
//!
//! The file's objects are read by `hayro-syntax`, and its first page's
//! drawing operators are interpreted by `hayro-interpret`, which keeps the
//! graphics state (transforms, clips, colours, line styles, `q`/`Q`), reads
//! the external graphics states, resolves colour spaces, shadings, patterns,
//! fonts, form and image objects and soft masks, and calls a device for
//! what is painted. The device here, [`Reader`], paints it on our
//! [`Canvas`], so the rasteriser draws it.
//!
//! - **Space.** The drawing's space is the page's crop box (inside its media
//!   box), in points, y down, turned as the page says it is turned.
//! - **Colour.** Every colour space is converted to sRGB by the interpreter:
//!   DeviceGray and DeviceRGB as they are, DeviceCMYK through a built-in
//!   CMYK profile, ICCBased through the file's own profile, and Lab, CalRGB,
//!   CalGray, Indexed, Separation and DeviceN through their definitions.
//!   Colours then blend in gamma-encoded sRGB, as every layer does.
//! - **Shadings.** An axial or radial shading becomes a gradient whose stops
//!   follow its function within 1/100 of a channel; one that a gradient
//!   cannot draw exactly (an end it does not extend, a background), and
//!   function and mesh shadings, are sampled into a picture of
//!   [`TEXTURE_DENSITY`] pixels a point, at most [`MAX_TEXTURE`] a side.
//! - **Text** is drawn from the glyph outlines of the fonts the file
//!   embeds. A font it does not embed is reported, and its text is not drawn.
//! - **Line width 0**, the thinnest line a device can draw, is drawn one
//!   point wide.
//!
//! [D70]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, PoisonError};

use hayro_interpret::encode::{EncodedShadingPattern, EncodedShadingType};
use hayro_interpret::font::Glyph;
use hayro_interpret::gradient::SvgGradientKind;
use hayro_interpret::pattern::{Pattern, ShadingPattern, TilingPattern};
use hayro_interpret::util::{RectExt, TransformExt};
use hayro_interpret::{
    CacheKey, ClipPath, Context, Device, DrawMode, DrawProps, Image as PdfImage, ImageData,
    ImageDrawProps, InterpreterCache, InterpreterSettings, InterpreterWarning, LumaData, MaskType,
    RenderContent, SoftMask, StrokeProps, interpret_page,
};
use hayro_syntax::Pdf;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object};
use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_raster::kurbo::{self, Affine, BezPath, Rect, Shape, Size};
use nakkash_raster::{
    BlendMode, Cap, Color, Dashes, FillRule, Gradient, GradientShape, Image, Join, Layer, Matte,
    MatteMode, Node, Paint, Picture, Stop, StrokeStyle,
};

use crate::canvas::{Canvas, Group};
use crate::drawing::Drawing;

/// How many pixels a point a shading a gradient cannot draw is sampled at.
pub const TEXTURE_DENSITY: f64 = 4.0;

/// The most pixels a side a sampled shading has.
pub const MAX_TEXTURE: f64 = 2048.0;

/// How far, in channel values from 0 to 1, a shading's gradient stops may
/// stray from its function between them.
const STOP_TOLERANCE: f32 = 0.01;

/// The most cells of a tiling pattern one fill draws.
pub const MAX_CELLS: usize = 4096;

/// How finely outlines made from strokes follow their curves, in the
/// drawing's units.
const STROKE_TOLERANCE: f64 = 0.01;

/// Whether a file's first bytes are a PDF file's: its header, `%PDF-`,
/// within them, as readers allow a little junk before it.
pub(crate) fn recognise(head: &[u8]) -> bool {
    head.windows(5).any(|window| window == b"%PDF-")
}

/// Reads the first page of the PDF file `bytes`.
///
/// # Errors
/// An invalid-value diagnostic when the file is not a PDF the reader can
/// follow, is encrypted with a password, or has no page.
pub(crate) fn read(bytes: &[u8]) -> Result<Drawing> {
    let pdf = Pdf::new(bytes.to_vec()).map_err(|error| {
        Diagnostic::new(Kind::Invalid, format!("PDF: {error:?}")).expected("a PDF file")
    })?;
    let pages = pdf.pages();
    let page = pages.first().ok_or_else(|| {
        Diagnostic::new(Kind::Invalid, "a PDF without pages").expected("a PDF with a page")
    })?;
    let (width, height) = page.render_dimensions();
    let size = Size::new(f64::from(width), f64::from(height));
    let initial = page.initial_transform(true).to_kurbo();
    let art = page
        .raw()
        .get::<hayro_syntax::object::Rect>(ART_BOX)
        .map(|art| initial.transform_rect_bbox(art.to_kurbo()));
    let warnings: Arc<Mutex<BTreeSet<&'static str>>> = Arc::default();
    let sink = Arc::clone(&warnings);
    let settings = InterpreterSettings {
        // Text in fonts the file does not embed is reported, not guessed.
        font_resolver: Arc::new(|_| None),
        cmap_resolver: Arc::new(|_| None),
        warning_sink: Arc::new(move |warning| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(warned(warning));
        }),
        // The page's own drawing; annotations are not artwork.
        render_content: RenderContent::PAGE,
    };
    let cache = InterpreterCache::new();
    let bounds = size.to_rect();
    let mut context = Context::new(initial, bounds, &cache, pdf.xref(), settings);
    let mut reader = Reader::new(bounds);
    interpret_page(page, &mut context, &mut reader);
    let mut canvas = reader.canvas;
    for warning in warnings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
    {
        canvas.unsupported(*warning);
    }
    scan(&pdf, &mut canvas);
    Ok(Drawing::new("PDF", size, art, canvas))
}

/// The key of a page's art box.
const ART_BOX: &[u8] = b"ArtBox";

/// What a warning of the interpreter means for what is drawn, as the
/// report says it.
fn warned(warning: InterpreterWarning) -> &'static str {
    match warning {
        InterpreterWarning::UnsupportedFont => {
            "text in a font the file does not embed or that cannot be read (not drawn)"
        }
        InterpreterWarning::ImageDecodeFailure => "an image that cannot be decoded (not drawn)",
        _ => "an object the PDF reader could not draw",
    }
}

/// Notes on `canvas` what the file asks for that the interpreter leaves
/// out, found in its dictionaries, indirect or written inside others:
/// overprinting, transfer functions and knockout groups.
fn scan(pdf: &Pdf, canvas: &mut Canvas) {
    for object in pdf.objects() {
        match object {
            Object::Dict(dict) => scan_dict(&dict, canvas, 0),
            Object::Stream(stream) => scan_dict(stream.dict(), canvas, 0),
            Object::Array(array) => scan_array(&array, canvas, 0),
            _ => {}
        }
    }
}

/// How deep [`scan`] follows dictionaries and arrays written inside
/// others; indirect objects are scanned on their own.
const SCAN_DEPTH: usize = 16;

/// Notes what `dict` asks for that is not drawn, then looks inside it.
fn scan_dict(dict: &Dict<'_>, canvas: &mut Canvas, depth: usize) {
    if [b"OP".as_slice(), b"op"]
        .iter()
        .any(|key| dict.get::<bool>(key).unwrap_or(false))
    {
        canvas.unsupported("overprinting (drawn as if off)");
    }
    let identity = |object: Option<Object<'_>>| match object {
        None => true,
        Some(Object::Name(name)) => [b"Identity".as_slice(), b"Default"]
            .iter()
            .any(|known| name.as_ref() == *known),
        Some(_) => false,
    };
    // A soft mask's own transfer function is reported where it is drawn.
    let soft_mask = dict.contains_key(b"G");
    if !soft_mask
        && (!identity(dict.get::<Object<'_>>(b"TR")) || !identity(dict.get::<Object<'_>>(b"TR2")))
    {
        canvas.unsupported("transfer functions (drawn without them)");
    }
    let transparency = dict
        .get::<Name<'_>>(b"S")
        .is_some_and(|name| name.as_ref() == b"Transparency");
    if transparency && dict.get::<bool>(b"K").unwrap_or(false) {
        canvas.unsupported("knockout groups (drawn as plain groups)");
    }
    if depth >= SCAN_DEPTH {
        return;
    }
    for (_, value) in dict.entries() {
        match value {
            MaybeRef::NotRef(Object::Dict(inner)) => scan_dict(&inner, canvas, depth + 1),
            MaybeRef::NotRef(Object::Array(inner)) => scan_array(&inner, canvas, depth + 1),
            _ => {}
        }
    }
}

/// Looks inside the dictionaries and arrays written in `array`.
fn scan_array(array: &Array<'_>, canvas: &mut Canvas, depth: usize) {
    if depth >= SCAN_DEPTH {
        return;
    }
    for value in array.raw_iter() {
        match value {
            MaybeRef::NotRef(Object::Dict(inner)) => scan_dict(&inner, canvas, depth + 1),
            MaybeRef::NotRef(Object::Array(inner)) => scan_array(&inner, canvas, depth + 1),
            _ => {}
        }
    }
}

/// The device the interpreter paints through: each paint becomes nodes on
/// the canvas, in the page's space.
struct Reader {
    /// What is painted.
    canvas: Canvas,
    /// The page, which shadings without bounds fill.
    page: Rect,
    /// Soft masks drawn so far, by the interpreter's key for them: one mask
    /// usually applies to many paints.
    mattes: HashMap<u128, Matte>,
}

impl Reader {
    fn new(page: Rect) -> Reader {
        Reader {
            canvas: Canvas::new(),
            page,
            mattes: HashMap::new(),
        }
    }

    /// Runs `paint` inside a group with `mask` and `blend`, when either asks
    /// for one.
    fn composited<'a>(
        &mut self,
        mask: Option<&SoftMask<'a>>,
        blend: hayro_interpret::BlendMode,
        paint: impl FnOnce(&mut Reader),
    ) {
        let mask = mask.map(|mask| self.matte(mask));
        let blend = blend_mode(blend);
        let grouped = mask.is_some() || blend != BlendMode::Normal;
        if grouped {
            self.canvas.push_group(Group {
                opacity: 1.0,
                blend,
                mask,
            });
        }
        paint(self);
        if grouped {
            self.canvas.pop();
        }
    }

    /// The matte a soft mask is: its group drawn, read by its alpha or its
    /// luminosity over its backdrop colour.
    fn matte<'a>(&mut self, mask: &SoftMask<'a>) -> Matte {
        let key = mask.cache_key();
        if let Some(matte) = self.mattes.get(&key) {
            return matte.clone();
        }
        let mut inner = Reader::new(self.page);
        mask.interpret(&mut inner);
        let (mut nodes, unsupported) = inner.canvas.finish();
        unsupported
            .into_iter()
            .for_each(|what| self.canvas.unsupported(what));
        if mask.transfer_function().is_some() {
            self.canvas
                .unsupported("soft masks with a transfer function (drawn without it)");
        }
        let mode = match mask.mask_type() {
            MaskType::Alpha => MatteMode::Alpha,
            MaskType::Luminosity => {
                // Where the group paints nothing, the mask is its backdrop's
                // brightness; the matte reads nothing as black.
                let backdrop = color(&mask.background_color());
                if backdrop.r > 0.0 || backdrop.g > 0.0 || backdrop.b > 0.0 {
                    let everywhere = self.page.inflate(self.page.width(), self.page.height());
                    let mut fill = Canvas::new();
                    fill.fill(
                        &everywhere.to_path(0.1),
                        Affine::IDENTITY,
                        Paint::Solid(Color { a: 1.0, ..backdrop }),
                        FillRule::NonZero,
                    );
                    nodes.splice(0..0, fill.finish().0);
                }
                MatteMode::Luma
            }
        };
        let matte = Matte {
            source: Box::new(Node::Layer(Layer::new(nodes))),
            mode,
        };
        self.mattes.insert(key, matte.clone());
        matte
    }

    /// Fills `path`, in coordinates `transform` maps to the page's.
    fn fill(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &hayro_interpret::Paint<'_>,
        rule: FillRule,
    ) {
        match paint {
            hayro_interpret::Paint::Color(c) => {
                self.canvas
                    .fill(path, transform, Paint::Solid(color(c)), rule);
            }
            hayro_interpret::Paint::Pattern(pattern) => {
                self.patterned(&(transform * path.clone()), rule, pattern, false);
            }
        }
    }

    /// Strokes `path`, in coordinates `transform` maps to the page's.
    fn stroke(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &hayro_interpret::Paint<'_>,
        props: &StrokeProps,
    ) {
        let style = stroke_style(props, transform);
        match paint {
            hayro_interpret::Paint::Color(c) => {
                self.canvas
                    .stroke(path, transform, Paint::Solid(color(c)), &style);
            }
            hayro_interpret::Paint::Pattern(pattern) => {
                // A pattern paints the area the stroke covers.
                let outline = kurbo::stroke(
                    path.iter(),
                    &kurbo_stroke(&style),
                    &kurbo::StrokeOpts::default(),
                    STROKE_TOLERANCE / largest_scale(transform).max(1e-9),
                );
                self.patterned(&(transform * outline), FillRule::NonZero, pattern, true);
            }
        }
    }

    /// Paints `outline`, in the page's space, with a pattern.
    fn patterned(
        &mut self,
        outline: &BezPath,
        rule: FillRule,
        pattern: &Pattern<'_>,
        stroke: bool,
    ) {
        match pattern {
            Pattern::Shading(shading) => self.shade(outline, rule, shading),
            Pattern::Tiling(tiling) => self.tile(outline, rule, tiling, stroke),
        }
    }

    /// Paints `outline` with a shading: a gradient where one draws it
    /// exactly, else a picture sampled from it.
    fn shade(&mut self, outline: &BezPath, rule: FillRule, shading: &ShadingPattern) {
        let area = outline.bounding_box().intersect(self.page);
        if area.is_zero_area() {
            return;
        }
        let encoded = shading.encode();
        let clipped = shading.shading.clip_path.clone();
        if let Some(clip) = &clipped {
            self.canvas.push_clip(clip.clone(), FillRule::NonZero);
        }
        match gradient(&encoded, area) {
            Some(gradient) => self.canvas.fill(
                outline,
                Affine::IDENTITY,
                Paint::Gradient(Box::new(gradient)),
                rule,
            ),
            None => self.sampled(outline, rule, &encoded, area),
        }
        if clipped.is_some() {
            self.canvas.pop();
        }
    }

    /// Paints `outline` with a picture of the shading `encoded` sampled
    /// over `area`.
    fn sampled(
        &mut self,
        outline: &BezPath,
        rule: FillRule,
        encoded: &EncodedShadingPattern,
        area: Rect,
    ) {
        let density = TEXTURE_DENSITY.min(MAX_TEXTURE / area.width().max(area.height()));
        let mut pixels = Vec::new();
        let (width, height, placed) = encoded.sample_texture(area, density as f32, |sample| {
            pixels.extend(sample.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8));
        });
        let Ok(picture) = Image::new(width, height, pixels).and_then(|i| Picture::from_image(&i))
        else {
            return self
                .canvas
                .unsupported("a shading too large to sample (not drawn)");
        };
        self.canvas.push_clip(outline.clone(), rule);
        self.canvas.picture(picture, placed);
        self.canvas.pop();
    }

    /// Paints `outline` with a tiling pattern: its cell drawn as vectors at
    /// every step that reaches the outline, at most [`MAX_CELLS`] of them.
    fn tile<'a>(
        &mut self,
        outline: &BezPath,
        rule: FillRule,
        tiling: &TilingPattern<'a>,
        stroke: bool,
    ) {
        let area = outline.bounding_box().intersect(self.page);
        if area.is_zero_area() {
            return;
        }
        let cells = tiling.matrix.inverse().transform_rect_bbox(area);
        let (dx, dy) = (
            f64::from(tiling.x_step).abs(),
            f64::from(tiling.y_step).abs(),
        );
        let bbox = tiling.bbox;
        let steps = |from: f64, to: f64, low: f64, high: f64, step: f64| {
            ((from - high) / step).floor() as i64..=((to - low) / step).ceil() as i64
        };
        let columns = steps(cells.x0, cells.x1, bbox.x0, bbox.x1, dx);
        let rows = steps(cells.y0, cells.y1, bbox.y0, bbox.y1, dy);
        let count = columns.clone().count().saturating_mul(rows.clone().count());
        if count > MAX_CELLS {
            return self.canvas.unsupported(format!(
                "tiling patterns of more than {MAX_CELLS} cells (not drawn)"
            ));
        }
        self.canvas.push_clip(outline.clone(), rule);
        self.canvas.push_group(Group {
            opacity: tiling.opacity,
            ..Group::new()
        });
        for row in rows {
            for column in columns.clone() {
                let step = Affine::translate((column as f64 * dx, row as f64 * dy));
                // Each cell is the pattern's operators run again, at its step.
                tiling.interpret(self, tiling.matrix * step, stroke);
            }
        }
        self.canvas.pop();
        self.canvas.pop();
    }

    /// Paints an image, in coordinates `transform` maps to the page's.
    fn image(&mut self, image: PdfImage<'_, '_>, transform: Affine) {
        match image {
            PdfImage::Raster(raster) => raster.with_rgba(
                |data, alpha| {
                    let (sx, sy) = data.scale_factors();
                    let placed =
                        transform * Affine::scale_non_uniform(f64::from(sx), f64::from(sy));
                    match raster_picture(&data, alpha.as_ref()) {
                        Some(picture) => self.canvas.picture(picture, placed),
                        None => self
                            .canvas
                            .unsupported("an image that cannot be decoded (not drawn)"),
                    }
                },
                None,
            ),
            PdfImage::Stencil(stencil) => stencil.with_stencil(
                |mask, paint| {
                    let (sx, sy) = mask.scale_factors;
                    let placed =
                        transform * Affine::scale_non_uniform(f64::from(sx), f64::from(sy));
                    match paint {
                        hayro_interpret::Paint::Color(c) => {
                            match stencil_picture(&mask, color(c)) {
                                Some(picture) => self.canvas.picture(picture, placed),
                                None => self
                                    .canvas
                                    .unsupported("an image that cannot be decoded (not drawn)"),
                            }
                        }
                        hayro_interpret::Paint::Pattern(_) => self
                            .canvas
                            .unsupported("image masks painted with a pattern (not drawn)"),
                    }
                },
                None,
            ),
        }
    }
}

impl<'a> Device<'a> for Reader {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, draw_mode: &DrawMode) {
        let DrawProps {
            transform,
            paint,
            soft_mask,
            blend_mode,
        } = props;
        self.composited(soft_mask.as_ref(), blend_mode, |reader| match draw_mode {
            DrawMode::Fill(rule) => reader.fill(path, transform, &paint, fill_rule(*rule)),
            DrawMode::Stroke(stroke) => reader.stroke(path, transform, &paint, stroke),
            DrawMode::FillAndStroke(rule, stroke) => {
                reader.fill(path, transform, &paint, fill_rule(*rule));
                reader.stroke(path, transform, &paint, stroke);
            }
            DrawMode::Invisible => {}
        });
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        self.canvas
            .push_clip(clip_path.path.clone(), fill_rule(clip_path.fill));
    }

    fn push_transparency_group(
        &mut self,
        opacity: f32,
        mask: Option<SoftMask<'a>>,
        blend: hayro_interpret::BlendMode,
    ) {
        let mask = mask.map(|mask| self.matte(&mask));
        self.canvas.push_group(Group {
            opacity,
            blend: blend_mode(blend),
            mask,
        });
    }

    fn draw_glyph_run(
        &mut self,
        glyph_run: &hayro_interpret::font::GlyphRun<'_, 'a>,
        props: DrawProps<'a>,
        draw_mode: &DrawMode,
    ) {
        for glyph in glyph_run.glyphs() {
            let placed = glyph.transform();
            match &**glyph {
                Glyph::Outline(outline) => {
                    self.draw_path(&(placed * outline.outline()), props.clone(), draw_mode)
                }
                Glyph::Type3(type3) => {
                    if !matches!(draw_mode, DrawMode::Invisible) {
                        type3.interpret(self, props.transform, placed, &props.paint);
                    }
                }
            }
        }
    }

    fn draw_image(&mut self, image: PdfImage<'a, '_>, props: ImageDrawProps<'a>) {
        let ImageDrawProps {
            transform,
            soft_mask,
            blend_mode,
        } = props;
        self.composited(soft_mask.as_ref(), blend_mode, |reader| {
            reader.image(image, transform);
        });
    }

    fn pop_clip(&mut self) {
        self.canvas.pop();
    }

    fn pop_transparency_group(&mut self) {
        self.canvas.pop();
    }
}

/// The gradient that draws an axial or radial shading over `area`
/// exactly, in the page's space; `None` for other shadings, and for one
/// whose unextended ends or background a gradient cannot draw.
fn gradient(encoded: &EncodedShadingPattern, area: Rect) -> Option<Gradient> {
    let EncodedShadingType::RadialAxial(shading) = &encoded.shading_type else {
        return None;
    };
    let native = shading.as_svg_gradient(encoded, area, STOP_TOLERANCE)?;
    let shape = match native.kind {
        SvgGradientKind::Linear { start, end } => GradientShape::Linear { start, end },
        SvgGradientKind::Radial {
            start_center,
            start_radius,
            end_center,
            end_radius,
        } => GradientShape::Radial {
            start: start_center,
            start_radius,
            end: end_center,
            end_radius,
        },
    };
    let stops = native
        .stops
        .iter()
        .map(|stop| Stop {
            offset: stop.offset,
            color: rgba(stop.color),
        })
        .collect();
    Some(Gradient {
        shape,
        stops,
        transform: native.transform,
    })
}

/// A PDF colour, converted to sRGB, as the rasteriser's.
fn color(c: &hayro_interpret::color::Color) -> Color {
    rgba(c.to_rgba().components())
}

/// Straight sRGB channels from 0 to 1 as a colour.
fn rgba([r, g, b, a]: [f32; 4]) -> Color {
    Color::rgba(r, g, b, a)
}

/// The rasteriser's fill rule for a PDF one.
fn fill_rule(rule: hayro_interpret::FillRule) -> FillRule {
    match rule {
        hayro_interpret::FillRule::NonZero => FillRule::NonZero,
        hayro_interpret::FillRule::EvenOdd => FillRule::EvenOdd,
    }
}

/// The rasteriser's blend mode for a PDF one; every PDF mode has one.
fn blend_mode(mode: hayro_interpret::BlendMode) -> BlendMode {
    use hayro_interpret::BlendMode as Pdf;
    match mode {
        Pdf::Normal => BlendMode::Normal,
        Pdf::Multiply => BlendMode::Multiply,
        Pdf::Screen => BlendMode::Screen,
        Pdf::Overlay => BlendMode::Overlay,
        Pdf::Darken => BlendMode::Darken,
        Pdf::Lighten => BlendMode::Lighten,
        Pdf::ColorDodge => BlendMode::ColorDodge,
        Pdf::ColorBurn => BlendMode::ColorBurn,
        Pdf::HardLight => BlendMode::HardLight,
        Pdf::SoftLight => BlendMode::SoftLight,
        Pdf::Difference => BlendMode::Difference,
        Pdf::Exclusion => BlendMode::Exclusion,
        Pdf::Hue => BlendMode::Hue,
        Pdf::Saturation => BlendMode::Saturation,
        Pdf::Color => BlendMode::Color,
        Pdf::Luminosity => BlendMode::Luminosity,
    }
}

/// A PDF line style as the rasteriser's, in the coordinates the line is
/// given in, which `transform` maps to the page's. A width of 0 is the
/// thinnest line: one point on the page.
fn stroke_style(props: &StrokeProps, transform: Affine) -> StrokeStyle {
    let width = match f64::from(props.line_width) {
        width if width > 0.0 => width,
        _ => 1.0 / largest_scale(transform).max(1e-9),
    };
    let cap = match props.line_cap {
        kurbo::Cap::Butt => Cap::Butt,
        kurbo::Cap::Round => Cap::Round,
        kurbo::Cap::Square => Cap::Square,
    };
    let join = match props.line_join {
        kurbo::Join::Miter => Join::Miter,
        kurbo::Join::Round => Join::Round,
        kurbo::Join::Bevel => Join::Bevel,
    };
    let pattern: Vec<f64> = props
        .dash_array
        .iter()
        .map(|length| f64::from(length.abs()))
        .collect();
    // An empty pattern, or one of nothing but zeros, is a solid line.
    let dashes = (pattern.iter().sum::<f64>() > 0.0).then(|| Dashes {
        offset: f64::from(props.dash_offset),
        pattern,
    });
    StrokeStyle {
        width,
        cap,
        join,
        miter_limit: f64::from(props.miter_limit),
        dashes,
    }
}

/// `style` as kurbo's, to outline a stroke.
fn kurbo_stroke(style: &StrokeStyle) -> kurbo::Stroke {
    let cap = match style.cap {
        Cap::Butt => kurbo::Cap::Butt,
        Cap::Round => kurbo::Cap::Round,
        Cap::Square => kurbo::Cap::Square,
    };
    let join = match style.join {
        Join::Miter => kurbo::Join::Miter,
        Join::Round => kurbo::Join::Round,
        Join::Bevel => kurbo::Join::Bevel,
    };
    let stroke = kurbo::Stroke::new(style.width)
        .with_caps(cap)
        .with_join(join)
        .with_miter_limit(style.miter_limit);
    match &style.dashes {
        None => stroke,
        Some(dashes) => stroke.with_dashes(dashes.offset, dashes.pattern.iter().copied()),
    }
}

/// How much `transform` stretches the direction it stretches most.
fn largest_scale(transform: Affine) -> f64 {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    a.hypot(b).max(c.hypot(d))
}

/// A decoded raster image, with its alpha when it has one, as a picture.
fn raster_picture(data: &ImageData, alpha: Option<&LumaData>) -> Option<Picture> {
    let (width, height) = (data.width(), data.height());
    let count = width as usize * height as usize;
    let mut pixels = Vec::with_capacity(count * 4);
    match data {
        ImageData::Rgb(rgb) => rgb
            .data
            .chunks_exact(3)
            .take(count)
            .for_each(|p| pixels.extend([p[0], p[1], p[2], 255])),
        ImageData::Luma(luma) => luma
            .data
            .iter()
            .take(count)
            .for_each(|&v| pixels.extend([v, v, v, 255])),
    }
    if let Some(alpha) = alpha {
        let at = resampled(alpha, width, height);
        for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            pixel[3] = at(i % width as usize, i / width as usize);
        }
    }
    let image = Image::new(width, height, pixels).ok()?;
    Picture::from_image(&image).ok()
}

/// An image mask: `color` where the mask covers, clear elsewhere.
fn stencil_picture(mask: &LumaData, color: Color) -> Option<Picture> {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b] = [color.r, color.g, color.b].map(channel);
    let pixels = mask
        .data
        .iter()
        .flat_map(|&coverage| {
            [
                r,
                g,
                b,
                (f32::from(coverage) * color.a.clamp(0.0, 1.0)).round() as u8,
            ]
        })
        .collect();
    let image = Image::new(mask.width, mask.height, pixels).ok()?;
    Picture::from_image(&image).ok()
}

/// The value of `alpha` at each pixel of a `width` by `height` image, the
/// nearest of its own when its size differs.
fn resampled(alpha: &LumaData, width: u32, height: u32) -> impl Fn(usize, usize) -> u8 + '_ {
    let (aw, ah) = (alpha.width as usize, alpha.height as usize);
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    move |x, y| {
        let ax = (x * aw / w).min(aw.saturating_sub(1));
        let ay = (y * ah / h).min(ah.saturating_sub(1));
        alpha.data.get(ay * aw + ax).copied().unwrap_or(255)
    }
}

#[cfg(test)]
#[path = "../tests/unit/pdf.rs"]
mod tests;
