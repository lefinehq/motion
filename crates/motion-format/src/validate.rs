//! The rules a scene must follow beyond its JSON shape, checked in one pass.
//!
//! Part of step 1 (Read), after `crate::doc`. Serde has already checked the
//! shape; [`scene`] turns the document into the typed [`Scene`], parsing each
//! shorthand where it was written and checking every rule that types cannot
//! express, including the keys a scene cannot do without. Each rule is a
//! small function. Errors are built without the enclosing path and placed
//! with [`Diagnostic::under`] by the caller, so a rule does not need to know
//! where its value sits. Every key in a path is a constant of
//! `crate::parse::key`.

use std::collections::HashSet;
use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Kind, Result};

use crate::anim::{AnimKey, AnimTrack, KEYS_EXPECTED, RANGE_EXPECTED, Span, TrackLine};
use crate::doc::{
    AnimDoc, KeyDoc, KeyTime, LayerDoc, List, Number, Object, PresetDoc, PresetsDoc, SceneDoc,
    Shorthand, StartDoc, TrackDoc,
};
use crate::edit_word::NONE;
use crate::parse::key::{
    ANIM, AT, BG, DUR, FILL, FONT, FPS, ID, IN, LAYERS, MOTION, PRESETS, RECT, SIZE,
};
use crate::preset::{PresetSpec, Start, expand_as_anim, lookup, names};
use crate::property::{Prop, PropertySpec, property_names};
use crate::shorthand::{Behaviour, Coord, Font, Position};
use crate::units::{Color, Size, TIME_EXPECTED, Time};
use crate::{Content, Curve, Layer, Scene, fmt_num};

/// The id rule, as an accepted form.
pub(crate) const ID_RULE: &str =
    "ids use lowercase letters, digits, `-` and `_`, starting with a letter";

/// The only format version.
const VERSION: f64 = 1.0;
/// Frames per second when `fps` is left out.
const DEFAULT_FPS: f64 = 30.0;
/// The canvas when `size` is left out.
const DEFAULT_SIZE: Size = Size {
    width: 1920,
    height: 1080,
};
/// The background when `bg` is left out: opaque black.
const DEFAULT_BACKGROUND: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};
/// A layer's fill when `fill` is left out: opaque white.
const DEFAULT_FILL: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// A layer's position when `at` is left out: the centre of the canvas.
const DEFAULT_POSITION: Position = Position {
    x: Coord::Center,
    y: Coord::Center,
};

/// What a layer needs from the scene around it.
struct Stage<'a> {
    /// The scene's own presets.
    presets: &'a [(String, PresetSpec)],
    /// Frames per second.
    fps: f64,
    /// The canvas size.
    size: Size,
}

