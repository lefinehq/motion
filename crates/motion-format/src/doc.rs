//! The scene document as JSON writes it, read with serde.
//!
//! Part of step 1 (Read), between the JSON text and `crate::validate`. These
//! types mirror the JSON: each key has a field, and unknown keys are
//! rejected. Serde reports only structural errors here (a wrong JSON type or
//! an unknown key); shorthand strings are kept as written in a
//! [`Shorthand`] and parsed, with every rule that needs more than one value,
//! in `crate::validate`.
//!
//! Every value that can have the wrong JSON type carries its accepted form
//! ([`Form`]) in its type, so serde's wrong-type error names the form the
//! author has to write rather than "a string" or "a map". The track and key
//! forms have hand-written visitors rather than `#[serde(untagged)]` for the
//! same reason: a wrong value names the forms it could have been instead of
//! "did not match any variant".
//!
//! Every key may be left out as far as serde is concerned, so that
//! `crate::validate` reports a missing required key with an example of it.
//! No key may be `null`: [`present`] reads `null` as a wrong type.

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use indexmap::IndexMap;
use motion_diagnostic::Diagnostic;
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{
    self, Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Unexpected, Visitor,
};
use serde::ser::{Serialize, SerializeSeq, Serializer};

use crate::Curve;
use crate::anim::{KEY_EXPECTED, TRACK_EXPECTED};
use crate::preset::Start;
use crate::shorthand::{Behaviour, Font, Position, RectShape};
use crate::units::{Color, Size, Time};

/// The accepted form of a document value, for an error about a value of
/// the wrong JSON type: written for people, with an example.
pub(crate) trait Form {
    /// The form.
    const FORM: &str;
}

/// The top level of a scene. Its fields are, in order, the keys of
/// [`SCENE_KEYS`](crate::SCENE_KEYS); a unit test keeps the two in step.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SceneDoc {
    /// The format version; only `1` is valid. Required.
    #[serde(deserialize_with = "present")]
    pub(crate) motion: Option<Number>,
    /// The canvas size shorthand, e.g. `1080p`.
    #[serde(deserialize_with = "present")]
    pub(crate) size: Option<Shorthand<Size>>,
    /// Frames per second.
    #[serde(deserialize_with = "present")]
    pub(crate) fps: Option<Number>,
    /// The scene's length, e.g. `5s`. Required.
    #[serde(deserialize_with = "present")]
    pub(crate) dur: Option<Shorthand<Time>>,
    /// The background colour, e.g. `#0B0D12`.
    #[serde(deserialize_with = "present")]
    pub(crate) bg: Option<Shorthand<Color>>,
    /// The scene's own presets, in the order written; none when left out.
    pub(crate) presets: Object<PresetsDoc>,
    /// The layers, back to front. Required.
    #[serde(deserialize_with = "present")]
    pub(crate) layers: Option<List<LayersDoc>>,
}

impl Form for SceneDoc {
    const FORM: &str = r#"a scene object, e.g. {"motion":1,"dur":"5s","layers":[]}"#;
}

/// One layer. Its fields are, in order, the keys of
/// [`LAYER_KEYS`](crate::LAYER_KEYS); a unit test keeps the two in step.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LayerDoc {
    /// The layer's id. Required.
    #[serde(deserialize_with = "present")]
    pub(crate) id: Option<String>,
    /// The words of a text layer.
    #[serde(deserialize_with = "present")]
    pub(crate) text: Option<String>,
    /// The rectangle shorthand of a rect layer.
    #[serde(deserialize_with = "present")]
    pub(crate) rect: Option<Shorthand<RectShape>>,
    /// The font shorthand of a text layer.
    #[serde(deserialize_with = "present")]
    pub(crate) font: Option<Shorthand<Font>>,
    /// The fill colour shorthand.
    #[serde(deserialize_with = "present")]
    pub(crate) fill: Option<Shorthand<Color>>,
    /// The position shorthand.
    #[serde(deserialize_with = "present")]
    pub(crate) at: Option<Shorthand<Position>>,
    /// The entrance behaviour shorthand.
    #[serde(rename = "in", deserialize_with = "present")]
    pub(crate) enter: Option<Shorthand<Behaviour>>,
    /// Tracks by property name, in the order written; none when left out.
    pub(crate) anim: Object<AnimDoc>,
}

impl Form for LayerDoc {
    const FORM: &str = r#"a layer object, e.g. {"id":"box","rect":"100x100"}"#;
}

/// The `layers` list.
pub(crate) type LayersDoc = Vec<Object<LayerDoc>>;

impl Form for LayersDoc {
    const FORM: &str = r#"a list of layer objects, e.g. [{"id":"box","rect":"100x100"}]"#;
}

/// The `presets` object: preset bodies by name, in the order written.
pub(crate) type PresetsDoc = IndexMap<String, Object<PresetDoc>>;

