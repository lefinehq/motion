//! The rules a scene follows beyond its JSON shape, one small function each,
//! turning the document into the typed [`Scene`]. A rule reports its error
//! without the enclosing path; the caller places it with [`Diagnostic::under`].

mod item;

use std::collections::HashSet;
use std::str::FromStr;

use crate::{Diagnostic, Kind, Result};

use crate::anim::{
    AnimKey, AnimTrack, EASING_EXPECTED, Easing, KEYS_EXPECTED, PATH_EXPECTED, RANGE_EXPECTED,
    Span, TrackLine,
};
use crate::doc::{
    AnimDoc, CompDoc, CompsDoc, EasingDoc, KeyDoc, KeyTime, LayerDoc, LayersDoc, List, Number,
    Object, PathDoc, PresetDoc, PresetsDoc, SceneDoc, Shorthand, StartDoc, TrackDoc, ValueDoc,
};
use crate::edit_word::NONE;
use crate::item::{EFFECTS, SHAPES};
use crate::parse::key::{
    ANIM, AT, BG, COMP, DUR, FILL, FONT, FPS, FX, ID, IN, LAYERS, MOTION, OFFSET, PARENT, PIVOT,
    PRESETS, RECT, SHAPES as SHAPES_KEY, SIZE, SPAN, STRETCH, TEXT, comp_path, layer_path,
};
use crate::parse::layer_keys;
use crate::preset::{PresetSpec, Start, expand_as_anim, lookup, names};
use crate::property::{LAYER, Prop, PropertySpec, Registry, Rest, Target};
use crate::shorthand::{Behaviour, Coord, Font, Position};
use crate::units::{Color, SPAN_EXPECTED, Size, TIME_EXPECTED, Time, TimeSpan};
use crate::value::{Value, ValueType};
use crate::{Comp, Content, Curve, Layer, Scene, fmt_num};

/// The id rule, as an accepted form.
pub(crate) const ID_RULE: &str =
    "ids use lowercase letters, digits, `-` and `_`, starting with a letter";

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
const DEFAULT_FILL: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const DEFAULT_POSITION: Position = Position {
    x: Coord::Center,
    y: Coord::Center,
};

/// What a layer needs from the scene around it.
struct Context<'a> {
    presets: &'a [(String, PresetSpec)],
    /// Every composition's id and size, read before any layer.
    comps: &'a [(String, Size)],
    /// The frames per second of the layer's composition.
    fps: f64,
    /// The canvas of the layer's composition.
    size: Size,
}

