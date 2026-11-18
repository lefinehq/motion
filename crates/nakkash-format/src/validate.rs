//! The rules a scene follows beyond its JSON shape, one small function each,
//! turning the document into the typed [`Scene`]. A rule reports its error
//! without the enclosing path; the caller places it with [`Diagnostic::under`].

mod item;
mod text;

use std::collections::HashSet;
use std::str::FromStr;

use kurbo::Rect;

use crate::{Diagnostic, Kind, Result};

use crate::anim::{
    AnimKey, AnimTrack, EASING_EXPECTED, Easing, KEYS_EXPECTED, KeyValue, LevelLine, Listen,
    PATH_EXPECTED, RANGE_EXPECTED, Span, TrackLine, rest_word,
};
use crate::doc::{
    AnimDoc, CompDoc, CompsDoc, EasingDoc, Form, KeyDoc, KeyTime, LabelsDoc, LayerDoc, LayersDoc,
    List, Number, Object, OverrideDoc, OverrideEntry, OverridesDoc, PathDoc, Present, PresetDoc,
    PresetsDoc, SceneDoc, Shorthand, StartDoc, TrackDoc, ValueDoc,
};
use crate::edit_word::NONE;
use crate::item::{EFFECTS, MASKS, Reads, SHAPES};
use crate::parse::key::{
    ADJUST, ANIM, ANIMATE, AT, AUDIO, BG, BLEND, BOX, CAMERA, COLLAPSE, COMP, COMPS, CONTINUOUS,
    CROP, DUR, FILL, FIT, FONT, FPS, FX, ID, IMAGE, IN, LABELS, LAYERS, MASK, MATTE, MOTION,
    NAKKASH, OFFSET, OUT, OVERRIDES, PARAGRAPHS, PARENT, PIVOT, PRESETS, RECT, RUNS,
    SHAPES as SHAPES_KEY, SHUTTER, SIZE, SPAN, STRETCH, STYLE, TEXT, THREE_D, VIDEO, comp_path,
    layer_path,
};
use crate::parse::layer_keys;
use crate::preset::{PresetSpec, Start, expand_as_anim, lookup, names};
use crate::property::{LAYER, Prop, PropertySpec, Registry, Rest, Target};
use crate::shorthand::{Behaviour, Coord, Font, Position, Shutter};
use crate::slots::{Declared, Slot};
use crate::units::{Color, SPAN_EXPECTED, Size, Time, TimeSpan};
use crate::value::{Value, ValueType};
use crate::when::{Base, Moment, Moments, add, label_name};
use crate::{
    Address, AudioContent, BLEND_MODES, CameraContent, Comp, CompContent, Content, Curve,
    ImageContent, Layer, MediaFit, Scene, Step, VideoContent, fmt_num,
};

use crate::Place;
use crate::address::{ID_RULE, Id, SCENE};
use crate::overrides::written_id;

/// The only format version.
const VERSION: u32 = 1;
const DEFAULT_FPS: f64 = 30.0;
const DEFAULT_SIZE: Size = Size {
    width: 1920,
    height: 1080,
};
const DEFAULT_BACKGROUND: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};
const CLEAR: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};
/// White, the fill when a layer sets none.
const DEFAULT_FILL: [f64; 4] = [1.0; 4];
const DEFAULT_POSITION: Position = Position {
    x: Coord::Center,
    y: Coord::Center,
};

/// What every composition reads from the scene: its presets, the
/// compositions a `comp` layer may draw, and the JSON overrides are set over.
struct Shared<'a> {
    presets: &'a [(String, PresetSpec)],
    /// Every composition's canvas, read before any layer.
    comps: &'a [Canvas],
    /// The scene as written, whose compositions' layers an instance
    /// resolves with its own slot values before its overrides are set over.
    written: &'a serde_json::Value,
    /// The slots of the scene and of each composition.
    declared: &'a Declared,
}

/// A composition's id, size and frames per second, which a layer drawing it
/// may need before the composition is read.
struct Canvas {
    id: String,
    size: Size,
    fps: f64,
    /// Its length, so a layer before another knows where it ends.
    duration: Time,
    /// Its labels, resolved.
    labels: Vec<(String, Time)>,
}

/// A composition's frames per second, length and background.
struct Clock {
    fps: f64,
    duration: Time,
    background: Color,
    shutter: Shutter,
}

/// What a layer needs from the scene around it.
struct Context<'a> {
    shared: &'a Shared<'a>,
    /// The frames per second of the layer's composition.
    fps: f64,
    /// The canvas of the layer's composition.
    size: Size,
    /// The length of the layer's composition.
    duration: Time,
    /// The labels of the layer's composition, resolved.
    labels: &'a [(String, Time)],
}

/// Validates a scene document into a [`Scene`], with the defaults of the keys left out.
pub(crate) fn scene(
    doc: &SceneDoc,
    written: &serde_json::Value,
    declared: &Declared,
) -> Result<Scene> {
    // The key as written: the old name when the scene still uses it (D73).
    let version_key = if written.get(MOTION).is_some() {
        MOTION
    } else {
        NAKKASH
    };
    version(
        required(*doc.nakkash, NAKKASH, &VERSION.to_string())?,
        version_key,
    )?;
    let fps = doc.fps.map_or(Ok(DEFAULT_FPS), fps)?;
    let size = optional(doc.size.as_ref(), SIZE)?.unwrap_or(DEFAULT_SIZE);
    let clock = clock(fps, &doc.dur, &doc.bg, &doc.shutter, DEFAULT_BACKGROUND)?;
    let Object(own_presets) = &doc.presets;
    let presets = presets(own_presets).map_err(|e| e.under(PRESETS))?;
    let Object(comp_docs) = &doc.comps;
    let canvases = canvases(comp_docs, fps)?;
    let fonts = match &*doc.fonts {
        Some(List(fonts)) => fonts.clone(),
        None => Vec::new(),
    };
    let List(layer_docs) = required(doc.layers.as_ref(), LAYERS, "[]")?;
    let shared = Shared {
        presets: &presets,
        comps: &canvases,
        written,
        declared,
    };
    let comps = comp_docs
        .iter()
        .zip(&canvases)
        .map(|((id, Object(comp_doc)), canvas)| {
            listed_comp(canvas, comp_doc, &shared, declared.of_comp(id))
                .map_err(|e| e.under(comp_path(id)))
        })
        .collect::<Result<Vec<_>>>()?;
    let Object(own_labels) = &doc.labels;
    let root_labels = labels(own_labels, fps)?;
    let root = comp(
        SCENE,
        size,
        clock,
        root_labels,
        layer_docs,
        &shared,
        &declared.scene,
    )?;
    no_comp_draws_itself(&comps)?;
    let mut scene = Scene {
        version: VERSION,
        root,
        presets,
        fonts,
        comps,
        roles: Vec::new(),
        locks: Vec::new(),
    };
    scene.roles = crate::roles::read(doc.roles.as_ref(), &scene)?;
    scene.locks = crate::locks::read(doc.locks.as_ref(), &scene)?;
    Ok(scene)
}

/// A list of layers on one canvas, with paths relative to the list's owner:
/// each layer valid, ids unique, parents and mattes that exist and never
/// loop, and no
/// track fighting an entrance.
fn layers(docs: &LayersDoc, context: &Context) -> Result<Vec<Layer>> {
    // In order, so each layer can follow the one before it.
    let mut layers: Vec<Layer> = Vec::with_capacity(docs.len());
    for (i, Object(layer_doc)) in docs.iter().enumerate() {
        let id = layer_doc.id.as_deref().unwrap_or_default();
        let before = layers.last().map(|layer| extent(layer, context));
        layers.push(layer(layer_doc, context, before).map_err(|e| e.under(Place::layer(id, i)))?);
    }
    unique_ids(&layers)?;
    links(
        &layers,
        PARENT,
        |layer| layer.parent.as_deref(),
        "would move with itself through its parents",
        "parents that never lead back to the layer",
    )?;
    links(
        &layers,
        MATTE,
        |layer| layer.matte.as_ref().map(|m| m.layer.as_str()),
        "would be its own matte through its mattes",
        "mattes that never lead back to the layer",
    )?;
    inputs(&layers)?;
    outlines(&layers)?;
    sounds(&layers)?;
    listens(&layers)?;
    for (layer, Object(doc)) in layers.iter().zip(docs) {
        let Object(written) = &doc.anim;
        entrance_conflict(layer, &layers, context, written)
            .and_then(|()| exit_conflict(layer, context))
            .map_err(|e| e.under(Place::of_layer(&layer.id)))?;
    }
    Ok(layers)
}

/// Every composition's canvas: its id, which follows the id rule, its size,
/// which it must set, and its frames per second, the scene's unless set.
fn canvases(docs: &CompsDoc, scene_fps: f64) -> Result<Vec<Canvas>> {
    docs.iter()
        .map(|(id, Object(doc))| {
            let place = |e: Diagnostic| e.under(comp_path(id));
            if !Id::is_valid(id) {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("invalid composition id `{id}`"),
                )
                .at(comp_path(id))
                .expected(ID_RULE));
            }
            let size = required(doc.size.as_ref(), SIZE, r#""1080x1920""#)
                .and_then(|s| s.parse_at(SIZE))
                .map_err(place)?;
            let fps = doc.fps.map_or(Ok(scene_fps), fps).map_err(place)?;
            let duration = required(doc.dur.as_ref(), DUR, r#""5s""#)
                .and_then(|d| d.parse_at(DUR))
                .map_err(place)?;
            let Object(written) = &doc.labels;
            let labels = labels(written, fps).map_err(place)?;
            Ok(Canvas {
                id: id.clone(),
                size,
                fps,
                duration,
                labels,
            })
        })
        .collect()
}