impl Form for PresetsDoc {
    const FORM: &str = r#"an object of presets, e.g. {"slide":{"opacity":0,"x":"-d"}}"#;
}

/// A preset body: where each property starts, by property name, in the order
/// written. A map rather than a struct, because property names are data:
/// `crate::validate` checks them against the registry and lists its names.
pub(crate) type PresetDoc = IndexMap<String, StartDoc>;

impl Form for PresetDoc {
    const FORM: &str = r#"an object of property starts, e.g. {"opacity":0,"y":"+d"}"#;
}

/// A layer's `anim` object: tracks by property name, in the order written.
pub(crate) type AnimDoc = IndexMap<String, TrackDoc>;

impl Form for AnimDoc {
    const FORM: &str = r#"an object of property tracks, e.g. {"opacity":"0 -> 1 @0-12f"}"#;
}

/// Reads a key that may be left out but not set to `null`. Serde's
/// `Option` reads `null` as left out, which would widen the format; this
/// hands `null` to `T`, which rejects it as a wrong type with its form. The
/// struct's `#[serde(default)]` covers the key being left out.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// A shorthand string as written, parsed into a `T` by `crate::validate`.
///
/// Holding the target type lets a value that is not a string be told the
/// shorthand's own form (`T::FORM`), and lets the parse happen where the
/// rules that need the rest of the scene run.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shorthand<T> {
    /// The string as written.
    text: String,
    /// The type the string stands for.
    target: PhantomData<fn() -> T>,
}

impl<T: FromStr<Err = Diagnostic>> Shorthand<T> {
    /// Parses the string; an error has no path.
    pub(crate) fn parse(&self) -> Result<T, Diagnostic> {
        self.text.parse()
    }

    /// Parses the string written at `key`, placing an error there.
    pub(crate) fn parse_at(&self, key: &str) -> Result<T, Diagnostic> {
        self.parse().map_err(|e| e.under(key))
    }
}

impl<'de, T: Form> Deserialize<'de> for Shorthand<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(ShorthandVisitor(PhantomData))
    }
}

impl<T> Serialize for Shorthand<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

/// Reads a [`Shorthand`]: any string; any other JSON type is told `T::FORM`.
struct ShorthandVisitor<T>(PhantomData<fn() -> T>);

impl<T: Form> Visitor<'_> for ShorthandVisitor<T> {
    type Value = Shorthand<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::FORM)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Shorthand<T>, E> {
        Ok(Shorthand {
            text: v.to_owned(),
            target: PhantomData,
        })
    }
}

/// A `T` read from a JSON object only.
///
/// Serde's derived structs also accept an array of their field values in
/// order (`[1, null, null, "5s", …]`), which is not part of the scene
/// format; this wrapper offers them nothing but an object. Any other JSON
/// type is told `T::FORM`, which is also why the scene's maps are read
/// through it.
#[derive(Debug, Default)]
pub(crate) struct Object<T>(pub(crate) T);

impl<'de, T: Deserialize<'de> + Form> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

/// Reads an [`Object`]: hands the map to `T`'s own reader.
struct ObjectVisitor<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de> + Form> Visitor<'de> for ObjectVisitor<T> {
    type Value = Object<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::FORM)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Object<T>, A::Error> {
        T::deserialize(MapAccessDeserializer::new(map)).map(Object)
    }
}

/// A `T` read from a JSON array only; any other JSON type is told `T::FORM`.
#[derive(Debug)]
pub(crate) struct List<T>(pub(crate) T);

impl<'de, T: Deserialize<'de> + Form> Deserialize<'de> for List<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(ListVisitor(PhantomData))
    }
}

/// Reads a [`List`]: hands the array to `T`'s own reader.
struct ListVisitor<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de> + Form> Visitor<'de> for ListVisitor<T> {
    type Value = List<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::FORM)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<List<T>, A::Error> {
        T::deserialize(SeqAccessDeserializer::new(seq)).map(List)
    }
}

/// Where one property of a preset starts.
///
/// A newtype of the crate, so that reading a start from JSON stays an
/// implementation detail rather than a public `Deserialize` on [`Start`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StartDoc(pub(crate) Start);

impl<'de> Deserialize<'de> for StartDoc {
    /// A number, `"+d"` or `"-d"`. Any other string is the right type with
    /// the wrong content, so it is an invalid value, not a wrong type.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        const EXPECTED: &str = r#"a number, "+d" or "-d""#;
        match deserializer.deserialize_any(ScalarVisitor(EXPECTED))? {
            Scalar::Number(n) => Ok(StartDoc(Start::Value(n))),
            Scalar::Text(text) => Start::distance(&text)
                .map(StartDoc)
                .ok_or_else(|| de::Error::invalid_value(Unexpected::Str(&text), &EXPECTED)),
        }
    }
}

/// One `anim` track as written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TrackDoc {
    /// The string form, `"0 -> 1 @0-12f ease-out"`, parsed by
    /// `crate::anim::TrackLine`.
    Line(String),
    /// The key-list form.
    Keys(Vec<KeyDoc>),
}

