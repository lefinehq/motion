//! The scene document as JSON writes it, read with serde. Shorthand strings
//! stay unparsed here; `crate::validate` parses them and checks the rules,
//! including the keys a scene requires, so every key may be left out here.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;

use crate::Diagnostic;
use indexmap::IndexMap;
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{
    self, Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Unexpected, Visitor,
};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

use crate::Curve;
use crate::anim::{EASING_EXPECTED, KEY_EXPECTED, PATH_EXPECTED, TRACK_EXPECTED};
use crate::preset::Start;
use crate::shorthand::{Behaviour, Font, Matte, Position, RectShape};
use crate::units::{Color, SignedTime, Size, Time, TimeSpan};

/// The accepted form of a document value, told to a value of the wrong JSON
/// type instead of serde's "a string" or "a map".
pub(crate) trait Form {
    /// The form, written for people, with an example.
    const FORM: &str;
}

/// The top level of a scene; its fields follow [`SCENE_KEYS`](crate::SCENE_KEYS).
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SceneDoc {
    pub(crate) motion: Present<Number>,
    pub(crate) size: Present<Shorthand<Size>>,
    pub(crate) fps: Present<Number>,
    pub(crate) dur: Present<Shorthand<Time>>,
    pub(crate) bg: Present<Shorthand<Color>>,
    pub(crate) presets: Object<PresetsDoc>,
    pub(crate) fonts: Present<List<FontsDoc>>,
    pub(crate) comps: Object<CompsDoc>,
    pub(crate) layers: Present<List<LayersDoc>>,
}

impl Form for SceneDoc {
    const FORM: &str = r#"a scene object, e.g. {"motion":1,"dur":"5s","layers":[]}"#;
}

/// One layer; its fields follow [`LAYER_KEYS`](crate::LAYER_KEYS), and any
/// other key is a property's resting value, which `crate::validate` checks
/// against the registry (serde cannot deny unknown fields next to `flatten`).
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct LayerDoc {
    pub(crate) id: Present<String>,
    pub(crate) text: Present<String>,
    pub(crate) rect: Present<Shorthand<RectShape>>,
    pub(crate) shapes: Present<List<ItemsDoc>>,
    pub(crate) comp: Present<String>,
    pub(crate) pivot: Present<bool>,
    pub(crate) font: Present<Shorthand<Font>>,
    pub(crate) fill: Present<serde_json::Value>,
    pub(crate) style: Object<StyleDoc>,
    pub(crate) runs: Present<List<StylesDoc>>,
    pub(crate) paragraphs: Present<List<StylesDoc>>,
    #[serde(rename = "box")]
    pub(crate) frame: Present<ValueDoc>,
    pub(crate) animate: Present<List<ItemsDoc>>,
    pub(crate) at: Present<Shorthand<Position>>,
    #[serde(rename = "in")]
    pub(crate) enter: Present<Shorthand<Behaviour>>,
    pub(crate) anim: Object<AnimDoc>,
    pub(crate) parent: Present<String>,
    pub(crate) span: Present<Shorthand<TimeSpan>>,
    pub(crate) offset: Present<Shorthand<SignedTime>>,
    pub(crate) stretch: Present<Number>,
    pub(crate) fx: Present<List<ItemsDoc>>,
    pub(crate) hidden: Present<bool>,
    pub(crate) matte: Present<Shorthand<Matte>>,
    /// Every other key, kept as JSON until its name is checked, so an unknown
    /// key is reported as unknown whatever its value.
    #[serde(flatten)]
    pub(crate) values: IndexMap<String, serde_json::Value>,
}

impl Form for LayerDoc {
    const FORM: &str = r#"a layer object, e.g. {"id":"box","rect":"100x100"}"#;
}

/// One shape item or effect. Its kind's name is one of the flattened keys,
/// holding the primary value; the others are settings and resting values,
/// which `crate::validate` checks against the kind.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct ItemDoc {
    pub(crate) id: Present<String>,
    pub(crate) hidden: Present<bool>,
    pub(crate) anim: Object<AnimDoc>,
    #[serde(flatten)]
    pub(crate) values: IndexMap<String, serde_json::Value>,
}

impl Form for ItemDoc {
    const FORM: &str = r##"an item object keyed by its kind, e.g. {"rect":[300,100],"round":12} or {"fill":"#FFE000"}"##;
}