/// One composition under `comps`, with paths relative to it: its background
/// defaults to clear.
fn listed_comp(canvas: &Canvas, doc: &CompDoc, shared: &Shared, slots: &[Slot]) -> Result<Comp> {
    let clock = clock(canvas.fps, &doc.dur, &doc.bg, &doc.shutter, CLEAR)?;
    let List(layer_docs) = required(doc.layers.as_ref(), LAYERS, "[]")?;
    comp(
        &canvas.id,
        canvas.size,
        clock,
        canvas.labels.clone(),
        layer_docs,
        shared,
        slots,
    )
}

/// A composition's labels, each resolved against the others it names, in
/// the order written. Paths are relative to the composition.
fn labels(written: &LabelsDoc, fps: f64) -> Result<Vec<(String, Time)>> {
    let at = |name: &str, e: Diagnostic| e.at(name).under(LABELS);
    let moments = written
        .iter()
        .map(|(name, moment)| {
            label_name(name).map_err(|e| at(name, e))?;
            moment.parse_at(name).map_err(|e| e.under(LABELS))
        })
        .collect::<Result<Vec<Moment>>>()?;
    let names: Vec<&str> = written.keys().map(String::as_str).collect();
    let mut resolved = vec![None; names.len()];
    for i in 0..names.len() {
        resolve_label(i, &names, &moments, &mut resolved, &mut Vec::new(), fps)
            .map_err(|e| at(names[i], e))?;
    }
    Ok(names
        .iter()
        .zip(resolved)
        .map(|(name, time)| ((*name).to_owned(), time.expect("every label resolved")))
        .collect())
}

/// Label `i`, resolving the labels it names first; `path` holds the labels
/// being resolved, to find a circle.
fn resolve_label(
    i: usize,
    names: &[&str],
    moments: &[Moment],
    resolved: &mut [Option<Time>],
    path: &mut Vec<usize>,
    fps: f64,
) -> Result<Time> {
    if let Some(time) = resolved[i] {
        return Ok(time);
    }
    if let Some(from) = path.iter().position(|&j| j == i) {
        let circle: Vec<&str> = path[from..].iter().chain([&i]).map(|&j| names[j]).collect();
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("labels in a circle: {}", circle.join(" → ")),
        )
        .expected("labels that each lead back to a time"));
    }
    path.push(i);
    let time = moments[i].resolve(
        |base| match base {
            Base::Label(name) => match names.iter().position(|n| n == name) {
                Some(j) => resolve_label(j, names, moments, resolved, path, fps),
                None => Err(Diagnostic::unknown("label", name, names)),
            },
            Base::Before | Base::BeforeStart => Err(Diagnostic::new(
                Kind::Invalid,
                format!("a label cannot follow a layer, as `{base}` does"),
            )
            .expected("a time, or another label moved by a time")),
            Base::Zero => unreachable!("a zero base resolves without a lookup"),
        },
        fps,
    )?;
    path.pop();
    resolved[i] = Some(time);
    Ok(time)
}

/// A composition's clock and background, from the keys the scene's root
/// and every composition under `comps` share.
fn clock(
    fps: f64,
    dur: &Present<Shorthand<Time>>,
    bg: &Present<Shorthand<Color>>,
    shutter: &Present<Shorthand<Shutter>>,
    default_background: Color,
) -> Result<Clock> {
    Ok(Clock {
        fps,
        duration: required(dur.as_ref(), DUR, r#""5s""#)?.parse_at(DUR)?,
        background: optional(bg.as_ref(), BG)?.unwrap_or(default_background),
        shutter: optional(shutter.as_ref(), SHUTTER)?.unwrap_or_default(),
    })
}

/// One composition, the scene's root or one under `comps`, from its size,
/// its clock and its layers: the one place either is built.
fn comp(
    id: &str,
    size: Size,
    clock: Clock,
    labels: Vec<(String, Time)>,
    layer_docs: &LayersDoc,
    shared: &Shared,
    slots: &[Slot],
) -> Result<Comp> {
    let context = Context {
        shared,
        fps: clock.fps,
        size,
        duration: clock.duration,
        labels: &labels,
    };
    let layers = layers(layer_docs, &context)?;
    Ok(Comp {
        id: id.to_owned(),
        size,
        fps: clock.fps,
        duration: clock.duration,
        background: clock.background,
        shutter: clock.shutter,
        layers,
        labels,
        slots: slots.to_vec(),
    })
}

/// No composition draws itself, directly or through the compositions it draws.
fn no_comp_draws_itself(comps: &[Comp]) -> Result<()> {
    /// The compositions `comp`'s layers draw, with each layer's index.
    fn drawn(comp: &Comp) -> impl Iterator<Item = (usize, &str)> {
        comp.layers
            .iter()
            .enumerate()
            .filter_map(|(i, layer)| match &layer.content {
                Content::Comp(comp) => Some((i, comp.id.as_str())),
                _ => None,
            })
    }
    for start in comps {
        // Depth-first over what `start` draws, remembering the first layer of each step.
        let mut stack: Vec<(&str, usize)> = drawn(start).map(|(i, id)| (id, i)).collect();
        let mut seen = HashSet::new();
        while let Some((id, first)) = stack.pop() {
            if id == start.id {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("composition `{id}` would draw itself through its layers"),
                )
                .at(COMP)
                .under(Place::of_layer(&start.layers[first].id))
                .under(comp_path(&start.id))
                .expected("compositions that never lead back to themselves"));
            }
            if !seen.insert(id) {
                continue;
            }
            if let Some(next) = comps.iter().find(|c| c.id == id) {
                stack.extend(drawn(next).map(|(_, inner)| (inner, first)));
            }
        }
    }
    Ok(())
}

/// Validates a `presets` object; the built-ins in `presets.json` use it too.
pub(crate) fn presets(doc: &PresetsDoc) -> Result<Vec<(String, PresetSpec)>> {
    doc.iter()
        .map(|(name, Object(body))| {
            preset(name, body)
                .map(|spec| (name.clone(), spec))
                .map_err(|e| e.under(name))
        })
        .collect()
}

