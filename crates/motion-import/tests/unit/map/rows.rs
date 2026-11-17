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
        To::Curves => super::super::curves::NAMES.iter().for_each(|name| {
            assert!(
                registry.target(name).is_some(),
                "{at}: `{}` writes the curve `{name}`, which `{}` lacks",
                row.match_name,
                registry.kind
            );
        }),
        To::Levels => {
            super::super::levels::NAMES.iter().for_each(|name| {
                assert!(
                    registry.target(name).is_some(),
                    "{at}: `{}` writes the level `{name}`, which `{}` lacks",
                    row.match_name,
                    registry.kind
                );
            });
            let channels = kind.and_then(|kind| kind.setting("channel"));
            super::super::levels::CHANNELS.iter().for_each(|channel| {
                assert!(
                    channels.is_some_and(|setting| setting.choices.contains(channel)),
                    "{at}: `{}` writes the channel `{channel}`, which is no `channel`",
                    row.match_name
                );
            });
        }
        To::Input => assert!(
            kind.is_some_and(|kind| kind.input.is_some()),
            "{at}: `{}` names a layer, which its kind does not read",
            row.match_name
        ),
        To::Outline => assert!(
            kind.is_some_and(|kind| kind.outline.is_some()),
            "{at}: `{}` names a mask, which its kind does not follow",
            row.match_name
        ),
        To::Split(point, depth) => {
            let point = registry.target(point);
            assert!(
                point.is_some_and(|t| t.value_type() == motion_format::ValueType::Point),
                "{at}: `{}` splits into a point the registry lacks",
                row.match_name
            );
            assert!(
                registry
                    .target(depth)
                    .is_some_and(|t| t.value_type() == motion_format::ValueType::Number),
                "{at}: `{}` splits off a depth `{depth}` the registry lacks",
                row.match_name
            );
        }
        To::Ignore(_) | To::OnlyDefault(_) | To::OnlyDefaultWith(..) => {}
    });
}

/// The transform tables of every kind of layer, with their names.
const TRANSFORMS: [(&[Row], &str); 5] = [
    (&TRANSFORM, "flat transform"),
    (&TRANSFORM_3D, "3D transform"),
    (&CAMERA_AIMED, "two-node camera"),
    (&CAMERA_FREE, "one-node camera"),
    (&LIGHT_TRANSFORM, "light"),
];

#[test]
fn every_transform_table_reads_the_same_properties() {
    let names = |rows: &[Row]| rows.iter().map(|row| row.match_name).collect::<Vec<_>>();
    for (rows, at) in TRANSFORMS {
        assert_eq!(names(rows), names(&TRANSFORM), "{at}");
    }
}

#[test]
fn only_3d_tables_write_depth() {
    let depth = |rows: &[Row]| {
        rows.iter().any(|row| {
            matches!(row.to, To::Split(..))
                || matches!(row.to, To::Prop(name) if LAYER
                    .target(name)
                    .is_some_and(|t| motion_format::property::Prop::DEPTH.contains(&t.prop)))
        })
    };
    assert!(!depth(&TRANSFORM), "a flat layer drops its depth");
    for (rows, at) in &TRANSFORMS[1..] {
        assert!(depth(rows), "{at}");
    }
}

#[test]
fn every_row_targets_a_property_or_setting_the_scene_has() {
    for (rows, at) in TRANSFORMS {
        check(rows, &LAYER, None, at);
    }
    check(&CAMERA_OPTIONS, &LAYER, None, "camera options");
    check(&LAYER_ROOT, &LAYER, None, "layer");
    KINDS.iter().for_each(|kind| {
        let scene = kind.scene_kind();
        for rows in all_rows(kind) {
            check(rows, scene.registry, Some(scene), kind.match_name);
        }
    });
}

/// Every set of rows `kind` has: its own, and those its switch picks.
fn all_rows(kind: &Kind) -> Vec<&'static [Row]> {
    std::iter::once(kind.rows)
        .chain(kind.switch.as_ref().map(|switch| switch.off_rows))
        .collect()
}

#[test]
fn a_switch_is_a_row_of_both_its_sets_and_picks_rows_of_the_same_names() {
    KINDS
        .iter()
        .filter_map(|kind| kind.switch.as_ref().map(|switch| (kind, switch)))
        .for_each(|(kind, switch)| {
            let names = |rows: &[Row]| {
                let mut names: Vec<&str> = rows.iter().map(|row| row.match_name).collect();
                names.sort_unstable();
                names
            };
            assert_eq!(
                names(kind.rows),
                names(switch.off_rows),
                "{}",
                kind.match_name
            );
            assert!(
                kind.rows
                    .iter()
                    .any(|row| row.match_name == switch.match_name),
                "{}",
                kind.match_name
            );
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
    KINDS.iter().flat_map(all_rows).for_each(|rows| {
        let mut names: Vec<&str> = rows.iter().map(|row| row.match_name).collect();
        let all = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all, "{names:?}: a row is listed twice");
    });
}

#[test]
fn parameter_defaults_are_only_used_by_effects() {
    KINDS.iter().for_each(|kind| {
        let effect = std::ptr::eq(kind.table, EFFECTS.as_slice());
        all_rows(kind)
            .into_iter()
            .flatten()
            .filter(|row| row.default == Missing::Parameter)
            .for_each(|row| assert!(effect, "{}: `{}`", kind.match_name, row.match_name));
    });
}
