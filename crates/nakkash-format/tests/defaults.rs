//! The defaults of the format's version, as one table: every property a
//! layer or an item rests at when a scene leaves it out, and every
//! setting's default choice. A scene may leave out any value written at its
//! default ([D91]), so a default that moves changes what such a scene means:
//! it bumps the format's version and comes with a migration that writes the
//! old value back ([D36]). This snapshot makes the move visible in review.
//!
//! [D36]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d36-the-format-only-changes-in-the-open
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::fmt::Write;

use nakkash_format::item::{ANIMATORS, EFFECTS, ItemKind, MASKS, PART_KINDS, SELECTORS, SHAPES};
use nakkash_format::property::{PROPERTIES, PropertySpec, Rest};

fn rest(spec: &PropertySpec) -> String {
    match &spec.rest {
        Rest::Value(value) => format!("{value:?}"),
        other => format!("{other:?}"),
    }
}

#[test]
fn the_defaults_of_format_version_1() {
    let mut table = String::new();
    for spec in PROPERTIES {
        writeln!(table, "layer {} = {}", spec.name, rest(spec)).unwrap();
    }
    let lists: [&[ItemKind]; 6] = [
        &SHAPES,
        &EFFECTS,
        &ANIMATORS,
        &SELECTORS,
        &PART_KINDS,
        &MASKS,
    ];
    for kind in lists.into_iter().flatten() {
        for row in kind.registry.rows {
            writeln!(table, "{} {} = {}", kind.name, row.name, rest(row)).unwrap();
        }
        for setting in kind.settings {
            writeln!(
                table,
                "{} {} = {:?}",
                kind.name, setting.name, setting.default
            )
            .unwrap();
        }
    }
    insta::assert_snapshot!(table);
}