/// A required key; the expected form shows it written with `value` as an example.
fn required<T>(written: Option<T>, key: &str, value: &str) -> Result<T> {
    written.ok_or_else(|| {
        Diagnostic::new(Kind::Missing, format!("missing `{key}`"))
            .expected(format!(r#"e.g. "{key}":{value}"#))
    })
}

fn version(version: Number, key: &str) -> Result<()> {
    if version.0 == f64::from(VERSION) {
        return Ok(());
    }
    Err(
        Diagnostic::new(Kind::Invalid, format!("unsupported version {}", version.0))
            .at(key)
            .expected(format!("`{VERSION}`")),
    )
}

fn fps(fps: Number) -> Result<f64> {
    if fps.0 > 0.0 {
        return Ok(fps.0);
    }
    Err(
        Diagnostic::new(Kind::OutOfRange, "must be a number above zero")
            .at(FPS)
            .expected("e.g. `30`"),
    )
}

/// Parses the shorthand written at `key`, if the key is there.
fn optional<T: FromStr<Err = Diagnostic>>(
    written: Option<&Shorthand<T>>,
    key: &str,
) -> Result<Option<T>> {
    written.map(|text| text.parse_at(key)).transpose()
}

/// Validates one layer, with paths relative to it.
/// Where a layer starts and ends on its composition's timeline, for the
/// layer after it: its span, or from its offset to where its composition
/// ends at its stretch for a `comp` layer, or to the end of the timeline.
fn extent(layer: &Layer, context: &Context) -> (Time, Time) {
    if let Some(span) = layer.span {
        return (span.start, span.end);
    }
    let end = match &layer.content {
        Content::Comp(comp) => {
            let canvas = context
                .shared
                .comps
                .iter()
                .find(|c| c.id == comp.id)
                .expect("a composition the layer draws exists");
            let seconds = canvas.duration.to_frames(canvas.fps) / canvas.fps * layer.stretch.abs();
            add(layer.offset, Time::Seconds(seconds), context.fps)
        }
        _ => context.duration,
    };
    (layer.offset, end)
}

/// A layer's `offset` or an end of its `span`, resolved against the
/// composition's labels and `before`, the extent of the layer before it.
fn moment(moment: &Moment, context: &Context, before: Option<(Time, Time)>) -> Result<Time> {
    moment.resolve(
        |base| match base {
            Base::Label(name) => context
                .labels
                .iter()
                .find(|(label, _)| label == name)
                .map(|&(_, time)| time)
                .ok_or_else(|| match context.labels.is_empty() {
                    true => Diagnostic::new(
                        Kind::UnknownName,
                        format!("unknown label `{name}`: the composition has no labels"),
                    )
                    .expected(
                        r#"a label in the composition's "labels", e.g. "labels":{"intro":"0.5s"}"#,
                    ),
                    false => {
                        Diagnostic::unknown("label", name, context.labels.iter().map(|(l, _)| l))
                    }
                }),
            Base::Before | Base::BeforeStart => before
                .map(|(start, end)| match base {
                    Base::Before => end,
                    _ => start,
                })
                .ok_or_else(|| {
                    Diagnostic::new(
                        Kind::Invalid,
                        format!("`{base}` follows the layer before, and this layer is the first"),
                    )
                    .expected("a time or a label on the first layer")
                }),
            Base::Zero => unreachable!("a zero base resolves without a lookup"),
        },
        context.fps,
    )
}

fn layer(doc: &LayerDoc, context: &Context, before: Option<(Time, Time)>) -> Result<Layer> {
    // Unknown keys first, as serde reports them for documents that deny them.
    let values = resting_values(doc)?;
    let id = required(doc.id.as_deref(), ID, r#""title""#)?;
    layer_id(id)?;
    let Object(tracks) = &doc.anim;
    let layer = Layer {
        id: id.to_owned(),
        content: content(doc, context)?,
        fill: match &*doc.fill {
            Some(written) => crate::text::color(written).map_err(|e| e.under(FILL))?,
            None => DEFAULT_FILL,
        },
        at: optional(doc.at.as_ref(), AT)?.unwrap_or(DEFAULT_POSITION),
        values,
        enter: doc
            .enter
            .as_ref()
            .map(|written| behaviour(written, context.shared.presets, IN))
            .transpose()?,
        exit: doc
            .exit
            .as_ref()
            .map(|written| behaviour(written, context.shared.presets, OUT))
            .transpose()?,
        anim: anim(tracks, &LAYER, context.fps).map_err(|e| e.under(ANIM))?,
        parent: doc.parent.clone(),
        span: optional(doc.span.as_ref(), SPAN)?
            .map(|Moments { start, end }| {
                Ok::<_, Diagnostic>(TimeSpan {
                    start: moment(&start, context, before)?,
                    end: moment(&end, context, before)?,
                })
            })
            .transpose()
            .map_err(|e| e.at(SPAN))?,
        offset: optional(doc.offset.as_ref(), OFFSET)?
            .map(|written| moment(&written, context, before).map_err(|e| e.at(OFFSET)))
            .transpose()?
            .unwrap_or(Time::Frames(0.0)),
        stretch: doc.stretch.map_or(Ok(1.0), stretch)?,
        mask: match &*doc.mask {
            Some(List(docs)) => item::items(docs, &MASKS, context.fps)
                .and_then(|mask| item::unique_ids(&mask, "").map(|()| mask))
                .map_err(|e| e.under(MASK))?,
            None => Vec::new(),
        },
        fx: match &*doc.fx {
            Some(List(docs)) => item::items(docs, &EFFECTS, context.fps)
                .and_then(|fx| item::unique_ids(&fx, "").map(|()| fx))
                .map_err(|e| e.under(FX))?,
            None => Vec::new(),
        },
        adjust: false,
        collapse: doc.collapse.unwrap_or(false),
        hidden: doc.hidden.unwrap_or(false),
        matte: optional(doc.matte.as_ref(), MATTE)?,
        blend: match doc.blend.as_deref() {
            Some(name) => BLEND_MODES
                .into_iter()
                .find(|mode| *mode == name)
                .ok_or_else(|| Diagnostic::unknown("blend mode", name, BLEND_MODES).at(BLEND))?,
            None => BLEND_MODES[0],
        },
        mblur: doc.mblur.unwrap_or(false),
        three_d: doc.three_d.unwrap_or(false),
    };
    let layer = Layer {
        adjust: adjusts(doc, &layer)?,
        three_d: layer.three_d || matches!(layer.content, Content::Camera(_)),
        ..layer
    };
    depth(&layer, doc)?;
    if let Some(span) = layer.span {
        span_runs_forward(span, context.fps).map_err(|e| e.under(SPAN))?;
    }
    if let Some(behaviour) = &layer.enter {
        parts_entrance(&layer, behaviour, context.shared.presets).map_err(|e| e.at(IN))?;
    }
    if let Some(behaviour) = &layer.exit {
        exit(&layer, behaviour).map_err(|e| e.at(OUT))?;
    }
    for (behaviour, key) in [(&layer.enter, IN), (&layer.exit, OUT)] {
        if let Some(behaviour) = behaviour {
            flat_preset(&layer, behaviour, context.shared.presets).map_err(|e| e.at(key))?;
        }
    }
    Ok(layer)
}

/// The depth rows are for 3D layers and cameras, and a camera's own rows
/// for cameras: a flat layer that sets or animates them would have them
/// ignored, so that is an error.
fn depth(layer: &Layer, doc: &LayerDoc) -> Result<()> {
    let camera = matches!(layer.content, Content::Camera(_));
    let used = |prop: Prop| {
        layer.value(prop).is_some() || layer.anim.iter().any(|track| track.target.prop == prop)
    };
    // Where the row is written: as a resting value, or under `anim`.
    let at = |d: Diagnostic, prop: Prop| match layer.value(prop) {
        Some(_) => d.at(prop.name()),
        None => match layer.anim.iter().find(|track| track.target.prop == prop) {
            Some(track) => d.at(track.target.name()).under(ANIM),
            None => d.at(prop.name()),
        },
    };
    if !layer.three_d
        && let Some(prop) = Prop::DEPTH.into_iter().find(|&p| used(p))
    {
        let e = Diagnostic::new(
            Kind::Invalid,
            format!(
                "`{}` places a layer in depth, and `{}` is flat",
                prop.name(),
                layer.id
            ),
        );
        return Err(at(e, prop).expected(r#""3d":true on the layer"#));
    }
    if !camera && let Some(prop) = Prop::CAMERA.into_iter().find(|&p| used(p)) {
        let e = Diagnostic::new(
            Kind::Invalid,
            format!(
                "`{}` is a camera's, and `{}` is not a camera",
                prop.name(),
                layer.id
            ),
        );
        return Err(at(e, prop).expected(r#""camera":true on the layer"#));
    }
    if camera && *doc.three_d == Some(false) {
        return Err(Diagnostic::new(Kind::Conflict, "a camera is always 3D")
            .at(THREE_D)
            .expected("`3d` left out on a camera"));
    }
    Ok(())
}

/// Whether `layer` applies its effects to what is below it: a filter layer
/// always does, any other when `adjust` says so, and then it needs effects
/// to apply.
fn adjusts(doc: &LayerDoc, layer: &Layer) -> Result<bool> {
    let adjust = matches!(layer.content, Content::Filter) || doc.adjust.unwrap_or(false);
    if adjust && layer.collapse {
        return Err(Diagnostic::new(
            Kind::Conflict,
            "`adjust` keeps the layer from drawing, so it has nothing to `collapse`",
        )
        .at(COLLAPSE)
        .expected("`adjust` or `collapse`, not both"));
    }
    if adjust && layer.fx.is_empty() {
        return Err(Diagnostic::new(
            Kind::Missing,
            "`adjust` applies the layer's `fx` to what is below it, and it has none",
        )
        .at(ADJUST)
        .expected(r##""fx":[{"fill":"#FFFFFF"}] next to "adjust":true"##));
    }
    Ok(adjust)
}

/// The layer properties an entrance can move text parts by: those a text
/// animator has under the same name and meaning.
pub(crate) const PART_PROPS: [&str; 4] = ["position", "scale", "rotate", "opacity"];

/// Checks an entrance that moves the parts of a text: the layer must be
/// text, and its preset may move only what a text animator can.
fn parts_entrance(
    layer: &Layer,
    behaviour: &Behaviour,
    presets: &[(String, PresetSpec)],
) -> Result<()> {
    let Some(parts) = behaviour.parts else {
        return Ok(());
    };
    if !matches!(layer.content, Content::Text(_)) {
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!("`{parts}` moves the parts of a text, and this is not a text layer"),
        )
        .expected("parts only on a text layer; elsewhere the whole layer enters"));
    }
    let spec = lookup(presets, &behaviour.preset).expect("the preset was checked");
    match spec
        .props
        .iter()
        .find(|(target, _)| !PART_PROPS.contains(&target.prop.name()))
    {
        Some((target, _)) => Err(Diagnostic::new(
            Kind::Conflict,
            format!(
                "preset `{}` moves `{}`, which the parts of a text cannot move",
                behaviour.preset,
                target.name()
            ),
        )
        .expected(format!(
            "a preset that moves only {}",
            PART_PROPS.map(|p| format!("`{p}`")).join(", ")
        ))),
        None => Ok(()),
    }
}

/// The layer keys that are not fixed keys: each a property, set to a value of its type.
/// A property that rests on `at` may be set by name instead, as an exact point
/// in the parent's own coordinates, but not with `at` as well.
fn resting_values(doc: &LayerDoc) -> Result<Vec<(Prop, Value)>> {
    doc.values
        .iter()
        .map(|(name, written)| {
            let prop = Prop::named(name).ok_or_else(|| {
                Diagnostic::unknown("layer key", name, layer_keys()).at(name.as_str())
            })?;
            if prop.spec().rest == Rest::At && doc.at.is_some() {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("`{name}` and `{AT}` both place the layer"),
                )
                .at(name.as_str())
                .expected(format!(
                    r#"one of them: "{AT}":"540 960" places it in the parent's box, "{name}":[540,960] at an exact point in the parent's own coordinates"#
                )));
            }
            let value = crate::de::from_value::<ValueDoc>(written)
                .and_then(|doc| typed_value(&doc, Target::whole(prop)))
                .map_err(|e| e.under(name))?;
            Ok((prop, value))
        })
        .collect()
}

/// A stretch is a finite number other than zero.
fn stretch(stretch: Number) -> Result<f64> {
    if stretch.0.is_finite() && stretch.0 != 0.0 {
        return Ok(stretch.0);
    }
    Err(
        Diagnostic::new(Kind::OutOfRange, "must be a number other than zero")
            .at(STRETCH)
            .expected("e.g. `2` for half speed, `-1` for backwards"),
    )
}

/// What sound allows: an audio layer draws nothing, so it holds no effects,
/// is not matted and mattes nothing; and only a layer that plays sound, an
/// audio or `comp` layer, has a `volume`.
fn sounds(layers: &[Layer]) -> Result<()> {
    let plays = |layer: &Layer| matches!(layer.content, Content::Audio(_));
    for layer in layers {
        let place = |d: Diagnostic| d.under(Place::of_layer(&layer.id));
        let nothing = |key: &str, what: &str| {
            place(
                Diagnostic::new(
                    Kind::Conflict,
                    format!("an audio layer draws nothing, so it has nothing {what}"),
                )
                .at(key)
                .expected(format!("`{key}` on a layer that draws")),
            )
        };
        if plays(layer) && !layer.fx.is_empty() {
            return Err(nothing(FX, "for effects to change"));
        }
        if plays(layer) && layer.matte.is_some() {
            return Err(nothing(MATTE, "for a matte to cut"));
        }
        if let Some(matte) = &layer.matte
            && layers.iter().any(|l| l.id == matte.layer && plays(l))
        {
            return Err(place(
                Diagnostic::new(
                    Kind::Conflict,
                    format!(
                        "`{}` is an audio layer, which draws nothing to matte with",
                        matte.layer
                    ),
                )
                .at(MATTE)
                .expected("a layer that draws"),
            ));
        }
        let volume = layer.value(Prop::VOLUME).is_some()
            || layer
                .anim
                .iter()
                .any(|track| track.target.prop == Prop::VOLUME);
        let remap = layer.value(Prop::REMAP).is_some()
            || layer
                .anim
                .iter()
                .any(|track| track.target.prop == Prop::REMAP);
        if remap && !matches!(layer.content, Content::Comp(_) | Content::Video(_)) {
            return Err(place(
                Diagnostic::new(
                    Kind::Conflict,
                    "`remap` only applies to `comp` and video layers, which play content in time",
                )
                .at(Prop::REMAP.name())
                .expected("`remap` on a `comp` or video layer"),
            ));
        }
        if volume && !matches!(layer.content, Content::Audio(_) | Content::Comp(_)) {
            return Err(place(
                Diagnostic::new(
                    Kind::Conflict,
                    "`volume` only applies to `audio` or `comp` layers, which play sound",
                )
                .at(Prop::VOLUME.name())
                .expected("`volume` on an audio layer, or on a `comp` layer for its whole sound"),
            ));
        }
    }
    Ok(())
}

/// Every link at `key` (a parent, a matte) names another layer of the list,
/// and following links never comes back; `looping` ends the message for a
/// layer that would reach itself, `expected` says what is allowed.
fn links(
    layers: &[Layer],
    key: &str,
    link: impl Fn(&Layer) -> Option<&str>,
    looping: &str,
    expected: &'static str,
) -> Result<()> {
    let index = |id: &str| layers.iter().position(|layer| layer.id == id);
    for (i, layer) in layers.iter().enumerate() {
        let Some(target) = link(layer) else {
            continue;
        };
        let place = |d: Diagnostic| d.at(key).under(Place::of_layer(&layer.id));
        let mut next = index(target).ok_or_else(|| {
            place(Diagnostic::unknown(
                "layer id",
                target,
                layers.iter().map(|l| l.id.as_str()),
            ))
        })?;
        // A chain without a cycle visits each layer at most once.
        for _ in 0..layers.len() {
            if next == i {
                return Err(place(
                    Diagnostic::new(Kind::Invalid, format!("`{}` {looping}", layer.id))
                        .expected(expected),
                ));
            }
            match link(&layers[next]) {
                Some(up) => next = index(up).unwrap_or(next),
                None => break,
            }
        }
    }
    Ok(())
}

/// Every layer an effect reads (its input) is in the same list and is
/// not the layer itself, and no layer reaches itself through the layers
/// its effects read and its matte, which are drawn to draw it.
/// Every track that follows a sound's level hears an audio or `comp` layer
/// of the same list: the layer's own tracks and its items', however deep.
fn listens(layers: &[Layer]) -> Result<()> {
    for layer in layers {
        let mut tracks: Vec<(String, &AnimTrack)> = layer
            .anim
            .iter()
            .map(|track| (format!("{ANIM}.{}", track.target.name()), track))
            .collect();
        let mut lists: Vec<(String, &[crate::Item])> =
            vec![("fx".into(), &layer.fx), ("mask".into(), &layer.mask)];
        match &layer.content {
            Content::Shapes(items) => lists.push(("shapes".into(), items)),
            Content::Text(text) => lists.push(("animate".into(), &text.animators)),
            _ => {}
        }
        while let Some((at, items)) = lists.pop() {
            for (i, item) in items.iter().enumerate() {
                let here = format!("{at}[{i}]");
                tracks.extend(
                    item.anim
                        .iter()
                        .map(|track| (format!("{here}.{ANIM}.{}", track.target.name()), track)),
                );
                lists.push((format!("{here}.items"), &item.items));
            }
        }
        for (at, track) in tracks {
            let Some(listen) = &track.listen else {
                continue;
            };
            let place = |d: Diagnostic| d.under(at.clone()).under(Place::of_layer(&layer.id));
            let Some(heard) = layers.iter().find(|l| l.id == listen.layer) else {
                return Err(place(Diagnostic::unknown(
                    "layer id",
                    &listen.layer,
                    layers.iter().map(|l| l.id.as_str()),
                )));
            };
            if !matches!(heard.content, Content::Audio(_) | Content::Comp(_)) {
                return Err(place(
                    Diagnostic::new(
                        Kind::Conflict,
                        format!("`{}` plays no sound to follow", listen.layer),
                    )
                    .expected("an `audio` layer, or a `comp` layer for its composition's sound"),
                ));
            }
        }
    }
    Ok(())
}

/// Every mask an effect follows (its outline) is one of its own layer's.
fn outlines(layers: &[Layer]) -> Result<()> {
    for layer in layers {
        for (i, item) in layer.fx.iter().enumerate() {
            let (Some(id), Some(key)) = (item.outline.as_deref(), item.kind.outline) else {
                continue;
            };
            let ids: Vec<&str> = layer.mask.iter().filter_map(|m| m.id.as_deref()).collect();
            if !ids.contains(&id) {
                let unknown = Diagnostic::unknown("mask id", id, &ids);
                let unknown = match ids.is_empty() {
                    true => unknown.expected("the id of one of the layer's own masks"),
                    false => unknown,
                };
                return Err(unknown
                    .at(key)
                    .under(format!("fx[{i}]"))
                    .under(Place::of_layer(&layer.id)));
            }
        }
    }
    Ok(())
}

fn inputs(layers: &[Layer]) -> Result<()> {
    let index = |id: &str| layers.iter().position(|layer| layer.id == id);
    let mut reads: Vec<Vec<usize>> = Vec::with_capacity(layers.len());
    for layer in layers {
        let mut needs = Vec::new();
        for (i, item) in layer.fx.iter().enumerate() {
            let Some(target) = item.input.as_deref() else {
                continue;
            };
            let Some(input) = item.kind.input else {
                continue;
            };
            let key = input.key;
            let place = |d: Diagnostic| {
                d.at(key)
                    .under(format!("fx[{i}]"))
                    .under(Place::of_layer(&layer.id))
            };
            let at = index(target).ok_or_else(|| {
                place(Diagnostic::unknown(
                    "layer id",
                    target,
                    layers.iter().map(|l| l.id.as_str()),
                ))
            })?;
            let plays = matches!(layers[at].content, Content::Audio(_) | Content::Comp(_));
            match input.reads {
                Reads::Drawing | Reads::Laid if matches!(layers[at].content, Content::Audio(_)) => {
                    return Err(place(
                        Diagnostic::new(
                            Kind::Conflict,
                            format!("`{target}` is an audio layer, which draws nothing to read"),
                        )
                        .expected("a layer that draws"),
                    ));
                }
                Reads::Drawing | Reads::Laid => needs.push(at),
                // Sound is mixed, not drawn, so reading it needs no drawing.
                Reads::Sound if plays => {}
                Reads::Sound => {
                    return Err(place(
                        Diagnostic::new(Kind::Conflict, format!("`{target}` plays no sound"))
                            .expected(
                                "an `audio` layer, or a `comp` layer for its composition's sound",
                            ),
                    ));
                }
            }
        }
        if let Some(matte) = layer.matte.as_ref().and_then(|m| index(&m.layer)) {
            needs.push(matte);
        }
        reads.push(needs);
    }
    // A layer that reaches itself would need its own drawing to be drawn.
    for start in 0..layers.len() {
        let mut stack = reads[start].clone();
        let mut seen = vec![false; layers.len()];
        while let Some(next) = stack.pop() {
            if next == start {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!(
                        "`{}` would read its own drawing through the layers its effects read",
                        layers[start].id
                    ),
                )
                .under(Place::of_layer(&layers[start].id))
                .expected("effects that read other layers, never leading back"));
            }
            if !std::mem::replace(&mut seen[next], true) {
                stack.extend(&reads[next]);
            }
        }
    }
    Ok(())
}

/// An id follows the id rule.
fn follows_id_rule(id: &str) -> Result<()> {
    id.parse::<Id>().map(drop).map_err(|e| e.at(ID))
}

/// A layer's id follows the id rule, and is not `scene`, which names the
/// scene itself in an address (D43).
fn layer_id(id: &str) -> Result<()> {
    follows_id_rule(id)?;
    if id == SCENE {
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!("`{SCENE}` names the scene itself, so no layer may take it"),
        )
        .at(ID)
        .expected("another id, such as `scene-2`"));
    }
    Ok(())
}

/// The keys that say what a layer draws, of which it has exactly one.
const CONTENT_KEYS: [&str; 9] = [
    TEXT, RECT, SHAPES_KEY, COMP, IMAGE, VIDEO, AUDIO, PIVOT, CAMERA,
];

/// A layer draws exactly one of [`CONTENT_KEYS`]. None is `Missing` and two
/// is a `Conflict`, at the second, with the same message since the fix is the same.
fn content(doc: &LayerDoc, context: &Context) -> Result<Content> {
    let written = [
        doc.text.is_some(),
        doc.rect.is_some(),
        doc.shapes.is_some(),
        doc.comp.is_some(),
        doc.image.is_some(),
        doc.video.is_some(),
        doc.audio.is_some(),
        doc.pivot.is_some(),
        doc.camera.is_some(),
    ];
    let mut keys = CONTENT_KEYS
        .iter()
        .zip(written)
        .filter(|(_, w)| *w)
        .map(|(k, _)| *k);
    let one_of = || {
        let quoted: Vec<String> = CONTENT_KEYS.iter().map(|k| format!("`{k}`")).collect();
        let (last, rest) = quoted.split_last().expect("content keys");
        format!(
            "needs exactly one of {} or {last}, or only `fx` for a filter layer",
            rest.join(", ")
        )
    };
    const EXAMPLE: &str = r#"e.g. "text":"Hello", "rect":"300x100", "shapes":[…], "comp":"card", "image":"photo.jpg", "video":"clip.mov", "audio":"music.mp3" or "pivot":true"#;
    let key = match (keys.next(), keys.next()) {
        (Some(key), None) => key,
        // Only effects: a filter layer.
        (None, _) if doc.fx.as_ref().is_some_and(|List(fx)| !fx.is_empty()) => {
            only_on(doc.fill.is_some(), FILL, FX, &[TEXT, RECT])?;
            only_on(doc.frame.is_some(), BOX, FX, &[TEXT, IMAGE, VIDEO])?;
            only_on(doc.collapse.is_some(), COLLAPSE, FX, &[COMP])?;
            only_on(doc.crop.is_some(), CROP, FX, &[IMAGE])?;
            only_on(doc.continuous.is_some(), CONTINUOUS, FX, &[IMAGE])?;
            only_on(doc.fit.is_some(), FIT, FX, &[TEXT, IMAGE, VIDEO])?;
            return Ok(Content::Filter);
        }
        (None, _) => return Err(Diagnostic::new(Kind::Missing, one_of()).expected(EXAMPLE)),
        (Some(_), Some(second)) => {
            return Err(Diagnostic::new(Kind::Conflict, one_of())
                .at(second)
                .expected(EXAMPLE));
        }
    };
    only_on(doc.fill.is_some(), FILL, key, &[TEXT, RECT])?;
    only_on(!doc.overrides.0.is_empty(), OVERRIDES, key, &[COMP])?;
    only_on(doc.collapse.is_some(), COLLAPSE, key, &[COMP])?;
    only_on(doc.crop.is_some(), CROP, key, &[IMAGE])?;
    only_on(doc.continuous.is_some(), CONTINUOUS, key, &[IMAGE])?;
    // A text layer's `fit` (D77) is its own, and not checked here.
    only_on(doc.fit.is_some(), FIT, key, &[TEXT, IMAGE, VIDEO])?;
    only_on(doc.frame.is_some(), BOX, key, &[TEXT, IMAGE, VIDEO])?;
    let Object(style) = &doc.style;
    for (written, name) in [
        (doc.font.is_some(), FONT),
        (!style.is_empty(), STYLE),
        (doc.runs.is_some(), RUNS),
        (doc.paragraphs.is_some(), PARAGRAPHS),
        (doc.animate.is_some(), ANIMATE),
    ] {
        only_on(written, name, key, &[TEXT])?;
    }
    match key {
        TEXT => Ok(Content::Text(Box::new(text::text(
            doc,
            doc.text.as_deref().expect("written"),
            default_font(),
            DEFAULT_FILL,
            context.fps,
        )?))),
        RECT => Ok(Content::Rect(
            doc.rect.as_ref().expect("written").parse_at(RECT)?,
        )),
        SHAPES_KEY => {
            let List(docs) = doc.shapes.as_ref().expect("written");
            let items = item::items(docs, &SHAPES, context.fps).map_err(|e| e.under(SHAPES_KEY))?;
            item::unique_ids(&items, SHAPES_KEY)?;
            Ok(Content::Shapes(items))
        }
        COMP => {
            let id = doc.comp.clone().expect("written");
            let comps = &context.shared.comps;
            let Some(canvas) = comps.iter().find(|c| c.id == id) else {
                return Err(match comps.is_empty() {
                    true => Diagnostic::new(
                        Kind::UnknownName,
                        format!("unknown composition `{id}`: the scene has none"),
                    )
                    .at(COMP)
                    .expected(r#"a composition under "comps", e.g. "comps":{"card":{"size":"600x400","dur":"5s","layers":[]}}"#),
                    false => Diagnostic::unknown(
                        "composition",
                        &id,
                        comps.iter().map(|c| c.id.as_str()),
                    )
                    .at(COMP),
                });
            };
            let Object(overrides) = &doc.overrides;
            let instance = match overrides.is_empty() {
                true => None,
                false => Some(instance(canvas, overrides, context.shared)?),
            };
            Ok(Content::Comp(CompContent { id, instance }))
        }
        IMAGE => image(doc, doc.image.as_ref().expect("written").as_deref()),
        VIDEO => video(doc, doc.video.as_ref().expect("written").as_deref()),
        AUDIO => audio(doc.audio.as_deref().expect("written")),
        CAMERA => match *doc.camera {
            Some(true) => Ok(Content::Camera(CameraContent {
                aim: doc.values.contains_key(Prop::TARGET.name())
                    || doc.values.contains_key(Prop::TARGET_Z.name())
                    || doc.anim.0.keys().any(|name| {
                        Target::named(name).is_some_and(|t| Prop::CAMERA[1..].contains(&t.prop))
                    }),
            })),
            _ => Err(
                Diagnostic::new(Kind::Invalid, "`camera` is `true` or left out")
                    .at(CAMERA)
                    .expected(r#""camera":true"#),
            ),
        },
        _ => match *doc.pivot {
            Some(true) => Ok(Content::Pivot),
            _ => Err(
                Diagnostic::new(Kind::Invalid, "`pivot` is `true` or left out")
                    .at(PIVOT)
                    .expected(r#""pivot":true"#),
            ),
        },
    }
}

/// The layers of the composition `canvas` names with `overrides` set over
/// them ([`crate::overrides`]), with paths relative to the `comp` layer:
/// the list is validated as any composition's layers are, so an error
/// points at the inner layer, `title.text`.
fn instance(canvas: &Canvas, overrides: &OverridesDoc, shared: &Shared) -> Result<Vec<Layer>> {
    let at_overrides = |d: Diagnostic| d.under(OVERRIDES);
    // The slot values this instance gives, then the layer keys it sets.
    let (slot_entries, layer_entries): (Vec<_>, Vec<_>) = overrides
        .iter()
        .partition(|(target, _)| target.starts_with('$'));
    let comp_slots = shared.declared.of_comp(&canvas.id);
    let given = crate::slots::given(
        comp_slots,
        slot_entries
            .iter()
            .map(|(target, OverrideEntry(value))| (&target[1..], value)),
    )
    .map_err(at_overrides)?;
    // The composition as written, its references resolved with this
    // instance's values first; the layer keys, already resolved where they
    // are written, go over it after, so none is resolved twice.
    let mut written = shared
        .written
        .get(COMPS)
        .and_then(|comps| comps.get(&canvas.id))
        .and_then(|comp| comp.get(LAYERS))
        .filter(|layers| layers.is_array())
        .cloned()
        .expect("the composition was read from this JSON");
    let scopes = [
        given.as_slice(),
        comp_slots,
        shared.declared.scene.as_slice(),
    ];
    crate::slots::resolve_layers(
        &mut written,
        &scopes,
        Some(&canvas.id),
        &mut Default::default(),
    )?;
    let serde_json::Value::Array(mut written) = written else {
        unreachable!("filtered to an array above");
    };
    let at_target = |d: Diagnostic| d.at(OVERRIDES);
    for (target, OverrideEntry(keys)) in layer_entries {
        let Some(keys) = keys.as_object() else {
            let entry = Place::Scene(Address {
                layers: Vec::new(),
                keys: vec![Step::Key(target.clone())],
            });
            return Err(at_overrides(
                Diagnostic::new(
                    Kind::Invalid,
                    format!("`{keys}` is not an object of layer keys"),
                )
                .expected(OverrideDoc::FORM)
                .at(entry),
            ));
        };
        let address = target
            .parse::<Address>()
            .ok()
            .filter(|a| a.keys.is_empty() && !a.layers.is_empty())
            .ok_or_else(|| {
                at_target(
                    Diagnostic::new(
                        Kind::Invalid,
                        format!("`{target}` is not the address of a layer"),
                    )
                    .expected("a layer of the composition, e.g. `title` or `badge/icon`, or a slot, `$accent`"),
                )
            })?;
        if keys.contains_key(ID) {
            return Err(at_target(
                Diagnostic::new(
                    Kind::Conflict,
                    format!("an override of `{target}` sets its `id`"),
                )
                .expected("the keys to change; the id stays the layer's own"),
            ));
        }
        if !crate::overrides::set_over(&mut written, &address.layers, keys) {
            let known: Vec<&str> = written.iter().filter_map(written_id).collect();
            return Err(at_target(Diagnostic::unknown(
                "layer",
                address.layers[0].as_ref(),
                known,
            )));
        }
    }
    let raw = serde_json::json!({ LAYERS: written });
    let read = crate::de::from_value::<List<LayersDoc>>(&raw[LAYERS]).map_err(|mut d| {
        d.path = Some(
            Place::from(LAYERS)
                .join(d.path.take().unwrap_or_else(|| Place::from("")))
                .by_layer_ids(&raw),
        );
        d
    });
    let List(docs) = read?;
    let context = Context {
        shared,
        fps: canvas.fps,
        size: canvas.size,
        duration: canvas.duration,
        labels: &canvas.labels,
    };
    layers(&docs, &context)
}

/// An image layer: a picture file, relative to the scene file, fitted to
/// the layer's `box`, which it must have since the scene is read without
/// opening its files; or, `null`, no file yet, which draws nothing. `crop`,
/// the part of the picture shown, is written as a box is.
fn image(doc: &LayerDoc, path: Option<&str>) -> Result<Content> {
    if path.is_some_and(|path| path.trim().is_empty()) {
        return Err(Diagnostic::new(Kind::Invalid, "an empty image path")
            .at(IMAGE)
            .expected(
                "a PNG, JPEG or PDF file, relative to the scene file, e.g. `photos/beach.jpg`, or null for none yet",
            ));
    }
    let frame = doc.frame.as_ref().ok_or_else(|| {
        Diagnostic::new(Kind::Missing, "missing `box`: an image layer fills its box")
            .expected(r#"the picture's size in px, e.g. "box":[1920,1080]"#)
    })?;
    let crop = doc
        .crop
        .as_ref()
        .map(|crop| text::frame(crop).map_err(|e| e.under(CROP)))
        .transpose()?;
    Ok(Content::Image(ImageContent {
        path: path.map(str::to_owned),
        frame: text::frame(frame).map_err(|e| e.under(BOX))?,
        crop,
        fit: media_fit(doc)?,
        continuous: doc.continuous.unwrap_or(false),
    }))
}

/// A video layer: a clip file, relative to the scene file, fitted to the
/// layer's `box`, which it must have since the scene is read without
/// opening its files; or, `null`, no file yet, which draws nothing.
fn video(doc: &LayerDoc, path: Option<&str>) -> Result<Content> {
    if path.is_some_and(|path| path.trim().is_empty()) {
        return Err(Diagnostic::new(Kind::Invalid, "an empty video path")
            .at(VIDEO)
            .expected(
                "an MP4 or QuickTime clip, relative to the scene file, e.g. `clips/sea.mov`, or null for none yet",
            ));
    }
    let frame = doc.frame.as_ref().ok_or_else(|| {
        Diagnostic::new(Kind::Missing, "missing `box`: a video layer fills its box")
            .expected(r#"the clip's size in px, e.g. "box":[1920,1080]"#)
    })?;
    Ok(Content::Video(VideoContent {
        path: path.map(str::to_owned),
        frame: text::frame(frame).map_err(|e| e.under(BOX))?,
        fit: media_fit(doc)?,
    }))
}

/// An image or video layer's `fit`, `fill` when it writes none.
fn media_fit(doc: &LayerDoc) -> Result<MediaFit> {
    let Some(name) = doc.fit.as_deref() else {
        return Ok(MediaFit::default());
    };
    MediaFit::named(name).ok_or_else(|| {
        Diagnostic::unknown("fit", name, MediaFit::NAMES.map(|n| format!("`{n}`"))).at(FIT)
    })
}

/// An audio layer: a sound file, relative to the scene file. It draws
/// nothing, so it needs no box.
fn audio(path: &str) -> Result<Content> {
    if path.trim().is_empty() {
        return Err(Diagnostic::new(Kind::Invalid, "an empty audio path")
            .at(AUDIO)
            .expected(
                "a WAV, MP3, FLAC or Ogg Vorbis file, relative to the scene file, e.g. `music/theme.mp3`",
            ));
    }
    Ok(Content::Audio(AudioContent {
        path: path.to_owned(),
    }))
}

/// Inter, which ships built in, by the family name its variable font gives.
fn default_font() -> Font {
    Font {
        family: "Inter Variable".into(),
        size: 48.0,
        weight: 400,
    }
}

/// `key`, when written, needs a layer that draws one of `contents`.
fn only_on(written: bool, key: &str, content: &str, contents: &[&str]) -> Result<()> {
    if !written || contents.contains(&content) {
        return Ok(());
    }
    let layers: Vec<String> = contents.iter().map(|c| format!("`{c}`")).collect();
    let layers = layers.join(" or ");
    Err(Diagnostic::new(
        Kind::Conflict,
        format!("`{key}` only applies to {layers} layers"),
    )
    .at(key)
    .expected(format!("`{key}` only next to {layers}")))
}

/// Parses `in` or `out`, written under `key`, and checks that its preset exists.
fn behaviour(
    written: &Shorthand<Behaviour>,
    presets: &[(String, PresetSpec)],
    key: &str,
) -> Result<Behaviour> {
    let behaviour = written.parse_at(key)?;
    match lookup(presets, &behaviour.preset) {
        Some(_) => Ok(behaviour),
        None => Err(Diagnostic::unknown("preset", &behaviour.preset, names(presets)).at(key)),
    }
}

/// A flat layer's entrance or exit may not turn it in depth, as its own
/// depth rows may not.
fn flat_preset(
    layer: &Layer,
    behaviour: &Behaviour,
    presets: &[(String, PresetSpec)],
) -> Result<()> {
    if layer.three_d {
        return Ok(());
    }
    let spec = lookup(presets, &behaviour.preset).expect("the preset was checked");
    match spec
        .props
        .iter()
        .find(|(t, _)| Prop::DEPTH.contains(&t.prop))
    {
        Some((target, _)) => Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "preset `{}` moves `{}`, which places a layer in depth, and `{}` is flat",
                behaviour.preset,
                target.name(),
                layer.id
            ),
        )
        .expected(r#""3d":true on the layer"#)),
        None => Ok(()),
    }
}

/// Checks an exit: it moves the whole layer, which runs forwards, since it
/// ends when the layer's span does on its composition's clock.
fn exit(layer: &Layer, behaviour: &Behaviour) -> Result<()> {
    if let Some(parts) = behaviour.parts {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("an exit cannot move the parts of a text yet (`{parts}`)"),
        )
        .expected("an exit of the whole layer; parts can leave through a text animator"));
    }
    if layer.stretch < 0.0 {
        return Err(Diagnostic::new(
            Kind::Conflict,
            "the layer runs backwards, so its exit would play as an entrance",
        )
        .expected(format!(
            "`{STRETCH}` above 0 with `{OUT}`, or the move as `{ANIM}` tracks"
        )));
    }
    Ok(())
}

/// Rejects an `anim` track for a property the `out` preset animates, and an
/// entrance and an exit that move the same property at once or move
/// overlapping parts of it.
fn exit_conflict(layer: &Layer, context: &Context) -> Result<()> {
    let Some(behaviour) = &layer.exit else {
        return Ok(());
    };
    let Some(spec) = lookup(context.shared.presets, &behaviour.preset) else {
        return Ok(());
    };
    if let Some(track) = layer
        .anim
        .iter()
        .find(|track| spec.props.iter().any(|(t, _)| t.overlaps(track.target)))
    {
        let name = track.target.name();
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!(
                "`{name}` is already animated by `{OUT}` (preset `{}`)",
                behaviour.preset
            ),
        )
        .at(name)
        .under(ANIM)
        .expected(format!(
            "the exit as `{ANIM}` keys after `{OUT}={NONE}`, or a scene preset without `{name}` under \"{PRESETS}\""
        )));
    }
    let Some(entrance) = layer
        .enter
        .as_ref()
        .filter(|b| b.parts.is_none())
        .and_then(|b| Some((b, lookup(context.shared.presets, &b.preset)?)))
    else {
        return Ok(());
    };
    let fps = context.fps;
    let end = Behaviour::last_shown(
        layer
            .span
            .map_or(context.duration, |span| span.end)
            .to_frames(fps),
    );
    let (leaves, _) =
        behaviour.exit_frames((end - layer.offset.to_frames(fps)) / layer.stretch, fps);
    let (_, arrives) = entrance.0.frames(fps);
    for (into, _) in &entrance.1.props {
        let Some((out, _)) = spec.props.iter().find(|(t, _)| t.overlaps(*into)) else {
            continue;
        };
        if out != into || arrives > leaves + 1e-9 {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!(
                    "`{IN}` and `{OUT}` both move `{}`, and the exit starts before the entrance ends",
                    out.name()
                ),
            )
            .at(OUT)
            .expected("an exit that starts once the entrance has ended, a shorter one, or presets that move other properties"));
        }
    }
    Ok(())
}

