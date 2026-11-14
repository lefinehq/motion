//! Unit tests for `src/map/rows.rs`: every row lands somewhere the scene
//! format has.

use motion_format::property::LAYER;

use super::*;

/// Every row's target, setting and choices exist in `kind`'s registry and
/// settings, or in the layer's for the transform.
fn check(
    rows: &[Row],
    registry: &'static motion_format::property::Registry,
    kind: Option<&ItemKind>,
    at: &str,
) {
    rows.iter().for_each(|row| match row.to {
        To::Prop(name) => assert!(
            registry.target(name).is_some(),
            "{at}: `{}` targets `{name}`, which `{}` lacks",
            row.match_name,
            registry.kind
        ),
        To::Setting(name, choices) => {
            let setting = kind.and_then(|kind| kind.setting(name)).unwrap_or_else(|| {
                panic!(
                    "{at}: `{}` sets `{name}`, which is no setting",
                    row.match_name
                )
            });
            choices.iter().for_each(|(_, choice)| {
                assert!(
                    setting.choices.contains(choice),
                    "{at}: `{choice}` is no `{name}`"
                );
            });
        }
        To::Ignore(_) | To::OnlyDefault(_) => {}
    });
}

#[test]
fn every_row_targets_a_property_or_setting_the_scene_has() {
    check(&TRANSFORM, &LAYER, None, "transform");
    check(&LAYER_ROOT, &LAYER, None, "layer");
    KINDS.iter().for_each(|kind| {
        let scene = kind.scene_kind();
        check(kind.rows, scene.registry, Some(scene), kind.match_name);
    });
}

#[test]
fn every_match_name_has_one_fate() {
    let mut names: Vec<&str> = KINDS
        .iter()
        .map(|kind| kind.match_name)
        .chain(POLICIES.iter().map(|(name, _)| *name))
        .collect();
    let all = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        all,
        "a group is both an item and a policy, or listed twice"
    );
    KINDS.iter().for_each(|kind| {
        let mut rows: Vec<&str> = kind.rows.iter().map(|row| row.match_name).collect();
        let all = rows.len();
        rows.sort_unstable();
        rows.dedup();
        assert_eq!(
            rows.len(),
            all,
            "{}: a row is listed twice",
            kind.match_name
        );
    });
}

#[test]
fn parameter_defaults_are_only_used_by_effects() {
    KINDS.iter().for_each(|kind| {
        let effect = std::ptr::eq(kind.table, EFFECTS.as_slice());
        kind.rows
            .iter()
            .filter(|row| row.default == Missing::Parameter)
            .for_each(|row| assert!(effect, "{}: `{}`", kind.match_name, row.match_name));
    });
}
