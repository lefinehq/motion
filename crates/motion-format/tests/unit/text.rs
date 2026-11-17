//! Unit tests for `src/text.rs`.

use rstest::rstest;
use serde_json::json;

use super::*;

#[test]
fn scene_format_doc_lists_every_style_key() {
    let doc = include_str!("../../../../docs/design/scene-format.md");
    let documented: Vec<Vec<String>> = doc
        .lines()
        .skip_while(|l| !l.starts_with("| Style key |"))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .map(|l| l.split('|').skip(1).map(|c| c.trim().to_owned()).collect())
        .collect();
    let expected: Vec<Vec<String>> = STYLE_KEYS
        .iter()
        .map(|k| {
            let level = match k.level() {
                Level::Character => "character",
                Level::Paragraph => "paragraph",
                Level::Text => "text",
            };
            vec![
                format!("`{}`", k.name),
                level.to_owned(),
                k.form.to_owned(),
                String::new(),
            ]
        })
        .collect();
    assert_eq!(documented, expected);
}

#[test]
fn key_names_are_unique_and_documented() {
    let mut names: Vec<&str> = STYLE_KEYS.iter().map(|k| k.name).collect();
    assert!(
        STYLE_KEYS
            .iter()
            .all(|k| !k.doc.is_empty() && !k.form.is_empty())
    );
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), STYLE_KEYS.len());
    assert!(style_key("tracking").is_some_and(|k| k.level() == Level::Character));
    assert!(style_key("align").is_some_and(|k| k.level() == Level::Paragraph));
    assert!(style_key("kerning").is_none());
    assert_eq!(
        format!("{:?}", style_key("caps").unwrap()),
        "StyleKey { name: \"caps\", level: Character, .. }"
    );
}

#[rstest]
#[case::default_width("stroke-width", json!(1), true)]
#[case::zero_width("stroke-width", json!(0), false)]
#[case::clear_stroke("stroke", json!([0, 0, 0, 0]), true)]
#[case::auto_leading("leading", json!("auto"), true)]
#[case::left("align", json!("left"), true)]
#[case::rejected("tracking", json!("wide"), false)]
#[case::unscaled("horizontal-scale", json!(1), true)]
#[case::narrow("horizontal-scale", json!(0.93), false)]
#[case::flat("vertical-scale", json!(0), false)]
#[case::own_anchor("anchor-by", json!("char"), true)]
#[case::word_anchor("anchor-by", json!("word"), false)]
#[case::unmoved_anchor("anchor-align", json!([0, 0]), true)]
#[case::each_paints("paint-across", json!("each"), true)]
#[case::fills_over("paint-across", json!("fills-over"), false)]
#[case::layer_font("font", json!("Inter 40"), false)]
#[case::layer_fill("fill", json!([0, 0, 0, 0]), false)]
fn a_key_is_at_its_default_only_when_setting_it_changes_nothing(
    #[case] name: &str,
    #[case] value: Json,
    #[case] default: bool,
) {
    assert_eq!(style_key(name).unwrap().is_default(&value), default);
}

#[test]
fn only_font_and_fill_are_set_on_the_layer() {
    let on_layer: Vec<&str> = STYLE_KEYS
        .iter()
        .filter(|k| k.on_layer)
        .map(|k| k.name)
        .collect();
    assert_eq!(on_layer, ["font", "fill"]);
}

#[rstest]
#[case::a_language("tr", true)]
#[case::with_a_region("pt-BR", true)]
#[case::with_a_script("zh-Hant-TW", true)]
#[case::a_word("turkish", false)]
#[case::empty("", false)]
#[case::underscores("tr_TR", false)]
fn a_language_is_a_tag(#[case] tag: &str, #[case] valid: bool) {
    let mut style = CharacterStyle::new("Inter 40".parse().unwrap(), [1.0; 4]);
    let set = match &style_key("lang").unwrap().set {
        Setter::Character(set) => set(&mut style, &json!(tag)),
        Setter::Paragraph(_) | Setter::Text(_) => unreachable!("a character key"),
    };
    assert_eq!(set.is_ok(), valid, "{set:?}");
    assert_eq!(style.lang.as_deref(), valid.then_some(tag));
}