/// A list of items: a layer's `shapes` or `fx`, or a group's items.
pub(crate) type ItemsDoc = Vec<Object<ItemDoc>>;

impl Form for ItemsDoc {
    const FORM: &str = r##"a list of item objects, e.g. [{"rect":[300,100]},{"fill":"#FFE000"}]"##;
}

/// A text style object: style keys by name, which `crate::validate` checks
/// against [`STYLE_KEYS`](crate::text::STYLE_KEYS).
pub(crate) type StyleDoc = IndexMap<String, serde_json::Value>;

impl Form for StyleDoc {
    const FORM: &str = r#"an object of text style keys, e.g. {"tracking":20,"align":"center"}"#;
}

/// A list of style objects: a text layer's `runs` or `paragraphs`.
pub(crate) type StylesDoc = Vec<Object<StyleDoc>>;

impl Form for StylesDoc {
    const FORM: &str = r##"a list of style objects, e.g. [{"range":[0,4],"fill":"#FF0000"}]"##;
}

/// The `fonts` list: font files and folders, relative to the scene file.
pub(crate) type FontsDoc = Vec<String>;

impl Form for FontsDoc {
    const FORM: &str = r#"a list of font files and folders, e.g. ["fonts/lato"]"#;
}

/// The `comps` object: compositions by id.
pub(crate) type CompsDoc = IndexMap<String, Object<CompDoc>>;

impl Form for CompsDoc {
    const FORM: &str =
        r#"an object of compositions, e.g. {"card":{"size":"600x400","dur":"5s","layers":[]}}"#;
}

/// One composition: a canvas of its own with its own layers, which a
/// `comp` layer draws.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CompDoc {
    pub(crate) size: Present<Shorthand<Size>>,
    pub(crate) fps: Present<Number>,
    pub(crate) dur: Present<Shorthand<Time>>,
    pub(crate) bg: Present<Shorthand<Color>>,
    pub(crate) layers: Present<List<LayersDoc>>,
}

impl Form for CompDoc {
    const FORM: &str = r#"a composition object, e.g. {"size":"600x400","dur":"5s","layers":[]}"#;
}

/// The `layers` list.
pub(crate) type LayersDoc = Vec<Object<LayerDoc>>;

impl Form for LayersDoc {
    const FORM: &str = r#"a list of layer objects, e.g. [{"id":"box","rect":"100x100"}]"#;
}

/// The `presets` object: preset bodies by name.
pub(crate) type PresetsDoc = IndexMap<String, Object<PresetDoc>>;

impl Form for PresetsDoc {
    const FORM: &str = r#"an object of presets, e.g. {"slide":{"opacity":0,"x":"-d"}}"#;
}

/// A preset body: starts by property name. A map, because property names are
/// data that `crate::validate` checks against the registry.
pub(crate) type PresetDoc = IndexMap<String, StartDoc>;

impl Form for PresetDoc {
    const FORM: &str = r#"an object of property starts, e.g. {"opacity":0,"y":"+d"}"#;
}

/// A layer's `anim` object: tracks by property name.
pub(crate) type AnimDoc = IndexMap<String, TrackDoc>;

impl Form for AnimDoc {
    const FORM: &str = r#"an object of property tracks, e.g. {"opacity":"0 -> 1 @0-12f"}"#;
}

/// A key that may be left out but not set to `null`.
///
/// `Option` would read `null` as left out; this hands `null` to `T`, which
/// rejects it with its form. `#[serde(default)]` covers a key left out.
#[derive(Debug)]
pub(crate) struct Present<T>(Option<T>);

impl<T> Default for Present<T> {
    fn default() -> Self {
        Present(None)
    }
}

impl<T> Deref for Present<T> {
    type Target = Option<T>;

    fn deref(&self) -> &Option<T> {
        &self.0
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Present<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(|value| Present(Some(value)))
    }
}

/// A shorthand string as written, parsed into a `T` by `crate::validate`.
///
/// The type parameter gives a value that is not a string `T::FORM`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shorthand<T> {
    text: String,
    target: PhantomData<fn() -> T>,
}

impl<T> Shorthand<T> {
    fn new(text: String) -> Self {
        Shorthand {
            text,
            target: PhantomData,
        }
    }
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

struct ShorthandVisitor<T>(PhantomData<fn() -> T>);

impl<T: Form> Visitor<'_> for ShorthandVisitor<T> {
    type Value = Shorthand<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::FORM)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Shorthand<T>, E> {
        Ok(Shorthand::new(v.to_owned()))
    }
}

