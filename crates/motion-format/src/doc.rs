//! The scene document as JSON writes it, read with serde.
//!
//! Part of step 1 (Read), between the JSON text and `crate::validate`. These
//! types mirror the JSON: each key has a field, and unknown keys are
//! rejected. Serde reports only structural errors here (a wrong JSON type, an
//! unknown or missing key); shorthand strings stay `String` and are parsed,
//! with every rule that needs more than one value, in `crate::validate`.
//!
//! The track and key forms have hand-written visitors rather than
//! `#[serde(untagged)]`, so a wrong value names the forms it could have been
//! instead of "did not match any variant".

use std::fmt;
use std::marker::PhantomData;

use indexmap::IndexMap;
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{
    self, Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Unexpected, Visitor,
};

use crate::anim::{KEY_EXPECTED, TRACK_EXPECTED};
use crate::preset::Start;

/// The top level of a scene. Its fields are, in order, the keys of
/// [`SCENE_KEYS`](crate::SCENE_KEYS); a unit test keeps the two in step.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SceneDoc {
    /// The format version; only `1` is valid.
    pub(crate) motion: Number,
    /// The canvas size shorthand, e.g. `1080p`.
    pub(crate) size: Option<String>,
    /// Frames per second.
    pub(crate) fps: Option<Number>,
    /// The scene's length, e.g. `5s`.
    pub(crate) dur: String,
    /// The background colour, e.g. `#0B0D12`.
    pub(crate) bg: Option<String>,
    /// The scene's own presets, in the order written.
    #[serde(default)]
    pub(crate) presets: IndexMap<String, PresetDoc>,
    /// The layers, back to front.
    pub(crate) layers: Vec<Object<LayerDoc>>,
}

impl ObjectForm for SceneDoc {
    const FORM: &str = r#"a scene object, e.g. {"motion":1,"dur":"5s","layers":[]}"#;
}

/// One layer. Its fields are, in order, the keys of
/// [`LAYER_KEYS`](crate::LAYER_KEYS); a unit test keeps the two in step.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LayerDoc {
    /// The layer's id.
    pub(crate) id: String,
    /// The words of a text layer.
    pub(crate) text: Option<String>,
    /// The rectangle shorthand of a rect layer.
    pub(crate) rect: Option<String>,
    /// The font shorthand of a text layer.
    pub(crate) font: Option<String>,
    /// The fill colour shorthand.
    pub(crate) fill: Option<String>,
    /// The position shorthand.
    pub(crate) at: Option<String>,
    /// The entrance behaviour shorthand.
    #[serde(rename = "in")]
    pub(crate) enter: Option<String>,
    /// Tracks by property name, in the order written.
    #[serde(default)]
    pub(crate) anim: IndexMap<String, TrackDoc>,
}

impl ObjectForm for LayerDoc {
    const FORM: &str = r#"a layer object, e.g. {"id":"box","rect":"100x100"}"#;
}

/// A document struct read from a JSON object only.
///
/// Serde's derived structs also accept an array of their field values in
/// order (`[1, null, null, "5s", …]`), which is not part of the scene
/// format. This wrapper offers them nothing but an object, and tells any
/// other JSON type the struct's [`ObjectForm::FORM`].
#[derive(Debug)]
pub(crate) struct Object<T>(pub(crate) T);

/// The accepted form of a document struct, for a value that is not an
/// object.
pub(crate) trait ObjectForm {
    /// The form, with an example.
    const FORM: &str;
}

impl<'de, T: Deserialize<'de> + ObjectForm> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

/// Reads an [`Object`]: hands the map to `T`'s derived reader.
struct ObjectVisitor<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de> + ObjectForm> Visitor<'de> for ObjectVisitor<T> {
    type Value = Object<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::FORM)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Object<T>, A::Error> {
        T::deserialize(MapAccessDeserializer::new(map)).map(Object)
    }
}

/// A preset body: where each property starts, by property name, in the order
/// written. A map rather than a struct, because property names are data:
/// `crate::validate` checks them against the registry and lists its names.
pub(crate) type PresetDoc = IndexMap<String, Start>;

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
    /// The curve shorthand towards the next key, if written.
    pub(crate) curve: Option<String>,
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

impl<'de> Deserialize<'de> for Start {
    /// A number, `"+d"` or `"-d"`. Any other string is the right type with
    /// the wrong content, so it is an invalid value, not a wrong type.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        const EXPECTED: &str = r#"a number, "+d" or "-d""#;
        match deserializer.deserialize_any(ScalarVisitor(EXPECTED))? {
            Scalar::Number(n) => Ok(Start::Value(n)),
            Scalar::Text(text) => Start::distance(&text)
                .ok_or_else(|| de::Error::invalid_value(Unexpected::Str(&text), &EXPECTED)),
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
