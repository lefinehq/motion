//! Locks on edits ([D51]): an edit that touches what the scene locks is
//! refused, naming the lock, before it is made.
//!
//! [D51]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d51-locks-are-refused-in-the-edit

use std::collections::BTreeSet;

use nakkash_format::key::{COMPS, FONT, LAYERS, SLOTS};
use nakkash_format::shorthand::Font;
use nakkash_format::{Address, Content, Diagnostic, Document, Kind, Lock, Result, Step};
use serde_json::Value as Json;

use crate::{Edit, EditValue};

/// `Ok` when `edit` touches none of `doc`'s locks.
///
/// # Errors
/// A conflict diagnostic naming the first lock it touches.
pub(crate) fn check(doc: &Document, edit: &Edit) -> Result<()> {
    let scene = doc.scene();
    let held = scene.held_locks();
    if held.is_empty() {
        return Ok(());
    }
    let target = &edit.target;
    for (lock, written) in &held {
        // `subject` ends where "locked" follows: "`logo` is".
        let refused = |subject: String| {
            Err(
                Diagnostic::new(Kind::Conflict, format!("{subject} locked by `{written}`"))
                    .expected(
                        "an edit of what the scene does not lock; the designer changes `locks`",
                    ),
            )
        };
        match lock {
            Lock::Layer(locked) => {
                if target.layers == locked.layers {
                    return refused(format!("`{target}` is"));
                }
                if target.layers.starts_with(&locked.layers) && !target.layers.is_empty() {
                    return refused(format!("`{target}` is inside `{locked}`, which is"));
                }
                for change in &edit.changes {
                    if reaches(scene, target, change.key.steps(), locked) {
                        let key = &change.key;
                        return refused(format!("`{target} {key}` reaches `{locked}`, which is"));
                    }
                }
            }
            Lock::Slot(slot) => {
                let own = format!("${slot}");
                for change in &edit.changes {
                    let steps = change.key.steps();
                    let names_it = match (target.is_scene(), steps) {
                        (true, [Step::Key(key)]) => *key == own || key == SLOTS,
                        (true, [Step::Key(key), Step::Key(id), ..]) => {
                            key == SLOTS && id == &slot.to_string()
                        }
                        _ => false,
                    };
                    if names_it {
                        return refused(format!("slot `{own}` is"));
                    }
                }
            }
            Lock::Palette => {
                let palette = colours(doc.raw());
                for value in set_values(edit) {
                    if let Some(new) = colours(value).into_iter().find(|c| !palette.contains(c)) {
                        return refused(format!(
                            "`{}` is not one of the scene's colours, which are",
                            hex(new)
                        ));
                    }
                }
            }
            Lock::Typefaces => {
                let families = typefaces(doc.raw());
                for change in &edit.changes {
                    let EditValue::Set(value) = &change.value else {
                        continue;
                    };
                    let under_font = change.key.steps().last() == Some(&Step::Key(FONT.to_owned()));
                    let new = match (under_font, value) {
                        (true, Json::String(font)) => font_family(font).into_iter().collect(),
                        _ => typefaces(value),
                    };
                    if let Some(family) = new.into_iter().find(|f| !families.contains(f)) {
                        return refused(format!(
                            "`{family}` is not one of the scene's typefaces, which are"
                        ));
                    }
                }
            }
            Lock::Role(_) => {}
        }
    }
    Ok(())
}

/// Whether a change of `steps` on `target` changes the locked layer
/// `locked` from outside it: the scene's layers, a composition one of the
/// layers on the way to it draws, or an override reaching into it.
fn reaches(
    scene: &nakkash_format::Scene,
    target: &Address,
    steps: &[Step],
    locked: &Address,
) -> bool {
    let key = |i: usize| match steps.get(i) {
        Some(Step::Key(key)) => Some(key.as_str()),
        _ => None,
    };
    if target.is_scene() {
        return match key(0) {
            Some(LAYERS) => true,
            Some(COMPS) => key(1).is_some_and(|comp| {
                (1..=locked.layers.len()).any(|n| {
                    matches!(
                        scene.layer(&locked.layers[..n]).map(|l| &l.content),
                        Some(Content::Comp(drawn)) if drawn.id == comp
                    )
                })
            }),
            _ => false,
        };
    }
    // An override of an inner layer, `overrides.title`, or all of them.
    if key(0) != Some(nakkash_format::key::OVERRIDES) || !locked.layers.starts_with(&target.layers)
    {
        return false;
    }
    match key(1).map(str::parse::<Address>) {
        None => true,
        Some(Ok(inner)) => {
            let reached: Vec<_> = target.layers.iter().chain(&inner.layers).cloned().collect();
            reached.starts_with(&locked.layers) || locked.layers.starts_with(&reached)
        }
        Some(Err(_)) => false,
    }
}

/// The values `edit` sets, a slot's among them.
fn set_values(edit: &Edit) -> impl Iterator<Item = &Json> {
    edit.changes
        .iter()
        .filter_map(|change| match &change.value {
            EditValue::Set(value) => Some(value),
            EditValue::Remove => None,
        })
}

/// Every colour in `json`, as 8-bit RGBA: strings and lists that read as
/// one.
fn colours(json: &Json) -> BTreeSet<[u8; 4]> {
    fn walk(json: &Json, out: &mut BTreeSet<[u8; 4]>) {
        if let Some(c) = color(json) {
            out.insert(c);
            return;
        }
        match json {
            Json::Array(items) => items.iter().for_each(|i| walk(i, out)),
            Json::Object(fields) => fields.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(json, &mut out);
    out
}

/// `json` as an 8-bit colour: a string, or a list of three or four
/// channels.
fn color(json: &Json) -> Option<[u8; 4]> {
    let colour = match json {
        Json::String(text) if text.starts_with('#') => nakkash_format::color(json),
        Json::Array(items) if (3..=4).contains(&items.len()) => nakkash_format::color(json),
        _ => None,
    }?;
    Some(colour.map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8))
}

fn hex([r, g, b, a]: [u8; 4]) -> String {
    match a {
        255 => format!("#{r:02X}{g:02X}{b:02X}"),
        _ => format!("#{r:02X}{g:02X}{b:02X}{a:02X}"),
    }
}

/// Every font family `json` names under a `font` key.
fn typefaces(json: &Json) -> BTreeSet<String> {
    fn walk(json: &Json, out: &mut BTreeSet<String>) {
        match json {
            Json::Array(items) => items.iter().for_each(|i| walk(i, out)),
            Json::Object(fields) => {
                for (key, value) in fields {
                    match (key.as_str(), value) {
                        (FONT, Json::String(font)) => out.extend(font_family(font)),
                        _ => walk(value, out),
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(json, &mut out);
    out
}

fn font_family(font: &str) -> Option<String> {
    font.parse::<Font>().ok().map(|f| f.family)
}

#[cfg(test)]
#[path = "../tests/unit/lock.rs"]
mod tests;