fn unique_ids(layers: &[Layer]) -> Result<()> {
    let mut seen = HashSet::new();
    match layers.iter().position(|layer| !seen.insert(&layer.id)) {
        None => Ok(()),
        Some(i) => Err(
            Diagnostic::new(Kind::Conflict, format!("duplicate id `{}`", layers[i].id))
                .at(ID)
                .under(layer_path(i))
                .expected("every layer to have its own id"),
        ),
    }
}

/// The tracks of `anim` on an object with `registry`'s properties, no two of
/// which write the same numbers.
fn anim(tracks: &AnimDoc, registry: &'static Registry, fps: f64) -> Result<Vec<AnimTrack>> {
    let tracks = tracks
        .iter()
        .map(|(name, doc)| track(name, doc, registry, fps).map_err(|e| e.under(name)))
        .collect::<Result<Vec<_>>>()?;
    let targets: Vec<Target> = tracks.iter().map(|t| t.target).collect();
    one_writer(&targets)?;
    Ok(tracks)
}

/// No two of `targets` write the same numbers: a property is animated whole
/// or one component at a time, not both. The error sits at the later one.
fn one_writer(targets: &[Target]) -> Result<()> {
    for (i, later) in targets.iter().enumerate() {
        if let Some(earlier) = targets[..i].iter().find(|t| t.overlaps(*later)) {
            let (a, b, prop) = (earlier.name(), later.name(), later.prop.name());
            let components = later.prop.spec().components.join("`, `");
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!("`{a}` and `{b}` both animate `{prop}`"),
            )
            .at(b)
            .expected(format!(
                "`{prop}` animated whole, or its components `{components}` one at a time"
            )));
        }
    }
    Ok(())
}

