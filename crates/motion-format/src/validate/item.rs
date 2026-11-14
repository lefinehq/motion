//! The rules of shape items and effects: one kind key, then the kind's
//! settings and resting values, and tracks against the kind's registry.

use std::collections::HashSet;

use serde_json::Value as Json;

use super::{anim, follows_id_rule, typed_value};
use crate::doc::{ItemDoc, ItemsDoc, List, Object, ValueDoc};
use crate::item::{Item, ItemKind, Role};
use crate::parse::key::{ANIM, HIDDEN, ID};
use crate::property::{Prop, Target};
use crate::table;
use crate::value::Value;
use crate::{Diagnostic, Kind, Result};

/// A list of items whose kinds come from `table`, with paths relative to the list.
pub(super) fn items(docs: &ItemsDoc, kinds: &'static [ItemKind], fps: f64) -> Result<Vec<Item>> {
    docs.iter()
        .enumerate()
        .map(|(i, Object(doc))| item(doc, kinds, fps).map_err(|e| e.under(format!("[{i}]"))))
        .collect()
}

/// One item: exactly one kind key, then that kind's keys only.
fn item(doc: &ItemDoc, kinds: &'static [ItemKind], fps: f64) -> Result<Item> {
    let kind = kind_of(doc, kinds)?;
    let mut values = Vec::new();
    let mut settings: Vec<usize> = kind.settings.iter().map(|s| s.default_index()).collect();
    let mut items = Vec::new();
    for (key, written) in &doc.values {
        let place = |e: Diagnostic| e.under(key.as_str());
        if key == kind.name {
            match kind.primary() {
                Some(prop) => values.insert(0, (prop, value_of(written, prop).map_err(place)?)),
                None => items = group_items(written, kinds, fps).map_err(place)?,
            }
        } else if let Some(at) = table::position(kind.settings, key) {
            settings[at] = choice(written, kind, at).map_err(place)?;
        } else if let Some(prop) = kind.registry.prop(key) {
            if kind.primary() == Some(prop) {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("`{key}` is the value `{}` holds", kind.name),
                )
                .at(key.as_str())
                .expected(format!("the {key} only as the value of `{}`", kind.name)));
            }
            values.push((prop, value_of(written, prop).map_err(place)?));
        } else {
            return Err(
                Diagnostic::unknown(format!("{} key", kind.name), key, keys(kind)).at(key.as_str()),
            );
        }
    }
    if let Some(id) = doc.id.as_deref() {
        follows_id_rule(id)?;
    }
    let Object(tracks) = &doc.anim;
    Ok(Item {
        kind,
        id: doc.id.clone(),
        values,
        settings,
        items,
        anim: anim(tracks, kind.registry, fps).map_err(|e| e.under(ANIM))?,
        hidden: doc.hidden.unwrap_or(false),
    })
}

/// The one key of `doc` that names a kind of `kinds`.
fn kind_of(doc: &ItemDoc, kinds: &'static [ItemKind]) -> Result<&'static ItemKind> {
    let mut named = doc
        .values
        .keys()
        .filter_map(|key| table::lookup(kinds, key).map(|kind| (key, kind)));
    let one_of = || {
        let names: Vec<String> = table::names(kinds).map(|n| format!("`{n}`")).collect();
        format!("one kind key: {}", names.join(", "))
    };
    match (named.next(), named.next()) {
        (Some((_, kind)), None) => Ok(kind),
        (None, _) => {
            Err(Diagnostic::new(Kind::Missing, "needs the key of its kind").expected(one_of()))
        }
        (Some((first, _)), Some((second, _))) => Err(Diagnostic::new(
            Kind::Conflict,
            format!("`{first}` and `{second}` are both kinds; an item has one"),
        )
        .at(second.as_str())
        .expected(one_of())),
    }
}

/// A resting value of `prop`, read as JSON wrote it and checked.
fn value_of(written: &Json, prop: Prop) -> Result<Value> {
    crate::de::from_value::<ValueDoc>(written)
        .and_then(|doc| typed_value(&doc, Target::whole(prop)))
}

/// A group's items: a list of shape items.
fn group_items(written: &Json, kinds: &'static [ItemKind], fps: f64) -> Result<Vec<Item>> {
    let List(docs) = crate::de::from_value::<List<ItemsDoc>>(written)?;
    items(&docs, kinds, fps)
}

/// Setting `at` of `kind`, which must be one of its choices.
fn choice(written: &Json, kind: &ItemKind, at: usize) -> Result<usize> {
    let spec = &kind.settings[at];
    let name: String = crate::de::from_value(written)?;
    spec.choices.iter().position(|c| *c == name).ok_or_else(|| {
        Diagnostic::unknown(format!("{} {}", kind.name, spec.name), &name, spec.choices)
    })
}

/// Every key an item of `kind` may hold, for an unknown-key error.
fn keys(kind: &'static ItemKind) -> impl Iterator<Item = &'static str> {
    let primary = kind.primary;
    [kind.name, ID, HIDDEN, ANIM]
        .into_iter()
        .chain(kind.settings.iter().map(|s| s.name))
        .chain(kind.registry.names().filter(move |n| Some(*n) != primary))
}

/// No two items of a layer's `items`, at any depth, share an id. The error
/// sits at the later one, under `list`.
pub(super) fn unique_ids(items: &[Item], list: &str) -> Result<()> {
    fn walk<'a>(items: &'a [Item], path: &str, seen: &mut HashSet<&'a str>) -> Result<()> {
        for (i, item) in items.iter().enumerate() {
            let here = format!("{path}[{i}]");
            if let Some(id) = item.id.as_deref()
                && !seen.insert(id)
            {
                return Err(
                    Diagnostic::new(Kind::Conflict, format!("duplicate item id `{id}`"))
                        .at(ID)
                        .under(here)
                        .expected("every item of a layer to have its own id"),
                );
            }
            if item.kind.role == Role::Group {
                walk(&item.items, &format!("{here}.{}", item.kind.name), seen)?;
            }
        }
        Ok(())
    }
    walk(items, list, &mut HashSet::new())
}

#[cfg(test)]
#[path = "../../tests/unit/validate/item.rs"]
mod tests;