/// Validates a scene document and builds the typed [`Scene`], applying the
/// defaults of the keys left out.
pub(crate) fn scene(doc: &SceneDoc) -> Result<Scene> {
    version(required(doc.motion, MOTION, "1")?)?;
    let fps = doc.fps.map_or(Ok(DEFAULT_FPS), fps)?;
    let size = optional(doc.size.as_ref(), SIZE)?.unwrap_or(DEFAULT_SIZE);
    let duration = required(doc.dur.as_ref(), DUR, r#""5s""#)?.parse_at(DUR)?;
    let background = optional(doc.bg.as_ref(), BG)?.unwrap_or(DEFAULT_BACKGROUND);
    let Object(own_presets) = &doc.presets;
    let presets = presets(own_presets).map_err(|e| e.under(PRESETS))?;
    let List(layer_docs) = required(doc.layers.as_ref(), LAYERS, "[]")?;
    let stage = Stage {
        presets: &presets,
        fps,
        size,
    };
    let layers = layer_docs
        .iter()
        .enumerate()
        .map(|(i, Object(layer_doc))| {
            layer(layer_doc, &stage).map_err(|e| e.under(format!("{LAYERS}[{i}]")))
        })
        .collect::<Result<Vec<_>>>()?;
    unique_ids(&layers)?;
    Ok(Scene {
        version: 1,
        size,
        fps,
        duration,
        background,
        presets,
        layers,
    })
}

/// Validates the `presets` object: names to property starts, in the order
/// written. The built-ins in `presets.json` go through the same rules.
pub(crate) fn presets(doc: &PresetsDoc) -> Result<Vec<(String, PresetSpec)>> {
    doc.iter()
        .map(|(name, Object(body))| {
            preset(name, body)
                .map(|spec| (name.clone(), spec))
                .map_err(|e| e.under(name))
        })
        .collect()
}

/// A key the scene cannot do without. The expected form shows it written,
/// with `value` as an example.
fn required<T>(written: Option<T>, key: &str, value: &str) -> Result<T> {
    written.ok_or_else(|| {
        Diagnostic::new(Kind::Missing, format!("missing `{key}`"))
            .expected(format!(r#"e.g. "{key}":{value}"#))
    })
}

/// `motion` must be the one version there is.
fn version(motion: Number) -> Result<()> {
    if motion.0 == VERSION {
        return Ok(());
    }
    Err(
        Diagnostic::new(Kind::Invalid, format!("unsupported version {}", motion.0))
            .at(MOTION)
            .expected("`1`"),
    )
}

/// `fps` must be above zero.
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
fn layer(doc: &LayerDoc, stage: &Stage) -> Result<Layer> {
    let id = required(doc.id.as_deref(), ID, r#""title""#)?;
    follows_id_rule(id)?;
    let Object(tracks) = &doc.anim;
    let layer = Layer {
        id: id.to_owned(),
        content: content(doc)?,
        fill: optional(doc.fill.as_ref(), FILL)?.unwrap_or(DEFAULT_FILL),
        at: optional(doc.at.as_ref(), AT)?.unwrap_or(DEFAULT_POSITION),
        enter: doc
            .enter
            .as_ref()
            .map(|written| entrance(written, stage.presets))
            .transpose()?,
        anim: anim(tracks, stage.fps).map_err(|e| e.under(ANIM))?,
    };
    entrance_conflict(&layer, stage, tracks)?;
    Ok(layer)
}

/// The id must follow [`ID_RULE`].
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

/// A layer draws exactly one of `text` or `rect`. Neither is missing
/// content and both are a contradiction, so the kinds differ; the message is
/// the same, since the fix is the same.
fn content(doc: &LayerDoc) -> Result<Content> {
    const ONE_OF: &str = "needs exactly one of `text` or `rect`";
    const EXAMPLE: &str = r#"e.g. "text":"Hello" or "rect":"300x100""#;
    match (&doc.text, &doc.rect) {
        (Some(text), None) => Ok(Content::Text {
            text: text.clone(),
            font: optional(doc.font.as_ref(), FONT)?.unwrap_or_else(default_font),
        }),
        (None, Some(rect)) => {
            font_only_on_text(doc)?;
            Ok(Content::Rect(rect.parse_at(RECT)?))
        }
        (None, None) => Err(Diagnostic::new(Kind::Missing, ONE_OF).expected(EXAMPLE)),
        (Some(_), Some(_)) => Err(Diagnostic::new(Kind::Conflict, ONE_OF).expected(EXAMPLE)),
    }
}

/// The font of a text layer when `font` is left out.
fn default_font() -> Font {
    Font {
        family: "Inter".into(),
        size: 48.0,
        weight: 400,
    }
}

/// `font` means nothing on a rect layer.
fn font_only_on_text(doc: &LayerDoc) -> Result<()> {
    match doc.font {
        None => Ok(()),
        Some(_) => Err(
            Diagnostic::new(Kind::Conflict, "`font` only applies to text layers")
                .at(FONT)
                .expected("`font` only next to `text`"),
        ),
    }
}

/// Parses `in` and checks that its preset exists, among the scene's own or
/// the built-ins.
fn entrance(written: &Shorthand<Behaviour>, presets: &[(String, PresetSpec)]) -> Result<Behaviour> {
    let behaviour = written.parse_at(IN)?;
    match lookup(presets, &behaviour.preset) {
        Some(_) => Ok(behaviour),
        None => Err(Diagnostic::unknown("preset", &behaviour.preset, names(presets)).at(IN)),
    }
}

/// Every layer has its own id.
fn unique_ids(layers: &[Layer]) -> Result<()> {
    let mut seen = HashSet::new();
    match layers.iter().position(|layer| !seen.insert(&layer.id)) {
        None => Ok(()),
        Some(i) => Err(
            Diagnostic::new(Kind::Conflict, format!("duplicate id `{}`", layers[i].id))
                .at(ID)
                .under(format!("{LAYERS}[{i}]"))
                .expected("every layer to have its own id"),
        ),
    }
}

/// Validates the tracks of `anim`, in the order written.
fn anim(tracks: &AnimDoc, fps: f64) -> Result<Vec<AnimTrack>> {
    tracks
        .iter()
        .map(|(name, doc)| track(name, doc, fps).map_err(|e| e.under(name)))
        .collect()
}

/// One track: a known property, and keys in its range and in time order.
fn track(name: &str, doc: &TrackDoc, fps: f64) -> Result<AnimTrack> {
    let property = known_property(name)?;
    let keys = match doc {
        TrackDoc::Line(text) => line(text, property.spec(), fps)?,
        TrackDoc::Keys(keys) => key_list(keys, property.spec(), fps)?,
    };
    Ok(AnimTrack { property, keys })
}

/// Looks `name` up in the property registry.
fn known_property(name: &str) -> Result<Prop> {
    Prop::named(name).ok_or_else(|| Diagnostic::unknown("property", name, property_names()))
}

/// The string form: both values in range, and a range that runs forward.
fn line(text: &str, spec: &PropertySpec, fps: f64) -> Result<Vec<AnimKey>> {
    let line: TrackLine = text.parse()?;
    in_range(line.from, spec)?;
    in_range(line.to, spec)?;
    runs_forward(line.span, fps)?;
    Ok(line.keys())
}

/// A range starts no later than it ends. Its ends may use different units,
/// so they are compared in frames.
fn runs_forward(span: Span, fps: f64) -> Result<()> {
    if span.start.to_frames(fps) <= span.end.to_frames(fps) {
        return Ok(());
    }
    Err(Diagnostic::new(
        Kind::Invalid,
        format!("`@{}-{}` starts after it ends", span.start, span.end),
    )
    .expected(RANGE_EXPECTED))
}

/// The key-list form: at least one key, each valid, in time order.
fn key_list(docs: &[KeyDoc], spec: &PropertySpec, fps: f64) -> Result<Vec<AnimKey>> {
    if docs.is_empty() {
        return Err(
            Diagnostic::new(Kind::Missing, "needs at least one key").expected(KEYS_EXPECTED)
        );
    }
    let keys = docs
        .iter()
        .enumerate()
        .map(|(i, doc)| key(doc, spec).map_err(|e| e.under(format!("[{i}]"))))
        .collect::<Result<Vec<_>>>()?;
    in_time_order(&keys, fps)?;
    Ok(keys)
}

/// One key: a time that is not negative, a value in range and a curve.
fn key(doc: &KeyDoc, spec: &PropertySpec) -> Result<AnimKey> {
    let time = match &doc.time {
        KeyTime::Written(text) => text.parse()?,
        KeyTime::Frames(frames) => not_negative(*frames)?,
    };
    let curve = doc
        .curve
        .as_ref()
        .map_or(Ok(Curve::Linear), Shorthand::parse)?;
    Ok(AnimKey {
        time,
        value: in_range(doc.value, spec)?,
        curve,
    })
}

/// A bare frame count is a time only when it is not negative. (A written
/// time cannot be negative: its grammar has no sign.)
fn not_negative(frames: f64) -> Result<Time> {
    if frames >= 0.0 {
        return Ok(Time::Frames(frames));
    }
    Err(
        Diagnostic::new(Kind::OutOfRange, format!("invalid time {frames}"))
            .expected(format!("a frame count of 0 or more, or {TIME_EXPECTED}")),
    )
}

/// No key is earlier than the one before it. Keys may mix units, so they
/// are compared in frames.
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

/// `value` is finite and inside the property's inclusive range.
fn in_range(value: f64, spec: &PropertySpec) -> Result<f64> {
    if !value.is_finite() {
        return Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("`{value}` is not a finite number"),
        )
        .expected("a finite number"));
    }
    match spec.range {
        Some((lo, hi)) if value < lo || value > hi => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!(
                "value {} is outside {}..={}",
                fmt_num(value),
                fmt_num(lo),
                fmt_num(hi)
            ),
        )
        .expected(format!("a number from {} to {}", fmt_num(lo), fmt_num(hi)))),
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
    let props = body
        .iter()
        .map(|(prop, StartDoc(start))| preset_start(prop, *start).map_err(|e| e.under(prop)))
        .collect::<Result<_>>()?;
    Ok(PresetSpec { props })
}

/// One start of a preset: a known property, and a start it can take. An
/// absolute start obeys the property's range, like an `anim` key; a
/// distance in pixels means nothing for a bounded property such as opacity,
/// and would leave its range.
fn preset_start(name: &str, start: Start) -> Result<(Prop, Start)> {
    let prop = known_property(name)?;
    match start {
        Start::Value(v) => {
            in_range(v, prop.spec())?;
        }
        Start::Plus | Start::Minus if prop.spec().range.is_some() => {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("`{name}` has a fixed range, so it cannot start at a distance"),
            )
            .expected("a number within its range, e.g. 0"));
        }
        Start::Plus | Start::Minus => {}
    }
    Ok((prop, start))
}

/// Rejects an `anim` track for a property the `in` preset already animates:
/// two writers of one property would fight, so the author must pick one. The
/// error gives the exact edit that moves the entrance into `anim`, keeping
/// the other tracks as written.
fn entrance_conflict(layer: &Layer, stage: &Stage, written: &AnimDoc) -> Result<()> {
    let Some(behaviour) = &layer.enter else {
        return Ok(());
    };
    let Some(spec) = lookup(stage.presets, &behaviour.preset) else {
        return Ok(());
    };
    let Some(track) = layer
        .anim
        .iter()
        .find(|track| spec.props.iter().any(|(p, _)| *p == track.property))
    else {
        return Ok(());
    };
    let name = track.property.name();
    let canvas = (f64::from(stage.size.width), f64::from(stage.size.height));
    let expansion = expand_as_anim(spec, behaviour, &layer.at, canvas, stage.fps, written);
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
        "`{} {IN}={NONE} {ANIM}={expansion}` (the entrance as anim tracks; then adjust `{name}`), or define a scene preset without `{name}` under \"{PRESETS}\"",
        layer.id
    )))
}

#[cfg(test)]
#[path = "../tests/unit/validate.rs"]
mod tests;