/// One track: a known target, and keys of its type, in range, in time order
/// and, for a path, of one shape.
fn track(name: &str, doc: &TrackDoc, registry: &'static Registry, fps: f64) -> Result<AnimTrack> {
    let target = known_target(name, registry)?;
    if let TrackDoc::Line(text) = doc
        && let Some(level) = LevelLine::read(text)
    {
        return listening(&level?, target);
    }
    let keys = match doc {
        TrackDoc::Line(_) if target.value_type() == ValueType::Path => {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("`{name}` is a path, which a key list animates"),
            )
            .expected(KEYS_EXPECTED));
        }
        TrackDoc::Line(text) => line(text, target, fps)?,
        TrackDoc::Keys(keys) => key_list(keys, target, fps)?,
    };
    one_shape(&keys)?;
    Ok(AnimTrack {
        target,
        keys,
        listen: None,
    })
}

/// A track that follows a sound's level: one key, its value at silence,
/// and what it listens to. `volume` may not listen, since what a level
/// drives must not change the sound it hears.
fn listening(level: &LevelLine, target: Target) -> Result<AnimTrack> {
    let name = target.name();
    if target.prop.name() == "volume" {
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!("`{name}` cannot follow a sound's level, which it would change"),
        )
        .expected("a property other than `volume`"));
    }
    if !matches!(
        target.value_type(),
        ValueType::Number | ValueType::Point | ValueType::Color | ValueType::Orientation
    ) {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("`{name}` is a path, which cannot follow a sound's level"),
        )
        .expected("a number, a point or a colour"));
    }
    Ok(AnimTrack {
        target,
        keys: vec![AnimKey {
            time: Time::Frames(0.0),
            value: word_value(&level.from, target)?,
            easing: Easing::All(Curve::Linear),
            path: None,
        }],
        listen: Some(Listen {
            layer: level.layer.clone(),
            band: level.band,
            smooth: level.smooth,
            curve: level.curve,
            to: word_value(&level.to, target)?,
        }),
    })
}