/// Validates a scene document into a [`Scene`], with the defaults of the keys left out.
pub(crate) fn scene(doc: &SceneDoc) -> Result<Scene> {
    version(required(*doc.motion, MOTION, &VERSION.to_string())?)?;
    let fps = doc.fps.map_or(Ok(DEFAULT_FPS), fps)?;
    let size = optional(doc.size.as_ref(), SIZE)?.unwrap_or(DEFAULT_SIZE);
    let duration = required(doc.dur.as_ref(), DUR, r#""5s""#)?.parse_at(DUR)?;
    let background = optional(doc.bg.as_ref(), BG)?.unwrap_or(DEFAULT_BACKGROUND);
    let Object(own_presets) = &doc.presets;
    let presets = presets(own_presets).map_err(|e| e.under(PRESETS))?;
    let Object(comp_docs) = &doc.comps;
    let comp_sizes = comp_sizes(comp_docs)?;
    let List(layer_docs) = required(doc.layers.as_ref(), LAYERS, "[]")?;
    let context = Context {
        presets: &presets,
        comps: &comp_sizes,
        fps,
        size,
    };
    let comps = comp_docs
        .iter()
        .map(|(id, Object(comp_doc))| {
            comp(id, comp_doc, &context).map_err(|e| e.under(comp_path(id)))
        })
        .collect::<Result<Vec<_>>>()?;
    let layers = layers(layer_docs, &context)?;
    no_comp_draws_itself(&comps)?;
    Ok(Scene {
        version: VERSION,
        size,
        fps,
        duration,
        background,
        presets,
        comps,
        layers,
    })
}

/// A list of layers on one canvas, with paths relative to the list's owner:
/// each layer valid, ids unique, parents that exist and never loop, and no
/// track fighting an entrance.
fn layers(docs: &LayersDoc, context: &Context) -> Result<Vec<Layer>> {
    let layers = docs
        .iter()
        .enumerate()
        .map(|(i, Object(layer_doc))| layer(layer_doc, context).map_err(|e| e.under(layer_path(i))))
        .collect::<Result<Vec<_>>>()?;
    unique_ids(&layers)?;
    parents(&layers)?;
    for (i, (layer, Object(doc))) in layers.iter().zip(docs).enumerate() {
        let Object(written) = &doc.anim;
        entrance_conflict(layer, &layers, context, written).map_err(|e| e.under(layer_path(i)))?;
    }
    Ok(layers)
}

/// Every composition's id, which follows the id rule, and size, which it must set.
fn comp_sizes(docs: &CompsDoc) -> Result<Vec<(String, Size)>> {
    docs.iter()
        .map(|(id, Object(doc))| {
            let place = |e: Diagnostic| e.under(comp_path(id));
            if !is_valid_id(id) {
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
            Ok((id.clone(), size))
        })
        .collect()
}

/// One composition, with paths relative to it: its clock defaults to the
/// scene's, its background to clear.
fn comp(id: &str, doc: &CompDoc, scene: &Context) -> Result<Comp> {
    let size = scene
        .comps
        .iter()
        .find(|(c, _)| c == id)
        .map(|(_, size)| *size)
        .expect("comp sizes are read first");
    let fps = doc.fps.map_or(Ok(scene.fps), fps)?;
    let duration = required(doc.dur.as_ref(), DUR, r#""5s""#)?.parse_at(DUR)?;
    let background = optional(doc.bg.as_ref(), BG)?.unwrap_or(CLEAR);
    let List(layer_docs) = required(doc.layers.as_ref(), LAYERS, "[]")?;
    let context = Context {
        presets: scene.presets,
        comps: scene.comps,
        fps,
        size,
    };
    Ok(Comp {
        id: id.to_owned(),
        size,
        fps,
        duration,
        background,
        layers: layers(layer_docs, &context)?,
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
                Content::Comp(id) => Some((i, id.as_str())),
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
                .under(layer_path(first))
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

fn version(motion: Number) -> Result<()> {
    if motion.0 == f64::from(VERSION) {
        return Ok(());
    }
    Err(
        Diagnostic::new(Kind::Invalid, format!("unsupported version {}", motion.0))
            .at(MOTION)
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
fn layer(doc: &LayerDoc, context: &Context) -> Result<Layer> {
    // Unknown keys first, as serde reports them for documents that deny them.
    let values = resting_values(doc)?;
    let id = required(doc.id.as_deref(), ID, r#""title""#)?;
    follows_id_rule(id)?;
    let Object(tracks) = &doc.anim;
    let layer = Layer {
        id: id.to_owned(),
        content: content(doc, context)?,
        fill: optional(doc.fill.as_ref(), FILL)?.unwrap_or(DEFAULT_FILL),
        at: optional(doc.at.as_ref(), AT)?.unwrap_or(DEFAULT_POSITION),
        values,
        enter: doc
            .enter
            .as_ref()
            .map(|written| entrance(written, context.presets))
            .transpose()?,
        anim: anim(tracks, &LAYER, context.fps).map_err(|e| e.under(ANIM))?,
        parent: doc.parent.clone(),
        span: optional(doc.span.as_ref(), SPAN)?,
        offset: optional(doc.offset.as_ref(), OFFSET)?.unwrap_or(Time::Frames(0.0)),
        stretch: doc.stretch.map_or(Ok(1.0), stretch)?,
        fx: match &*doc.fx {
            Some(List(docs)) => item::items(docs, &EFFECTS, context.fps)
                .and_then(|fx| item::unique_ids(&fx, "").map(|()| fx))
                .map_err(|e| e.under(FX))?,
            None => Vec::new(),
        },
        hidden: doc.hidden.unwrap_or(false),
    };
    if let Some(span) = layer.span {
        span_runs_forward(span, context.fps).map_err(|e| e.under(SPAN))?;
    }
    Ok(layer)
}

/// The layer keys that are not fixed keys: each a property, set to a value of its type.
/// A property that rests on `at` is set with `at`, so it is not a layer key.
fn resting_values(doc: &LayerDoc) -> Result<Vec<(Prop, Value)>> {
    doc.values
        .iter()
        .map(|(name, written)| {
            let prop = Prop::named(name).ok_or_else(|| {
                Diagnostic::unknown("layer key", name, layer_keys()).at(name.as_str())
            })?;
            if prop.spec().rest == Rest::At {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("`{name}` rests where `{AT}` puts it"),
                )
                .at(name.as_str())
                .expected(format!(
                    r#"e.g. "{AT}":"540 960", or animate it in `{ANIM}`"#
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

/// Every `parent` names another layer, and following parents never comes back.
fn parents(layers: &[Layer]) -> Result<()> {
    let index = |id: &str| layers.iter().position(|layer| layer.id == id);
    for (i, layer) in layers.iter().enumerate() {
        let Some(parent) = &layer.parent else {
            continue;
        };
        let place = |d: Diagnostic| d.at(PARENT).under(layer_path(i));
        let mut next = index(parent).ok_or_else(|| {
            place(Diagnostic::unknown(
                "layer id",
                parent,
                layers.iter().map(|l| l.id.as_str()),
            ))
        })?;
        // A chain without a cycle visits each layer at most once.
        for _ in 0..layers.len() {
            if next == i {
                return Err(place(
                    Diagnostic::new(
                        Kind::Invalid,
                        format!("`{}` would move with itself through its parents", layer.id),
                    )
                    .expected("parents that never lead back to the layer"),
                ));
            }
            match &layers[next].parent {
                Some(up) => next = index(up).unwrap_or(next),
                None => break,
            }
        }
    }
    Ok(())
}

fn follows_id_rule(id: &str) -> Result<()> {
    if is_valid_id(id) {
        return Ok(());
    }
    Err(Diagnostic::new(Kind::Invalid, format!("invalid id `{id}`"))
        .at(ID)
        .expected(ID_RULE))
}

/// `[a-z][a-z0-9_-]*`
pub(crate) fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// The keys that say what a layer draws, of which it has exactly one.
const CONTENT_KEYS: [&str; 5] = [TEXT, RECT, SHAPES_KEY, COMP, PIVOT];

/// A layer draws exactly one of [`CONTENT_KEYS`]. None is `Missing` and two
/// is a `Conflict`, at the second, with the same message since the fix is the same.
fn content(doc: &LayerDoc, context: &Context) -> Result<Content> {
    let written = [
        doc.text.is_some(),
        doc.rect.is_some(),
        doc.shapes.is_some(),
        doc.comp.is_some(),
        doc.pivot.is_some(),
    ];
    let mut keys = CONTENT_KEYS
        .iter()
        .zip(written)
        .filter(|(_, w)| *w)
        .map(|(k, _)| *k);
    let one_of = || {
        let quoted: Vec<String> = CONTENT_KEYS.iter().map(|k| format!("`{k}`")).collect();
        let (last, rest) = quoted.split_last().expect("content keys");
        format!("needs exactly one of {} or {last}", rest.join(", "))
    };
    const EXAMPLE: &str =
        r#"e.g. "text":"Hello", "rect":"300x100", "shapes":[…], "comp":"card" or "pivot":true"#;
    let key = match (keys.next(), keys.next()) {
        (Some(key), None) => key,
        (None, _) => return Err(Diagnostic::new(Kind::Missing, one_of()).expected(EXAMPLE)),
        (Some(_), Some(second)) => {
            return Err(Diagnostic::new(Kind::Conflict, one_of())
                .at(second)
                .expected(EXAMPLE));
        }
    };
    only_on(doc.font.is_some(), FONT, key, &[TEXT])?;
    only_on(doc.fill.is_some(), FILL, key, &[TEXT, RECT])?;
    match key {
        TEXT => Ok(Content::Text {
            text: doc.text.clone().expect("written"),
            font: optional(doc.font.as_ref(), FONT)?.unwrap_or_else(default_font),
        }),
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
            match context.comps.iter().any(|(c, _)| *c == id) {
                true => Ok(Content::Comp(id)),
                false if context.comps.is_empty() => Err(Diagnostic::new(
                    Kind::UnknownName,
                    format!("unknown composition `{id}`: the scene has none"),
                )
                .at(COMP)
                .expected(r#"a composition under "comps", e.g. "comps":{"card":{"size":"600x400","dur":"5s","layers":[]}}"#)),
                false => Err(Diagnostic::unknown(
                    "composition",
                    &id,
                    context.comps.iter().map(|(c, _)| c.as_str()),
                )
                .at(COMP)),
            }
        }
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

fn default_font() -> Font {
    Font {
        family: "Inter".into(),
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

/// Parses `in` and checks that its preset exists.
fn entrance(written: &Shorthand<Behaviour>, presets: &[(String, PresetSpec)]) -> Result<Behaviour> {
    let behaviour = written.parse_at(IN)?;
    match lookup(presets, &behaviour.preset) {
        Some(_) => Ok(behaviour),
        None => Err(Diagnostic::unknown("preset", &behaviour.preset, names(presets)).at(IN)),
    }
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
    Ok(AnimTrack { target, keys })
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
    match keys.iter().position(|k| !k.value.same_shape(&first.value)) {
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
    let from = word_value(&line.from, target)?;
    let to = word_value(&line.to, target)?;
    runs_forward(line.span, fps)?;
    Ok(line.keys(from, to))
}

/// A value word of a string form, read as the target's type and range-checked.
fn word_value(word: &str, target: Target) -> Result<Value> {
    let value = Value::parse(word, target.value_type(), broadcasts(target))?;
    in_range(value, target.prop.spec())
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

/// One key: a time that is not negative, a value of the target's type in
/// range, its easing, and a path only where the target moves in space.
fn key(doc: &KeyDoc, target: Target) -> Result<AnimKey> {
    let time = match &doc.time {
        KeyTime::Written(text) => text.parse()?,
        KeyTime::Frames(frames) => not_negative(*frames)?,
    };
    Ok(AnimKey {
        time,
        value: typed_value(&doc.value, target).map_err(|e| e.under(doc.value_path()))?,
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

/// A bare frame count must not be negative; a written time has no sign to check.
fn not_negative(frames: f64) -> Result<Time> {
    if frames >= 0.0 {
        return Ok(Time::Frames(frames));
    }
    Err(
        Diagnostic::new(Kind::OutOfRange, format!("invalid time {frames}"))
            .expected(format!("a frame count of 0 or more, or {TIME_EXPECTED}")),
    )
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
    if !is_valid_id(name) {
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
/// distance starts only an unbounded number, such as `y` (opacity has a range).
fn preset_start(name: &str, start: Start) -> Result<(Target, Start)> {
    let target = known_target(name, &LAYER)?;
    match start {
        Start::Value(v) => {
            typed_value(&ValueDoc::Numbers(vec![v]), target)?;
        }
        Start::Plus | Start::Minus
            if target.prop.spec().range.is_some() || target.value_type() != ValueType::Number =>
        {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("`{name}` is not a number of pixels, so it cannot start at a distance"),
            )
            .expected("a value of its own, e.g. 0"));
        }
        Start::Plus | Start::Minus => {}
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
    let Some(spec) = lookup(context.presets, &behaviour.preset) else {
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
    let canvas = (
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
            (None, Rest::At) => target.prop.spec().rest(&layer.at, extent?, (0.0, 0.0)),
            (None, Rest::BoxCentre) => {
                target
                    .prop
                    .spec()
                    .rest(&layer.at, canvas, known_box(layer, context)?)
            }
        };
        Some(target.rest_of(whole))
    };
    let edit = expand_as_anim(spec, behaviour, rest, context.fps, written)
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

/// A layer's box in pixels when it is known before compiling: a rect's, a
/// composition's canvas, and none for shapes and pivots, whose own
/// coordinates start at their anchor.
fn known_box(layer: &Layer, context: &Context) -> Option<(f64, f64)> {
    match &layer.content {
        Content::Rect(rect) => Some((rect.width, rect.height)),
        Content::Comp(id) => context
            .comps
            .iter()
            .find(|(c, _)| c == id)
            .map(|(_, size)| (f64::from(size.width), f64::from(size.height))),
        Content::Shapes(_) | Content::Pivot => Some((0.0, 0.0)),
        Content::Text { .. } => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/validate.rs"]
mod tests;
