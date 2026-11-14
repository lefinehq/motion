//! Unit tests for `src/text.rs`.

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
