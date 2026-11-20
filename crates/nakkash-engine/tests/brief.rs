//! A template's brief ([D91]) says what each colour slot paints, read from
//! the effects and backgrounds that take it, infers its sections' story
//! roles from their order, and gives each number its bounds; the measure
//! stage warns where the template's own text is shrunk to fit.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use nakkash_engine::Session;
use nakkash_engine::brief::brief;

/// Three sections; colours that only effects and a composition's
/// background take; a number with a floor and one with a ceiling; a title
/// too long for its box.
const TEMPLATE: &str = r##"{"nakkash":1,"size":"640x360","fps":10,"dur":"3s","bg":"#101010",
"slots":{"shadow":"#000000","dark":"#102030","light":"#F0E0D0","from":"#FF0000","to":"#0000FF","c1":"#00FF00",
 "plate":"#223344","speed":{"number":1,"min":0.5},"gap":{"number":4,"max":10},
 "title":{"text":"A rather long headline that runs on"}},
"comps":{
 "intro":{"size":"640x360","dur":"1s","bg":"$plate","layers":[
  {"id":"t","text":"$title","font":"Inter Variable 64 700","fill":"#FFFFFF","box":[200,80],"fit":"shrink"}]},
 "middle":{"size":"640x360","dur":"1s","layers":[{"id":"r","rect":"100x100","fill":"#808080",
  "fx":[{"drop-shadow":8,"color":"$shadow"},{"tint":1,"black":"$dark","white":"$light"}]}]},
 "outro":{"size":"640x360","dur":"1s","layers":[{"id":"q","rect":"100x100","fill":"#808080",
  "fx":[{"ramp":0.5,"start-color":"$from","end-color":"$to"},{"four-color":1,"color-1":"$c1"}]}]}},
"layers":[{"id":"intro","comp":"intro","span":"0s-1s"},{"id":"middle","comp":"middle","span":"1s-2s"},
 {"id":"outro","comp":"outro","span":"2s-3s"}]}"##;

#[test]
fn a_brief_names_what_each_colour_paints_and_the_sections_roles() {
    let session: Session = TEMPLATE.parse().expect("a valid template");
    let b = brief(&session, "t.json", None).expect("a brief");
    let paints: Vec<(&str, &str)> = b
        .colors
        .iter()
        .map(|c| (c.id.as_str(), c.paints.as_str()))
        .collect();
    assert_eq!(
        paints,
        [
            ("shadow", "shadow"),
            ("dark", "grade-dark"),
            ("light", "grade-light"),
            ("from", "gradient-start"),
            ("to", "gradient-end"),
            ("c1", "gradient-1"),
            ("plate", "background"),
        ]
    );
    // No section is named for its role: the first is the hook, the last
    // the call to action, and one between without text a feature; all
    // inferred.
    let roles: Vec<(&str, &str, bool)> = b
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.role.as_str(), s.inferred))
        .collect();
    assert_eq!(
        roles,
        [
            ("intro", "hook", true),
            ("middle", "feature", true),
            ("outro", "cta", true)
        ]
    );
    let bounds: Vec<(&str, Option<f64>, Option<f64>)> = b
        .numbers
        .iter()
        .map(|n| (n.id.as_str(), n.min, n.max))
        .collect();
    assert_eq!(
        bounds,
        [("speed", Some(0.5), None), ("gap", None, Some(10.0))]
    );
    let text = b.to_string();
    assert!(text.contains("speed ≥0.5 now 1"), "{text}");
    assert!(text.contains("gap ≤10 now 4"), "{text}");
    assert!(text.contains("| intro | hook? |"), "{text}");
}

#[test]
fn the_templates_own_text_shrunk_to_fit_is_a_warning() {
    let session: Session = TEMPLATE.parse().expect("a valid template");
    let problems = nakkash_engine::instance::own_problems(&session);
    let shrunk: Vec<_> = problems
        .iter()
        .filter(|p| p.code == "text.shrunk")
        .collect();
    assert_eq!(shrunk.len(), 1, "{problems:?}");
    assert_eq!(shrunk[0].slot.as_deref(), Some("title"));
    assert!(
        shrunk[0].message.contains("px of 64 to fit"),
        "{}",
        shrunk[0].message
    );
}