/// The target called `name` among `registry`'s properties and components.
fn known_target(name: &str, registry: &'static Registry) -> Result<Target> {
    registry.target(name).ok_or_else(|| {
        Diagnostic::unknown(
            format!("{} property", registry.kind),
            name,
            registry.target_names(),
        )
    })
}

/// Every key of a track has the first key's shape, so a path can move
/// between them vertex by vertex.
fn one_shape(keys: &[AnimKey]) -> Result<()> {
    let Some(first) = keys.first() else {
        return Ok(());
    };
    // Paths have no relative form, so their keys are all set.
    let shape = |k: &AnimKey| k.value.set().cloned();
    match keys.iter().position(|k| match (shape(k), shape(first)) {
        (Some(a), Some(b)) => !a.same_shape(&b),
        _ => false,
    }) {
        None => Ok(()),
        Some(i) => Err(Diagnostic::new(
            Kind::Invalid,
            "a path key differs in vertex count or closure from the first key",
        )
        .under(format!("[{i}]"))
        .expected("keys whose paths have the same vertices, all open or all closed")),
    }
}

/// The string form: both values of the target's type and in range, and a
/// range that runs forward.
fn line(text: &str, target: Target, fps: f64) -> Result<Vec<AnimKey>> {
    let line: TrackLine = text.parse()?;
    let values = line
        .values
        .iter()
        .map(|(word, _)| word_value(word, target))
        .collect::<Result<Vec<_>>>()?;
    runs_forward(line.span, fps)?;
    Ok(line.keys(values, fps))
}

