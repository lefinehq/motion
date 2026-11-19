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
    parse_document(&format!(
        r#"{{"nakkash":1,"dur":"1s","layers":[{layers}]}}"#
    ))
}

/// A scene with one shape layer whose `shapes` list holds `items`.
fn shapes(items: &str) -> Result<crate::Document> {
    layers(&format!(r#"{{"id":"a","shapes":[{items}]}}"#))
}

/// A scene with one rect layer whose `fx` list holds `effects`.
fn fx(effects: &str) -> Result<crate::Document> {
    layers(&format!(r#"{{"id":"a","rect":"1x1","fx":[{effects}]}}"#))
}

/// A scene whose `comps` object holds `comps`, drawn by layer `a` as `card`.
fn comps(comps: &str) -> Result<crate::Document> {
    parse_document(&format!(
        r#"{{"nakkash":1,"dur":"1s","comps":{{{comps}}},"layers":[{{"id":"a","comp":"card"}}]}}"#
    ))
}

/// A scene with one rect layer whose `anim` object holds `tracks`.
fn anim(tracks: &str) -> Result<crate::Document> {
    layers(&format!(r#"{{"id":"a","rect":"1x1","anim":{{{tracks}}}}}"#))
}

/// A scene whose `presets` value is `presets`.
fn presets(presets: &str) -> Result<crate::Document> {
    parse_document(&format!(
        r#"{{"nakkash":1,"dur":"1s","presets":{presets},"layers":[]}}"#
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
                r#"{"nakkash":1,"layers":[]}"#,
                r#"{"dur":"1s","layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s"}"#,
                r#"{"nakkash":2,"dur":"1s","layers":[]}"#,
                r#"{"nakkash":"1","dur":"1s","layers":[]}"#,
                r#"{"nakkash":1,"dur":null,"layers":[]}"#,
                r#"{"nakkash":1,"dur":5,"layers":[]}"#,
                r#"{"nakkash":1,"dur":"soon","layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","fps":null,"layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","fps":"a, expected b","layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","fps":0,"layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","size":null,"layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","size":3,"layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","size":"huge","layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","bg":0,"layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","colour":"x","layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","layers":{}}"#,
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
                r#"{"id":"scene","rect":"1x1"}"#,
                r#"{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}"#,
                r#"{"id":"a"}"#,
                r#"{"id":"a","fx":[]}"#,
                r##"{"id":"a","fx":[{"fill":"#FFFFFF"}],"fill":"#000000"}"##,
                r##"{"id":"a","fx":[{"fill":"#FFFFFF"}],"box":[1,1]}"##,
                r#"{"id":"a","rect":"1x1","adjust":true}"#,
                r#"{"id":"a","rect":"1x1","adjust":1,"fx":[]}"#,
                r#"{"id":"a","rect":"1x1","collapse":true}"#,
                r#"{"id":"a","audio":""}"#,
                r#"{"id":"a","audio":"x.mp3","box":[1,1]}"#,
                r##"{"id":"a","audio":"x.mp3","fx":[{"fill":"#FFFFFF"}]}"##,
                r#"{"id":"s","audio":"x.mp3"},{"id":"a","rect":"1x1","matte":"s"}"#,
                r#"{"id":"m","rect":"1x1"},{"id":"a","audio":"x.mp3","matte":"m"}"#,
                r#"{"id":"a","rect":"1x1","volume":0.5}"#,
                r#"{"id":"a","rect":"1x1","anim":{"volume":"0 -> 1 @0-5f"}}"#,
                r#"{"id":"a","audio":"x.mp3","volume":5}"#,
                r#"{"id":"a","rect":"1x1","remap":0.5}"#,
                r#"{"id":"a","audio":"x.mp3","anim":{"remap":"0 -> 1 @0-5f"}}"#,
                r##"{"id":"a","fx":[{"fill":"#FFFFFF"}],"collapse":true}"##,
                r#"{"id":"a","rect":"1x1","text":"x"}"#,
                r#"{"id":"a","text":null}"#,
                r#"{"id":"a","text":3}"#,
                r#"{"id":"a","rect":1}"#,
                r#"{"id":"a","text":"x","font":1}"#,
                r#"{"id":"a","rect":"1x1","font":"Inter 9"}"#,
                r#"{"id":"a","image":"a.png"}"#,
                r#"{"id":"a","image":"","box":[1,1]}"#,
                r#"{"id":"a","image":"a.png","box":[0,1]}"#,
                r#"{"id":"a","image":3,"box":[1,1]}"#,
                r#"{"id":"a","image":null}"#,
                r#"{"id":"a","image":"a.png","box":[1,1],"fit":"stretch"}"#,
                r#"{"id":"a","video":"a.mov","box":[1,1],"fit":3}"#,
                r#"{"id":"a","rect":"1x1","fit":"cover"}"#,
                r##"{"id":"a","fx":[{"fill":"#FFFFFF"}],"fit":"cover"}"##,
                r#"{"id":"a","rect":"1x1","fill":1}"#,
                r#"{"id":"a","rect":"1x1","fill":"red"}"#,
                r#"{"id":"a","rect":"1x1","at":1}"#,
                r#"{"id":"a","rect":"1x1","in":null}"#,
                r#"{"id":"a","rect":"1x1","in":true}"#,
                r#"{"id":"a","rect":"1x1","in":"zoom"}"#,
                r#"{"id":"a","rect":"1x1","anim":null}"#,
                r#"{"id":"a","rect":"1x1","anim":[]}"#,
                r#"{"id":"a","rect":"1x1","x":1}"#,
                r#"{"id":"a","rect":"1x1","z":10}"#,
                r#"{"id":"a","rect":"1x1","anim":{"rotate-y":"0 -> 90 @0-5f"}}"#,
                r#"{"id":"a","rect":"1x1","3d":true,"zoom":900}"#,
                r#"{"id":"a","camera":false}"#,
                r#"{"id":"a","camera":true,"3d":false}"#,
                r#"{"id":"a","rect":"1x1","3d":true,"orient":"0,90"}"#,
                r#"{"id":"a","rect":"1x1","at":"1 2","position":[1,2]}"#,
                r#"{"id":"a","rect":"1x1","rotate":"x"}"#,
                r#"{"id":"a","rect":"1x1","scale":[1,2,3]}"#,
                r#"{"id":"a","rect":"1x1","opacity":2}"#,
                r#"{"id":"a","rect":"1x1","parent":"b"}"#,
                r#"{"id":"a","rect":"1x1","parent":"a"}"#,
                r#"{"id":"a","rect":"1x1","parent":"b"},{"id":"b","rect":"1x1","parent":"a"}"#,
                r#"{"id":"a","rect":"1x1","parent":3}"#,
                r#"{"id":"a","rect":"1x1","matte":"b"}"#,
                r#"{"id":"a","rect":"1x1","matte":"a"}"#,
                r#"{"id":"a","rect":"1x1","matte":"b"},{"id":"b","rect":"1x1","matte":"a luma"}"#,
                r#"{"id":"a","rect":"1x1","matte":"b sideways"},{"id":"b","rect":"1x1"}"#,
                r#"{"id":"a","rect":"1x1","matte":"b alpha extra"},{"id":"b","rect":"1x1"}"#,
                r#"{"id":"a","rect":"1x1","matte":true}"#,
                r#"{"id":"a","rect":"1x1","fx":[{"displacement-map":5,"map":"b"}]}"#,
                r#"{"id":"a","rect":"1x1","fx":[{"displacement-map":5,"map":"a"}]}"#,
                r#"{"id":"a","rect":"1x1","fx":[{"displacement-map":5,"map":"b"}]},{"id":"b","rect":"1x1","matte":"a"}"#,
                r#"{"id":"a","rect":"1x1","fx":[{"displacement-map":5,"map":3}]}"#,
                r#"{"id":"a","rect":"1x1","fx":[{"audio-spectrum":5,"audio":"b"}]},{"id":"b","rect":"1x1"}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","fx":[{"displacement-map":5,"map":"m"}]}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","fx":[{"audio-waveform":5,"audio":"m","channel":"middle"}]}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","anim":{"scale":"1 -> 2 -> 3 @level m"}}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","anim":{"scale":"1 -> 2 @level"}}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","anim":{"scale":"1 -> 2 @level m 5-2Hz"}}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","anim":{"scale":"1 -> 2 @level m smooth"}}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","anim":{"scale":"1 -> 2 @level m linear extra"}}"#,
                r#"{"id":"b","rect":"1x1"},{"id":"a","rect":"1x1","anim":{"scale":"1 -> 2 @level b"}}"#,
                r#"{"id":"a","rect":"1x1","anim":{"scale":"1 -> 2 @level zz"}}"#,
                r#"{"id":"m","audio":"m.wav","anim":{"volume":"0 -> 1 @level m"}}"#,
                r##"{"id":"a","rect":"1x1","fx":[{"fill":"#FF0000","anim":{"opacity":"0 -> 1 @level q"}}]}"##,
                r##"{"id":"a","rect":"1x1","fx":[{"fill":"#FF0000","map":"a"}]}"##,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","fx":[{"audio-spectrum":5,"audio":"m","along":"ring"}]}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","mask":[{"mask":"M0,0 L1,0 Z","id":"arc"}],"fx":[{"audio-waveform":5,"audio":"m","along":"ring"}]}"#,
                r#"{"id":"m","audio":"m.wav"},{"id":"a","rect":"1x1","mask":[{"mask":"M0,0 L1,0 Z","id":"ring"}],"fx":[{"audio-spectrum":5,"audio":"m","along":3}]}"#,
                r##"{"id":"a","rect":"1x1","mask":[{"mask":"M0,0 L1,0 Z","id":"ring"}],"fx":[{"fill":"#FF0000","along":"ring"}]}"##,
                r#"{"id":"a","rect":"1x1","span":"5f"}"#,
                r#"{"id":"a","rect":"1x1","span":"5-1f"}"#,
                r#"{"id":"a","rect":"1x1","offset":"-1x"}"#,
                r#"{"id":"a","rect":"1x1","stretch":0}"#,
                r#"{"id":"a","rect":"10x10","in":"fade-up","anim":{"opacity":"0 -> 1 @0-5f"}}"#,
                r#"{"id":"a","rect":"10x10","in":"fade-up","anim":{"position":"0,0 -> 1,1 @0-5f"}}"#,
            ],
            layers,
        ),
        section(
            "overrides",
            &[
                r#""overrides":3"#,
                r#""overrides":{"title":3}"#,
                r#""overrides":{"ghost":{"text":"x"}}"#,
                r#""overrides":{"title.text":{}}"#,
                r#""overrides":{"title":{"id":"other"}}"#,
                r#""overrides":{"title":{"text":5}}"#,
                r#""overrides":{"title":{"rect":"1x1"}}"#,
                r#""overrides":{"title":{"fill":"red"}}"#,
                r#""overrides":{"title/x":{"text":"x"}}"#,
                r#""overrides":{"title":{"skew":1}}"#,
                r#""overrides":{"$ghost":"x"}"#,
            ],
            |kv| {
                parse_document(&format!(
                    r#"{{"nakkash":1,"dur":"1s","comps":{{"label":{{"size":"10x10","dur":"1s","layers":[{{"id":"title","text":"Hi"}}]}}}},"layers":[{{"id":"card","comp":"label",{kv}}}]}}"#
                ))
            },
        ),
        section(
            "collapse",
            &[
                r#""collapse":1"#,
                r##""collapse":true,"adjust":true,"fx":[{"fill":"#FFFFFF"}]"##,
            ],
            |kv| {
                parse_document(&format!(
                    r#"{{"nakkash":1,"dur":"1s","comps":{{"label":{{"size":"10x10","dur":"1s","layers":[{{"id":"title","text":"Hi"}}]}}}},"layers":[{{"id":"card","comp":"label",{kv}}}]}}"#
                ))
            },
        ),
        section(
            "cutouts and tracks",
            &[
                r#""rect":"1x1","cutout":{}"#,
                r#""cutout":3"#,
                r#""cutout":{}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"size":3}"#,
                r#""cutout":{"prompts":[{"box":[0,0,1,1]}]}"#,
                r#""cutout":{"prompts":[{"frame":0}]}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[5,0,1,1]}]}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]},{"frame":0,"box":[0,0,1,1]}]}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"edges":"crisp"}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"band":0}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"agree":2}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"smooth":{"amount":2}}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"smooth":{"tolerance":0}}"#,
                r#""cutout":{"prompts":[{"frame":0,"box":[0,0,1,1]}],"baked":"abc"}"#,
                r#""tracks":{}"#,
                r#""tracks":[{"prompts":[{"frame":0,"box":[0,0,1,1]}]}]"#,
                r#""tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}]},{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}]}]"#,
                r#""tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"anchors":[{"frame":0,"corners":[[0,0],[1,0],[1,1],[0,1]]}]}]"#,
                r#""tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"anchors":[{"frame":3,"corners":[[0,0],[1,0],[1,1],[0,1]]},{"frame":3,"corners":[[0,0],[1,0],[1,1],[0,1]]}]}]"#,
                r#""tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"anchors":[{"frame":3}]}]"#,
                r#""tracks":[{"id":"a","prompts":[{"frame":0,"box":[0,0,1,1]}],"anchors":[{"corners":[[0,0],[1,0],[1,1],[0,1]]}]}]"#,
            ],
            |kv| {
                let content = match kv.starts_with(r#""rect""#) {
                    true => "",
                    false => r#""video":"clip.mov","box":[16,9],"#,
                };
                layers(&format!(r#"{{"id":"clip",{content}{kv}}}"#))
            },
        ),
        section(
            "overrides on a layer that draws no composition",
            &[r#""overrides":{"x":{}}"#],
            |kv| {
                parse_document(&format!(
                    r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"a","rect":"1x1",{kv}}}]}}"#
                ))
            },
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
                r#""x":"rest*x -> rest @0-5f""#,
                r#""x":[[0,"rest/2"],[5,"rest"]]"#,
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
                r#""x":[["-1x", 0]]"#,
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
                r#"{"a":{"skew":1}}"#,
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
                    r#"{{"nakkash":1,"dur":"1s","presets":{own},"layers":[{{"id":"a","rect":"1x1","in":"zoom"}}]}}"#
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
                "fade stagger 3f",
                "fade word stagger",
                "fade word stagger soon center",
                "fade word line",
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
                "0 -> 1@50% -> 2@40% -> 3 @0-10f",
                "0@10% -> 1 @0-10f",
                "0 -> 1@half -> 2 @0-10f",
                "0 -> 1 -> @0-10f",
            ],
        ),
        section(
            "labels",
            &[
                r#"[]"#,
                r#"{"part-2":"1s"}"#,
                r#"{"a":"b","b":"a+1f"}"#,
                r#"{"a":"<"}"#,
                r#"{"a":"soon"}"#,
            ],
            |labels| {
                parse_document(&format!(
                    r#"{{"nakkash":1,"dur":"1s","labels":{labels},"layers":[]}}"#
                ))
            },
        ),
        section(
            "relative times",
            &[
                r#"{"id":"a","rect":"1x1","offset":"<"}"#,
                r#"{"id":"a","rect":"1x1","span":"intro-2s"}"#,
                r#"{"id":"a","rect":"1x1","offset":"Intro"}"#,
                r#"{"id":"a","rect":"1x1","span":"intro"}"#,
            ],
            layers,
        ),
        section(
            "contents",
            &[
                r#"{"id":"a","shapes":[],"comp":"x"}"#,
                r#"{"id":"a","pivot":false}"#,
                r##"{"id":"a","pivot":true,"fill":"#fff"}"##,
                r#"{"id":"a","shapes":{}}"#,
                r#"{"id":"a","comp":"nowhere"}"#,
                r#"{"id":"a","pivot":true,"hidden":1}"#,
            ],
            layers,
        ),
        section(
            "shapes",
            &[
                "1",
                "{}",
                r#"{"round":3}"#,
                r##"{"rect":[1,1],"fill":"#fff"}"##,
                r#"{"rect":[1,1],"size":[2,2]}"#,
                r#"{"rect":[1,1],"colour":1}"#,
                r#"{"rect":[-1,1]}"#,
                r#"{"rect":"wide"}"#,
                r#"{"rect":[1,1],"round":-2}"#,
                r#"{"star":101}"#,
                r#"{"star":-1}"#,
                r#"{"star":5,"outer-radius":-3}"#,
                r#"{"star":5,"inner-radius":"far"}"#,
                r#"{"star":5,"type":"triangle"}"#,
                r#"{"star":5,"direction":"left"}"#,
                r#"{"star":5,"size":[1,1]}"#,
                r#"{"group":{}}"#,
                r#"{"group":[{"rect":[1,1],"cap":"round"}]}"#,
                r##"{"stroke":"#000","cap":"pointy"}"##,
                r##"{"stroke":"#000","cap":1}"##,
                r##"{"stroke":"#000","miter":0.5}"##,
                r##"{"fill":"#fff","id":"Bad"}"##,
                r##"{"fill":"#fff","id":"f"},{"group":[{"fill":"#000","id":"f"}]}"##,
                r#"{"path":[1,2]}"#,
                r#"{"path":"M0,0 Q1,1 2,2"}"#,
                r#"{"path":"M0,0 L1,1","anim":{"path":"M0,0 L1,1 -> M0,0 L2,2 @0-5f"}}"#,
                r#"{"path":"M0,0 L1,1","anim":{"path":[{"t":0,"v":"M0,0 L1,1"},{"t":5,"v":"M0,0 L1,1 Z"}]}}"#,
                r#"{"path":"M0,0 L1,1","anim":{"path":[{"t":0,"v":"M0,0 L1,1","curve":["ease","ease"]}]}}"#,
                r#"{"rect":[1,1],"anim":{"opacity":"0 -> 1 @0-5f"}}"#,
                r#"{"rect":[1,1],"anim":{"width":"0 -> 1 @0-5f","size":"0,0 -> 1,1 @0-5f"}}"#,
                r##"{"fill":"#fff","anim":{"color":"#000 -> 2,0,0 @0-5f"}}"##,
            ],
            shapes,
        ),
        section(
            "text",
            &[
                r#"{"id":"a","rect":"1x1","style":{"tracking":1}}"#,
                r#"{"id":"a","rect":"1x1","box":[1,1]}"#,
                r#"{"id":"a","text":"x","style":[]}"#,
                r#"{"id":"a","text":"x","style":{"font":"Inter 9"}}"#,
                r#"{"id":"a","text":"x","style":{"tsume":1}}"#,
                r#"{"id":"a","text":"x","style":{"tracking":"wide"}}"#,
                r#"{"id":"a","text":"x","style":{"caps":"loud"}}"#,
                r#"{"id":"a","text":"x","style":{"leading":"tall"}}"#,
                r#"{"id":"a","text":"x","style":{"stroke":[2,0,0]}}"#,
                r#"{"id":"a","text":"x","style":{"stroke-width":-1}}"#,
                r#"{"id":"a","text":"x","style":{"faux-bold":"yes"}}"#,
                r#"{"id":"a","text":"x","fill":[1,0]}"#,
                r#"{"id":"a","text":"x","runs":{}}"#,
                r##"{"id":"a","text":"x","runs":[{"fill":"#FFF"}]}"##,
                r#"{"id":"a","text":"x","runs":[{"range":[1,1]}]}"#,
                r#"{"id":"a","text":"x","runs":[{"range":[0,2]}]}"#,
                r#"{"id":"a","text":"x","runs":[{"range":[0,0.5]}]}"#,
                r#"{"id":"a","text":"x","runs":[{"range":"0-1"}]}"#,
                r#"{"id":"a","text":"x","runs":[{"range":[0,1],"align":"center"}]}"#,
                r#"{"id":"a","text":"x","paragraphs":[{"tracking":3}]}"#,
                r#"{"id":"a","text":"x","paragraphs":[{},{}]}"#,
                r#"{"id":"a","text":"x","paragraphs":[{"align":"middle"}]}"#,
                r#"{"id":"a","text":"x","box":[0,10]}"#,
                r#"{"id":"a","text":"x","box":"wide"}"#,
                r#"{"id":"a","rect":"1x1","fit":"shrink 1x1"}"#,
                r#"{"id":"a","text":"x","fit":3}"#,
                r#"{"id":"a","text":"x","fit":"grow 10x10"}"#,
                r#"{"id":"a","text":"x","fit":"shrink 0x10"}"#,
                r#"{"id":"a","text":"x","fit":"shrink"}"#,
                r#"{"id":"a","text":"x","box":[10,10],"fit":"wrap 10x10"}"#,
            ],
            layers,
        ),
        section(
            "animators",
            &[
                r#"{"id":"a","rect":"1x1","animate":[]}"#,
                r#"{"id":"a","text":"x","animate":[{"range":"char"}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":{}}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[{"rect":[1,1]}]}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[{"range":"sentence"}]}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[{"range":"char","by":"word"}]}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[{"range":"char","amount":2}]}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[{"range":"char","shape":"wave"}]}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[],"y":3}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[],"opacity":2}]}"#,
                r#"{"id":"a","text":"x","animate":[{"animator":[],"anim":{"glow":"0 -> 1 @0-5f"}}]}"#,
            ],
            layers,
        ),
        section(
            "fonts",
            &[
                r#"{"nakkash":1,"dur":"1s","fonts":"fonts","layers":[]}"#,
                r#"{"nakkash":1,"dur":"1s","fonts":[3],"layers":[]}"#,
            ],
            parse_document,
        ),
        section(
            "effects",
            &[
                r#"{"rect":[1,1]}"#,
                r##"{"fill":"#fff","opacity":3}"##,
                r##"{"fill":"#fff","rule":"evenodd"}"##,
            ],
            fx,
        ),
        section(
            "comps",
            &[
                r#""card":1"#,
                r#""Card":{"size":"1x1","dur":"1s","layers":[]}"#,
                r#""card":{"dur":"1s","layers":[]}"#,
                r#""card":{"size":"1x1","layers":[]}"#,
                r#""card":{"size":"1x1","dur":"1s"}"#,
                r#""card":{"size":"1x1","dur":"1s","fps":0,"layers":[]}"#,
                r#""card":{"size":"1x1","dur":"1s","layers":[],"tint":1}"#,
                r#""card":{"size":"1x1","dur":"1s","layers":[{"id":"x"}]}"#,
                r#""card":{"size":"1x1","dur":"1s","layers":[{"id":"x","comp":"card"}]}"#,
                r#""card":{"size":"1x1","dur":"1s","layers":[{"id":"x","comp":"b"}]},"b":{"size":"1x1","dur":"1s","layers":[{"id":"y","comp":"card"}]}"#,
            ],
            comps,
        ),
        section("range", &["@x-5f", "@05f", "@0-5"], TimeSpan::from_range),
        shorthand::<TimeSpan>("span", &["x-5f", "05f", "0-5", "@0-5f"]),
        shorthand::<crate::Address>("address", &["", "scene/card", "card.fx[glow"]),
        shorthand::<crate::Id>("id", &["Card"]),
        section("serde", &["{}", r#""dim""#], serde),
    ];
    insta::assert_snapshot!(report.join("\n\n"));
}
