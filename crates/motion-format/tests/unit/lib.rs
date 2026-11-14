//! Unit tests for `src/lib.rs`: the wording of every error, in one snapshot.
//! The other unit tests check kinds, paths and forms, never the wording.

use std::fmt::Debug;
use std::str::FromStr;

use serde_json::Value;

use crate::anim::TrackLine;
use crate::shorthand::{Behaviour, Font, Position, RectShape};
use crate::units::{Color, Size, Time, TimeSpan};
use crate::{Curve, Diagnostic, Result, de, parse_document};

/// A section of the report: each input, then the full text of its error.
fn section<T: Debug>(title: &str, inputs: &[&str], parse: impl Fn(&str) -> Result<T>) -> String {
    let lines: Vec<String> = inputs
        .iter()
        .map(|input| format!("`{input}` → {}", parse(input).unwrap_err()))
        .collect();
    format!("## {title}\n{}", lines.join("\n"))
}

/// A section for a shorthand type, parsed on its own.
fn shorthand<T: FromStr<Err = Diagnostic> + Debug>(title: &str, inputs: &[&str]) -> String {
    section(title, inputs, str::parse::<T>)
}

/// A scene whose `layers` array holds `layers`.
fn layers(layers: &str) -> Result<crate::Document> {
    parse_document(&format!(r#"{{"motion":1,"dur":"1s","layers":[{layers}]}}"#))
}

/// A scene with one rect layer whose `anim` object holds `tracks`.
fn anim(tracks: &str) -> Result<crate::Document> {
    layers(&format!(r#"{{"id":"a","rect":"1x1","anim":{{{tracks}}}}}"#))
}

/// A scene whose `presets` value is `presets`.
fn presets(presets: &str) -> Result<crate::Document> {
    parse_document(&format!(
        r#"{{"motion":1,"dur":"1s","presets":{presets},"layers":[]}}"#
    ))
}

#[derive(Debug, serde::Deserialize)]
#[expect(dead_code, reason = "only the failure is read")]
struct Required {
    key: u8,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    On,
}

/// The deserializer's errors that no document type raises: a `Required`
/// from an object, a `Mode` from anything else.
fn serde(json: &str) -> Result<()> {
    let value: Value = serde_json::from_str(json).unwrap();
    match value {
        Value::Object(_) => de::from_value::<Required>(&value).map(drop),
        _ => de::from_value::<Mode>(&value).map(drop),
    }
}

#[test]
fn every_error_message() {
    let report = [
        section(
            "document",
            &[
                "nope",
                "[]",
                r#"[1,null,null,"1s",null,{},[]]"#,
                r#"{"motion":1,"layers":[]}"#,
                r#"{"dur":"1s","layers":[]}"#,
                r#"{"motion":1,"dur":"1s"}"#,
                r#"{"motion":2,"dur":"1s","layers":[]}"#,
                r#"{"motion":"1","dur":"1s","layers":[]}"#,
                r#"{"motion":1,"dur":null,"layers":[]}"#,
                r#"{"motion":1,"dur":5,"layers":[]}"#,
                r#"{"motion":1,"dur":"soon","layers":[]}"#,
                r#"{"motion":1,"dur":"1s","fps":null,"layers":[]}"#,
                r#"{"motion":1,"dur":"1s","fps":"a, expected b","layers":[]}"#,
                r#"{"motion":1,"dur":"1s","fps":0,"layers":[]}"#,
                r#"{"motion":1,"dur":"1s","size":null,"layers":[]}"#,
                r#"{"motion":1,"dur":"1s","size":3,"layers":[]}"#,
                r#"{"motion":1,"dur":"1s","size":"huge","layers":[]}"#,
                r#"{"motion":1,"dur":"1s","bg":0,"layers":[]}"#,
                r#"{"motion":1,"dur":"1s","colour":"x","layers":[]}"#,
                r#"{"motion":1,"dur":"1s","layers":{}}"#,
            ],
            parse_document,
        ),
        section(
            "layers",
            &[
                "1",
                r#"["a","hi"]"#,
                r#"{"rect":"1x1"}"#,
                r#"{"id":3,"rect":"1x1"}"#,
                r#"{"id":1.5,"rect":"1x1"}"#,
                r#"{"id":1e300,"rect":"1x1"}"#,
                r#"{"id":"Bad Id","rect":"1x1"}"#,
                r#"{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}"#,
                r#"{"id":"a"}"#,
                r#"{"id":"a","rect":"1x1","text":"x"}"#,
                r#"{"id":"a","text":null}"#,
                r#"{"id":"a","text":3}"#,
                r#"{"id":"a","rect":1}"#,
                r#"{"id":"a","text":"x","font":1}"#,
                r#"{"id":"a","rect":"1x1","font":"Inter 9"}"#,
                r#"{"id":"a","rect":"1x1","fill":1}"#,
                r#"{"id":"a","rect":"1x1","fill":"red"}"#,
                r#"{"id":"a","rect":"1x1","at":1}"#,
                r#"{"id":"a","rect":"1x1","in":null}"#,
                r#"{"id":"a","rect":"1x1","in":true}"#,
                r#"{"id":"a","rect":"1x1","in":"zoom"}"#,
                r#"{"id":"a","rect":"1x1","anim":null}"#,
                r#"{"id":"a","rect":"1x1","anim":[]}"#,
                r#"{"id":"a","rect":"1x1","x":1}"#,
                r#"{"id":"a","rect":"1x1","position":[1,2]}"#,
                r#"{"id":"a","rect":"1x1","rotate":"x"}"#,
                r#"{"id":"a","rect":"1x1","scale":[1,2,3]}"#,
                r#"{"id":"a","rect":"1x1","opacity":2}"#,
                r#"{"id":"a","rect":"1x1","parent":"b"}"#,
                r#"{"id":"a","rect":"1x1","parent":"a"}"#,
                r#"{"id":"a","rect":"1x1","parent":"b"},{"id":"b","rect":"1x1","parent":"a"}"#,
                r#"{"id":"a","rect":"1x1","parent":3}"#,
                r#"{"id":"a","rect":"1x1","span":"5f"}"#,
                r#"{"id":"a","rect":"1x1","span":"5-1f"}"#,
                r#"{"id":"a","rect":"1x1","offset":"-1f"}"#,
                r#"{"id":"a","rect":"1x1","stretch":0}"#,
                r#"{"id":"a","rect":"10x10","in":"fade-up","anim":{"opacity":"0 -> 1 @0-5f"}}"#,
                r#"{"id":"a","rect":"10x10","in":"fade-up","anim":{"position":"0,0 -> 1,1 @0-5f"}}"#,
            ],
            layers,
        ),
        section(
            "anim",
            &[
                r#""x":3"#,
                r#""glow":"0 -> 1 @0-5f""#,
                r#""x":"0 1 @0-5f""#,
                r#""x":"0 -> 1 @5-0f""#,
                r#""x":"inf -> 1 @0-5f""#,
                r#""x":"a -> 1 @0-5f""#,
                r#""scale":"1,2,3 -> 1 @0-5f""#,
                r#""position":"1 -> 2 @0-5f""#,
                r#""fill":"0 -> 1 @0-5f""#,
                r#""x":"0 -> 1 @0-5f","position":"0,0 -> 1,1 @0-5f""#,
                r#""position":[[0,[1,2],["linear","linear"]]]"#,
                r#""scale":[[0,[1,2],["linear"]]]"#,
                r#""scale":[[0,[1,2],["linear","bouncy"]]]"#,
                r#""scale":[[0,[1,2],3]]"#,
                r#""x":[{"t":0,"v":1,"path":[[0,0],[0,0]]}]"#,
                r#""position":[{"t":0,"v":[1,2],"path":[[0,0]]}]"#,
                r#""position":[{"t":0,"v":[1,2],"bend":1}]"#,
                r#""position":[{"v":[1,2]}]"#,
                r#""x":[[0,1,"speed(0,2,0,0)"]]"#,
                r#""opacity":"0 -> 2 @0-5f""#,
                r#""x":[]"#,
                r#""x":[5]"#,
                r#""x":[[0]]"#,
                r#""x":[[0, 1, "linear", 4]]"#,
                r#""x":[[true, 0]]"#,
                r#""x":[["12", 0]]"#,
                r#""x":[[-1, 0]]"#,
                r#""x":[[0, "a"]]"#,
                r#""x":[[0, 1, 3]]"#,
                r#""x":[[0, 1, null]]"#,
                r#""x":[[0, 1, "nope"]]"#,
                r#""x":[[5, 0], [2, 1]]"#,
                r#""opacity":[[0, 0], [5, 1], [9, 2]]"#,
            ],
            anim,
        ),
        section(
            "presets",
            &[
                "null",
                "[]",
                r#"{"a":3}"#,
                r#"{"Bad":{"x":1}}"#,
                r#"{"x":{}}"#,
                r#"{"a":{"zoom":1}}"#,
                r#"{"a":{"x":true}}"#,
                r#"{"a":{"x":"left"}}"#,
                r#"{"a":{"x":"l, expected q"}}"#,
                r#"{"a":{"opacity":2}}"#,
                r#"{"dim":{"opacity":"-d"}}"#,
            ],
            presets,
        ),
        section(
            "unknown preset with scene presets",
            &[r#"{"slide":{"x":1},"fade":{"y":1}}"#],
            |own| {
                parse_document(&format!(
                    r#"{{"motion":1,"dur":"1s","presets":{own},"layers":[{{"id":"a","rect":"1x1","in":"zoom"}}]}}"#
                ))
            },
        ),
        shorthand::<Time>("time", &["12"]),
        shorthand::<Color>("colour", &["red"]),
        shorthand::<Size>("size", &["8k", "640x", "640x360 ", "1x2x3"]),
        shorthand::<Curve>(
            "curve",
            &[
                "foo",
                "cubic(.2,0,0,1",
                "cubic(0,0,1,1)x",
                "cubic(.2,0,1)",
                "cubic(1,2))",
                "cubic(1.2,0,0,1)",
            ],
        ),
        shorthand::<RectShape>("rect", &["300"]),
        shorthand::<Position>("position", &["260"]),
        shorthand::<Font>(
            "font",
            &[
                "Inter",
                "96 700",
                "1 2 3",
                "Inter 0",
                "Inter 96 700 750",
                "Baloo 2 120",
            ],
        ),
        shorthand::<Behaviour>(
            "behaviour",
            &[
                "",
                "Fade-Up",
                "fade 12x",
                "fade -3f",
                "fade 1pxpx",
                "fade @12",
                "fade bounce",
                "fade 12f 10f",
                "fade ease-in cubic(.2,0,0,1)",
            ],
        ),
        shorthand::<TrackLine>(
            "track",
            &[
                "",
                "0 1 @0-5f",
                "0 -> 1",
                "0 -> 1 2 @0-5f",
                "0 -> 1 @0-5f bouncy",
                "0 -> 1 @0-5f ease ease",
            ],
        ),
        section("range", &["@x-5f", "@05f", "@0-5"], TimeSpan::from_range),
        shorthand::<TimeSpan>("span", &["x-5f", "05f", "0-5", "@0-5f"]),
        section("serde", &["{}", r#""dim""#], serde),
    ];
    insta::assert_snapshot!(report.join("\n\n"));
}
