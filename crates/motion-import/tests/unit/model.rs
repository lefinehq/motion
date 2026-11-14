//! Unit tests for `src/model.rs`.

use serde::Serialize;
use strum::IntoEnumIterator;

use super::*;

/// The name `--json` gives `value`: a string, or the object of a variant with data.
fn json_name(value: &impl Serialize) -> String {
    match serde_json::to_value(value).unwrap() {
        serde_json::Value::String(name) => name,
        other => other.to_string(),
    }
}

/// Every variant of `E` prints as `--json` names it.
fn prints_as_json<E: IntoEnumIterator + Serialize + std::fmt::Display>() {
    for variant in E::iter() {
        assert_eq!(variant.to_string(), json_name(&variant));
    }
}

#[test]
fn names_are_the_json_names() {
    prints_as_json::<Reason>();
    prints_as_json::<LayerKind>();
    prints_as_json::<BlendMode>();
    prints_as_json::<TrackMatte>();
    prints_as_json::<Interpolation>();
    assert_eq!(LayerKind::from(6).to_string(), r#"{"other":6}"#);
    assert_eq!(ItemKind::Folder.to_string(), "folder");
}
