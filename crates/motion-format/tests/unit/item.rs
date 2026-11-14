//! Unit tests for `src/item.rs`: the kind tables and their registries.

use super::*;
use crate::table::names;

#[test]
fn kind_names_are_unique_in_each_table() {
    for kinds in [&SHAPES[..], &EFFECTS[..]] {
        let mut all: Vec<&str> = names(kinds).collect();
        let count = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), count, "{all:?}");
    }
}

#[test]
fn every_kind_has_its_primary_settings_and_docs() {
    for kind in SHAPES.iter().chain(&EFFECTS) {
        assert!(!kind.doc.is_empty(), "{}", kind.name);
        assert_eq!(
            kind.primary.is_none(),
            kind.role == Role::Group,
            "only a group's key holds items: {}",
            kind.name
        );
        if let Some(prop) = kind.primary() {
            assert!(std::ptr::eq(prop.registry(), kind.registry));
        }
        for setting in kind.settings {
            assert!(
                setting.choices.contains(&setting.default),
                "{}",
                setting.name
            );
            assert_eq!(
                kind.setting(setting.name).map(|s| s.name),
                Some(setting.name)
            );
            assert!(
                kind.registry.prop(setting.name).is_none(),
                "a setting and a property share `{}`",
                setting.name
            );
        }
    }
    assert!(shape_kind("rect").is_some_and(|k| k.role == Role::Geometry));
    assert!(effect_kind("fill").is_some_and(|k| k.role == Role::Effect));
    assert!(shape_kind("blur").is_none());
}

#[test]
fn every_registry_names_its_rows_once_and_rests_at_its_type() {
    for kind in SHAPES.iter().chain(&EFFECTS) {
        let all: Vec<&str> = kind.registry.target_names().collect();
        let mut unique = all.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "{all:?}");
        for prop in kind.registry.props() {
            let Rest::Value(rest) = &prop.spec().rest else {
                panic!("{} rests at a value", prop.name());
            };
            assert_eq!(rest.value_type(), prop.spec().ty, "{}", prop.name());
            assert!(!prop.spec().doc.is_empty(), "{}", prop.name());
        }
    }
}

#[test]
fn the_constants_name_the_rows_code_reads() {
    use props::*;
    let expected = [
        (RECT_SIZE, "rect", "size"),
        (RECT_POSITION, "rect", "position"),
        (RECT_ROUND, "rect", "round"),
        (ELLIPSE_SIZE, "ellipse", "size"),
        (ELLIPSE_POSITION, "ellipse", "position"),
        (PATH_PATH, "path", "path"),
        (FILL_COLOR, "fill", "color"),
        (FILL_OPACITY, "fill", "opacity"),
        (STROKE_COLOR, "stroke", "color"),
        (STROKE_OPACITY, "stroke", "opacity"),
        (STROKE_WIDTH, "stroke", "width"),
        (STROKE_MITER, "stroke", "miter"),
        (FILL_EFFECT_COLOR, "fill effect", "color"),
        (FILL_EFFECT_OPACITY, "fill effect", "opacity"),
    ];
    for (prop, kind, name) in expected {
        assert_eq!((prop.registry().kind, prop.name()), (kind, name));
    }
    assert_ne!(
        FILL_COLOR, FILL_EFFECT_COLOR,
        "rows of different registries"
    );
}

#[test]
fn an_item_reads_its_values_and_settings() {
    let stroke = shape_kind("stroke").unwrap();
    let item = Item {
        kind: stroke,
        id: None,
        values: vec![(props::STROKE_WIDTH, Value::Number(6.0))],
        settings: vec![1, 2],
        items: Vec::new(),
        anim: Vec::new(),
        hidden: false,
    };
    assert_eq!(item.rest(props::STROKE_WIDTH), Value::Number(6.0));
    assert_eq!(item.rest(props::STROKE_MITER), Value::Number(4.0));
    assert_eq!(item.value(props::STROKE_COLOR), None);
    assert_eq!(
        (item.setting("cap"), item.setting("join")),
        ("round", "bevel")
    );
}

/// The rows of the scene-format table whose header starts with `header`:
/// each row's cells, trimmed.
fn documented(header: &str) -> Vec<Vec<String>> {
    include_str!("../../../../docs/design/scene-format.md")
        .lines()
        .skip_while(|l| !l.starts_with(header))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .map(|l| l.split('|').skip(1).map(|c| c.trim().to_owned()).collect())
        .collect()
}

/// `names` as the doc writes a list of them: "`a`, `b`".
fn quoted(names: impl Iterator<Item = &'static str>) -> String {
    names
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn scene_format_doc_lists_every_kind_with_its_properties_and_settings() {
    for (header, kinds) in [("| Item |", &SHAPES[..]), ("| Effect |", &EFFECTS[..])] {
        let rows = documented(header);
        assert_eq!(rows.len(), kinds.len(), "{header}");
        for (row, kind) in rows.iter().zip(kinds) {
            assert_eq!(row[0], format!("`{}`", kind.name));
            let holds = kind
                .primary
                .map_or("its items".to_owned(), |p| format!("`{p}`"));
            assert_eq!(row[1], holds, "{}", kind.name);
            assert_eq!(row[2], quoted(kind.registry.names()), "{}", kind.name);
            let settings: Vec<String> = kind
                .settings
                .iter()
                .map(|s| format!("`{}`: {}", s.name, quoted(s.choices.iter().copied())))
                .collect();
            assert_eq!(row[3], settings.join("; "), "{}", kind.name);
        }
    }
}