/// A `T` read from a JSON object only, told `T::FORM` otherwise.
///
/// Derived structs also accept an array of their fields in order, which is
/// not part of the format.
#[derive(Debug, Default)]
pub(crate) struct Object<T>(pub(crate) T);

impl<'de, T: Deserialize<'de> + Form> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

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

/// A `T` read from a JSON array only, told `T::FORM` otherwise.
#[derive(Debug)]
pub(crate) struct List<T>(pub(crate) T);

impl<'de, T: Deserialize<'de> + Form> Deserialize<'de> for List<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(ListVisitor(PhantomData))
    }
}

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

/// Where one property of a preset starts; a newtype so that [`Start`] has no
/// public `Deserialize`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StartDoc(pub(crate) Start);

impl<'de> Deserialize<'de> for StartDoc {
    /// A number, `"+d"` or `"-d"`; another string is an invalid value, not a wrong type.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let expected = Start::form();
        match deserializer.deserialize_any(ScalarVisitor(expected))? {
            Scalar::Number(n) => Ok(StartDoc(Start::Value(n))),
            Scalar::Text(text) => Start::distance(&text)
                .map(StartDoc)
                .ok_or_else(|| de::Error::invalid_value(Unexpected::Str(&text), &expected)),
        }
    }
}

/// One `anim` track as written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TrackDoc {
    /// The string form, `"0 -> 1 @0-12f ease-out"`.
    Line(String),
    /// The key-list form.
    Keys(Vec<KeyDoc>),
}

/// One key of the key-list form: `[time, value]`, `[time, value, curve]`,
/// or the object form, which can also bend a spatial path.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyDoc {
    #[serde(rename = "t")]
    pub(crate) time: KeyTime,
    #[serde(rename = "v")]
    pub(crate) value: ValueDoc,
    #[serde(default)]
    pub(crate) curve: Option<EasingDoc>,
    #[serde(default)]
    pub(crate) path: Option<PathDoc>,
    /// Written as an object, so errors name its keys rather than positions.
    #[serde(skip)]
    pub(crate) object: bool,
}

impl KeyDoc {
    /// Where the value sits in the key as written: `v` or `[1]`.
    pub(crate) fn value_path(&self) -> &'static str {
        if self.object { "v" } else { "[1]" }
    }

    /// Where the curve sits in the key as written: `curve` or `[2]`.
    pub(crate) fn curve_path(&self) -> &'static str {
        if self.object { "curve" } else { "[2]" }
    }
}

/// A value as JSON writes it: a number, a list of numbers or a string. Its
/// type comes from the property, so `crate::validate` reads it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ValueDoc {
    /// A number, or the numbers of a list.
    Numbers(Vec<f64>),
    /// The string form, such as `"#FFE000"` or `"540,960"`.
    Text(String),
}

/// A key's curve: one for the whole value, or one per component.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EasingDoc {
    /// One curve.
    One(Shorthand<Curve>),
    /// One curve per component.
    Each(Vec<Shorthand<Curve>>),
}

/// The tangents of a spatial key, `[[out-x, out-y], [in-x, in-y]]`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct PathDoc(pub(crate) [[f64; 2]; 2]);

impl Form for PathDoc {
    const FORM: &str = PATH_EXPECTED;
}

/// The time of a key: a bare frame count or a time shorthand.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KeyTime {
    /// A bare number, counted in frames.
    Frames(f64),
    /// A shorthand such as `12f` or `0.4s`; it may be negative.
    Written(Shorthand<SignedTime>),
}

/// A JSON number, which a wrong type is told is "a number" rather than `f64`.
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
    /// Writes a whole number as an integer (`12`, not `12.0`), as an author writes it.
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
        let visitor = ScalarVisitor("a frame count, or a time such as `12f` or `-0.4s`");
        Ok(match deserializer.deserialize_any(visitor)? {
            Scalar::Number(n) => KeyTime::Frames(n),
            Scalar::Text(text) => KeyTime::Written(Shorthand::new(text)),
        })
    }
}

impl Serialize for KeyTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            KeyTime::Frames(frames) => Number(*frames).serialize(serializer),
            KeyTime::Written(time) => time.serialize(serializer),
        }
    }
}