/// A value word of a string form, read as the target's type and
/// range-checked, or written against the rest.
fn word_value(word: &str, target: Target) -> Result<KeyValue> {
    let (ty, broadcast) = (target.value_type(), broadcasts(target));
    if let Some(change) = rest_word(word, ty, broadcast) {
        return change.map(KeyValue::Rest);
    }
    in_range(Value::parse(word, ty, broadcast)?, target.prop.spec()).map(KeyValue::Set)
}

/// Whether a lone number sets every component of the target.
fn broadcasts(target: Target) -> bool {
    target.component.is_none() && target.prop.spec().broadcast
}

/// A value as JSON wrote it, read as the target's type and range-checked.
fn typed_value(written: &ValueDoc, target: Target) -> Result<Value> {
    let (ty, broadcast) = (target.value_type(), broadcasts(target));
    let value = match written {
        ValueDoc::Numbers(numbers) => Value::from_numbers(numbers, ty, broadcast)?,
        ValueDoc::Text(text) => Value::parse(text, ty, broadcast)?,
    };
    in_range(value, target.prop.spec())
}

/// A range starts no later than it ends, compared in frames since units may differ.
fn runs_forward(span: Span, fps: f64) -> Result<()> {
    forward(span, fps, format!("@{span}"), RANGE_EXPECTED)
}

/// A layer's span starts no later than it ends.
fn span_runs_forward(span: TimeSpan, fps: f64) -> Result<()> {
    forward(span, fps, span.to_string(), SPAN_EXPECTED)
}

/// `span`, written as `written`, starts no later than it ends, compared in frames.
fn forward(span: TimeSpan, fps: f64, written: String, expected: &'static str) -> Result<()> {
    if span.start.to_frames(fps) <= span.end.to_frames(fps) {
        return Ok(());
    }
    Err(
        Diagnostic::new(Kind::Invalid, format!("`{written}` starts after it ends"))
            .expected(expected),
    )
}

/// The key-list form: at least one key, each valid, in time order.
fn key_list(docs: &[KeyDoc], target: Target, fps: f64) -> Result<Vec<AnimKey>> {
    if docs.is_empty() {
        return Err(
            Diagnostic::new(Kind::Missing, "needs at least one key").expected(KEYS_EXPECTED)
        );
    }
    let keys = docs
        .iter()
        .enumerate()
        .map(|(i, doc)| key(doc, target).map_err(|e| e.under(format!("[{i}]"))))
        .collect::<Result<Vec<_>>>()?;
    in_time_order(&keys, fps)?;
    Ok(keys)
}