/// One key of the key-list form: `[time, value]` or `[time, value, curve]`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeyDoc {
    /// When the key sits.
    pub(crate) time: KeyTime,
    /// The value at that time.
    pub(crate) value: f64,
    /// The curve towards the next key, if written.
    pub(crate) curve: Option<Shorthand<Curve>>,
}

/// The time of a key: a bare frame count or a time shorthand.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KeyTime {
    /// A bare number, counted in frames.
    Frames(f64),
    /// A shorthand such as `12f` or `0.4s`, parsed in `crate::validate`.
    Written(String),
}

/// A JSON number. Serde's own `f64` calls itself `f64` in a wrong-type
/// error; this says "a number", which is what the author has to write.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Number(pub(crate) f64);

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        const EXPECTED: &str = "a number";
        match deserializer.deserialize_any(ScalarVisitor(EXPECTED))? {
            Scalar::Number(n) => Ok(Number(n)),
            Scalar::Text(text) => Err(de::Error::invalid_type(Unexpected::Str(&text), &EXPECTED)),
        }
    }
}

impl Serialize for Number {
    /// Writes a whole number as an integer (`12`, not `12.0`), as an author
    /// writes it, so a track quoted back in a hint reads as it was written.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Below 2^53 every whole `f64` is exactly an `i64`.
        const EXACT: f64 = 9_007_199_254_740_992.0;
        let Number(v) = *self;
        if v.fract() == 0.0 && v.abs() < EXACT {
            serializer.serialize_i64(v as i64)
        } else {
            serializer.serialize_f64(v)
        }
    }
}

impl<'de> Deserialize<'de> for KeyTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let visitor = ScalarVisitor("a frame count, or a time such as `12f` or `0.4s`");
        Ok(match deserializer.deserialize_any(visitor)? {
            Scalar::Number(n) => KeyTime::Frames(n),
            Scalar::Text(text) => KeyTime::Written(text),
        })
    }
}

impl Serialize for KeyTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            KeyTime::Frames(frames) => Number(*frames).serialize(serializer),
            KeyTime::Written(text) => serializer.serialize_str(text),
        }
    }
}

/// A number or a string: where a scene may write either.
enum Scalar {
    /// A JSON number.
    Number(f64),
    /// A JSON string.
    Text(String),
}

/// Reads a [`Scalar`]; any other JSON type is a wrong type, told the
/// accepted form held here.
struct ScalarVisitor(&'static str);

impl Visitor<'_> for ScalarVisitor {
    type Value = Scalar;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Scalar, E> {
        Ok(Scalar::Number(v))
    }

    // JSON integers arrive as `i64` or `u64`; the scene works in `f64`.
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Scalar, E> {
        Ok(Scalar::Number(v as f64))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Scalar, E> {
        Ok(Scalar::Number(v as f64))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Scalar, E> {
        Ok(Scalar::Text(v.to_owned()))
    }
}

impl<'de> Deserialize<'de> for TrackDoc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(TrackVisitor)
    }
}

impl Serialize for TrackDoc {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            TrackDoc::Line(line) => serializer.serialize_str(line),
            TrackDoc::Keys(keys) => keys.serialize(serializer),
        }
    }
}

/// Reads a track: a string or a list of keys.
struct TrackVisitor;

impl<'de> Visitor<'de> for TrackVisitor {
    type Value = TrackDoc;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(TRACK_EXPECTED)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<TrackDoc, E> {
        Ok(TrackDoc::Line(v.to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<TrackDoc, A::Error> {
        Vec::deserialize(SeqAccessDeserializer::new(seq)).map(TrackDoc::Keys)
    }
}

impl<'de> Deserialize<'de> for KeyDoc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(KeyVisitor)
    }
}

impl Serialize for KeyDoc {
    /// `[time, value]`, or `[time, value, curve]` when a curve is written.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let len = if self.curve.is_some() { 3 } else { 2 };
        let mut seq = serializer.serialize_seq(Some(len))?;
        seq.serialize_element(&self.time)?;
        seq.serialize_element(&Number(self.value))?;
        if let Some(curve) = &self.curve {
            seq.serialize_element(curve)?;
        }
        seq.end()
    }
}

/// Reads one key: a list of two or three items.
struct KeyVisitor;

impl<'de> Visitor<'de> for KeyVisitor {
    type Value = KeyDoc;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(KEY_EXPECTED)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<KeyDoc, A::Error> {
        let time = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let Number(value) = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let curve = seq.next_element()?;
        // Count what is left, so a list that is too long reports its length.
        let mut len = 3;
        while seq.next_element::<IgnoredAny>()?.is_some() {
            len += 1;
        }
        if len > 3 {
            return Err(de::Error::invalid_length(len, &self));
        }
        Ok(KeyDoc { time, value, curve })
    }
}