/// A number or a string, where a scene may write either.
enum Scalar {
    Number(f64),
    Text(String),
}

/// Reads a [`Scalar`]; any other JSON type is told the form it holds.
struct ScalarVisitor(&'static str);

impl Visitor<'_> for ScalarVisitor {
    type Value = Scalar;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Scalar, E> {
        Ok(Scalar::Number(v))
    }

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
    // A visitor rather than `#[serde(untagged)]`, so a wrong value is told
    // both forms instead of "did not match any variant".
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
        let keys: Vec<KeyForm> = Vec::deserialize(SeqAccessDeserializer::new(seq))?;
        Ok(TrackDoc::Keys(
            keys.into_iter().map(|KeyForm(k)| k).collect(),
        ))
    }
}

/// The array or object form of a key.
pub(crate) struct KeyForm(pub(crate) KeyDoc);

impl<'de> Deserialize<'de> for KeyForm {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(KeyVisitor).map(KeyForm)
    }
}

impl Serialize for KeyDoc {
    /// The array form, or the object form when the key bends a path.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(path) = &self.path {
            let mut map = serializer.serialize_map(None)?;
            map.serialize_entry("t", &self.time)?;
            map.serialize_entry("v", &self.value)?;
            if let Some(curve) = &self.curve {
                map.serialize_entry("curve", curve)?;
            }
            map.serialize_entry("path", path)?;
            return map.end();
        }
        let len = if self.curve.is_some() { 3 } else { 2 };
        let mut seq = serializer.serialize_seq(Some(len))?;
        seq.serialize_element(&self.time)?;
        seq.serialize_element(&self.value)?;
        if let Some(curve) = &self.curve {
            seq.serialize_element(curve)?;
        }
        seq.end()
    }
}

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
        let value = seq
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
        Ok(KeyDoc {
            time,
            value,
            curve,
            path: None,
            object: false,
        })
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<KeyDoc, A::Error> {
        let key = KeyDoc::deserialize(MapAccessDeserializer::new(map))?;
        Ok(KeyDoc {
            object: true,
            ..key
        })
    }
}

impl<'de> Deserialize<'de> for ValueDoc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

impl Serialize for ValueDoc {
    /// One number bare, several as a list, a string as written.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ValueDoc::Numbers(numbers) if numbers.len() == 1 => {
                Number(numbers[0]).serialize(serializer)
            }
            ValueDoc::Numbers(numbers) => {
                let mut seq = serializer.serialize_seq(Some(numbers.len()))?;
                for n in numbers {
                    seq.serialize_element(&Number(*n))?;
                }
                seq.end()
            }
            ValueDoc::Text(text) => serializer.serialize_str(text),
        }
    }
}

/// The accepted forms of a value whose property is not known yet.
const VALUE_EXPECTED: &str =
    r##"a number, a list of numbers such as [540, 960], or a string such as "#FFE000""##;

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = ValueDoc;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(VALUE_EXPECTED)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<ValueDoc, E> {
        Ok(ValueDoc::Numbers(vec![v]))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<ValueDoc, E> {
        Ok(ValueDoc::Numbers(vec![v as f64]))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<ValueDoc, E> {
        Ok(ValueDoc::Numbers(vec![v as f64]))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<ValueDoc, E> {
        Ok(ValueDoc::Text(v.to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ValueDoc, A::Error> {
        let mut numbers = Vec::new();
        while let Some(Number(n)) = seq.next_element()? {
            numbers.push(n);
        }
        Ok(ValueDoc::Numbers(numbers))
    }
}

impl<'de> Deserialize<'de> for EasingDoc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(EasingVisitor)
    }
}

impl Serialize for EasingDoc {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            EasingDoc::One(curve) => curve.serialize(serializer),
            EasingDoc::Each(curves) => curves.serialize(serializer),
        }
    }
}

struct EasingVisitor;

impl<'de> Visitor<'de> for EasingVisitor {
    type Value = EasingDoc;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(EASING_EXPECTED)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<EasingDoc, E> {
        Ok(EasingDoc::One(Shorthand::new(v.to_owned())))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<EasingDoc, A::Error> {
        Vec::deserialize(SeqAccessDeserializer::new(seq)).map(EasingDoc::Each)
    }
}