/// One key: a time, which may fall before the layer's own zero, a value of
/// the target's type in range, its easing, and a path only where the target
/// moves in space.
fn key(doc: &KeyDoc, target: Target) -> Result<AnimKey> {
    let time = match &doc.time {
        KeyTime::Written(text) => text.parse()?.0,
        KeyTime::Frames(frames) => Time::Frames(*frames),
    };
    Ok(AnimKey {
        time,
        value: match &doc.value {
            ValueDoc::Text(text) => word_value(text, target),
            numbers => typed_value(numbers, target).map(KeyValue::Set),
        }
        .map_err(|e| e.under(doc.value_path()))?,
        easing: easing(doc.curve.as_ref(), target).map_err(|e| e.under(doc.curve_path()))?,
        path: doc.path.map(|path| bends(path, target)).transpose()?,
    })
}

/// A key's easing: one curve, or one per component of a value that is not spatial.
fn easing(written: Option<&EasingDoc>, target: Target) -> Result<Easing> {
    match written {
        None => Ok(Easing::All(Curve::Linear)),
        Some(EasingDoc::One(curve)) => Ok(Easing::All(curve.parse()?)),
        Some(EasingDoc::Each(curves)) => {
            let Some(dims) = target.value_type().dims() else {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!(
                        "`{}` is a path, so one curve eases every vertex together",
                        target.name()
                    ),
                )
                .expected("one curve"));
            };
            if target.prop.spec().spatial && target.component.is_none() {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!(
                        "`{}` moves along a path, so one curve times the whole move",
                        target.name()
                    ),
                )
                .expected(format!(
                    "one curve, or animate {} one at a time",
                    target.prop.spec().components.join(" and ")
                )));
            }
            if curves.len() != dims {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("{} curves for {dims} components", curves.len()),
                )
                .expected(EASING_EXPECTED));
            }
            let curves = curves
                .iter()
                .enumerate()
                .map(|(i, curve)| {
                    curve
                        .parse()
                        .map_err(|e: Diagnostic| e.under(format!("[{i}]")))
                })
                .collect::<Result<_>>()?;
            Ok(Easing::Each(curves))
        }
    }
}

/// A key's path tangents, only on a spatial property animated whole.
fn bends(PathDoc(path): PathDoc, target: Target) -> Result<[[f64; 2]; 2]> {
    if target.prop.spec().spatial && target.component.is_none() {
        return Ok(path);
    }
    Err(Diagnostic::new(
        Kind::Invalid,
        format!("`{}` does not move along a path", target.name()),
    )
    .at("path")
    .expected(PATH_EXPECTED))
}

/// No key is earlier than the one before it, compared in frames since units may differ.
fn in_time_order(keys: &[AnimKey], fps: f64) -> Result<()> {
    let earlier = keys
        .windows(2)
        .position(|pair| pair[1].time.to_frames(fps) < pair[0].time.to_frames(fps));
    match earlier {
        None => Ok(()),
        Some(i) => Err(
            Diagnostic::new(Kind::Invalid, "key is earlier than the one before it")
                .at(format!("[{}]", i + 1))
                .expected("keys in time order, earliest first"),
        ),
    }
}

/// Every component of `value` is finite and inside the property's inclusive range.
fn in_range(value: Value, spec: &PropertySpec) -> Result<Value> {
    for &n in value.components() {
        in_number_range(n, spec)?;
    }
    Ok(value)
}

/// `value` is finite and inside the property's inclusive range.
fn in_number_range(value: f64, spec: &PropertySpec) -> Result<f64> {
    if !value.is_finite() {
        return Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("`{value}` is not a finite number"),
        )
        .expected("a finite number"));
    }
    match spec.range {
        Some((lo, hi)) if value < lo || value > hi => {
            let (allowed, expected) = match hi {
                f64::INFINITY => (
                    format!("{}..", fmt_num(lo)),
                    format!("a number of at least {}", fmt_num(lo)),
                ),
                _ => (
                    format!("{}..={}", fmt_num(lo), fmt_num(hi)),
                    format!("a number from {} to {}", fmt_num(lo), fmt_num(hi)),
                ),
            };
            Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("value {} is outside {allowed}", fmt_num(value)),
            )
            .expected(expected))
        }
        _ => Ok(value),
    }
}

/// One preset: a valid name and at least one property start.
fn preset(name: &str, body: &PresetDoc) -> Result<PresetSpec> {
    if !Id::is_valid(name) {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("invalid preset name `{name}`"))
                .expected(ID_RULE),
        );
    }
    if body.is_empty() {
        return Err(
            Diagnostic::new(Kind::Missing, format!("preset `{name}` sets no properties"))
                .expected(r#"e.g. {"opacity":0}"#),
        );
    }
    let props: Vec<(Target, Start)> = body
        .iter()
        .map(|(prop, StartDoc(start))| preset_start(prop, *start).map_err(|e| e.under(prop)))
        .collect::<Result<_>>()?;
    let targets: Vec<Target> = props.iter().map(|(t, _)| *t).collect();
    one_writer(&targets)?;
    Ok(PresetSpec { props })
}

/// One start of a preset: a known target and a start of its type in its range.
/// A number starts a point only where the property broadcasts (`scale`); a
/// distance, or the canvas's size, starts only an unbounded number, such as
/// `y` (opacity has a range).
fn preset_start(name: &str, start: Start) -> Result<(Target, Start)> {
    let target = known_target(name, &LAYER)?;
    match start {
        Start::Value(v) => {
            typed_value(&ValueDoc::Numbers(vec![v]), target)?;
        }
        _ if target.prop.spec().range.is_some() || target.value_type() != ValueType::Number => {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("`{name}` is not a number of pixels, so it cannot start at a distance"),
            )
            .expected("a value of its own, e.g. 0"));
        }
        _ => {}
    }
    Ok((target, start))
}

/// Rejects an `anim` track for a property the `in` preset already animates,
/// since two writers would fight. The hint is the edit that moves the
/// entrance into `anim`, when every resting value it needs is known before
/// compiling (a text box is measured only then).
fn entrance_conflict(
    layer: &Layer,
    layers: &[Layer],
    context: &Context,
    written: &AnimDoc,
) -> Result<()> {
    let Some(behaviour) = &layer.enter else {
        return Ok(());
    };
    // An entrance on the parts of a text leaves the layer's own tracks alone.
    if behaviour.parts.is_some() {
        return Ok(());
    }
    let Some(spec) = lookup(context.shared.presets, &behaviour.preset) else {
        return Ok(());
    };
    let Some(track) = layer
        .anim
        .iter()
        .find(|track| spec.props.iter().any(|(t, _)| t.overlaps(track.target)))
    else {
        return Ok(());
    };
    let name = track.target.name();
    let canvas = Rect::new(
        0.0,
        0.0,
        f64::from(context.size.width),
        f64::from(context.size.height),
    );
    // `at` resolves in the parent's box, or on the canvas without a parent.
    let extent = match &layer.parent {
        Some(id) => layers
            .iter()
            .find(|l| &l.id == id)
            .and_then(|parent| known_box(parent, context)),
        None => Some(canvas),
    };
    let rest = |target: Target| {
        let whole = match (layer.value(target.prop), &target.prop.spec().rest) {
            (Some(value), _) => value,
            (None, Rest::Value(value)) => value.clone(),
            (None, Rest::At) => target.prop.spec().rest(&layer.at, extent?, Rect::ZERO),
            (None, Rest::BoxCentre) => {
                target
                    .prop
                    .spec()
                    .rest(&layer.at, canvas, known_box(layer, context)?)
            }
            (None, Rest::Zoom | Rest::Depth | Rest::OwnTime) => {
                layer.rest(target.prop, canvas, Rect::ZERO)
            }
        };
        Some(target.rest_of(whole))
    };
    let edit = expand_as_anim(spec, behaviour, rest, (context.fps, (canvas.width(), canvas.height())), written)
        .map(|expansion| {
            format!(
                "`{} {IN}={NONE} {ANIM}={expansion}` (the entrance as anim tracks; then adjust `{name}`), or ",
                layer.id
            )
        })
        .unwrap_or_default();
    Err(Diagnostic::new(
        Kind::Conflict,
        format!(
            "`{name}` is already animated by `in` (preset `{}`)",
            behaviour.preset
        ),
    )
    .at(name)
    .under(ANIM)
    .expected(format!(
        "{edit}define a scene preset without `{name}` under \"{PRESETS}\""
    )))
}

/// A layer's box in its own pixels when it is known before compiling: a
/// rect's, a composition's canvas, an image's `box`, and an empty box at the
/// origin for shapes and pivots. A text box is known only once the text is set.
fn known_box(layer: &Layer, context: &Context) -> Option<Rect> {
    match &layer.content {
        Content::Rect(rect) => Some(Rect::new(0.0, 0.0, rect.width, rect.height)),
        Content::Comp(comp) => context
            .shared
            .comps
            .iter()
            .find(|c| c.id == comp.id)
            .map(|c| Rect::new(0.0, 0.0, f64::from(c.size.width), f64::from(c.size.height))),
        Content::Image(image) => Some(image.frame),
        Content::Video(video) => Some(video.frame),
        Content::Shapes(_)
        | Content::Audio(_)
        | Content::Pivot
        | Content::Camera(_)
        | Content::Filter => Some(Rect::ZERO),
        Content::Text { .. } => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/validate.rs"]
mod tests;
