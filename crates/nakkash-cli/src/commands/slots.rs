//! `nakkash slots`: the slots of a scene and of its compositions, which an
//! agent changes to make a variant (D44).

use std::path::Path;
use std::process::ExitCode;

use nakkash_engine::{Diagnostic, Kind, Listed, Result, Session};
use serde_json::json;

/// Prints each slot: where it is declared, its id, type, value and label,
/// and how many values follow it; `json` prints them as data, with the
/// place of each value that follows.
pub fn run(path: &Path, json: bool) -> Result<ExitCode> {
    let slots = Session::open(path)?.slots();
    if json {
        let data: Vec<_> = slots
            .iter()
            .map(|listed| {
                json!({
                    "comp": listed.comp,
                    "id": listed.slot.id.to_string(),
                    "type": listed.slot.ty.name(),
                    "value": listed.slot.value,
                    "label": listed.slot.label,
                    "followers": listed.followers,
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&data).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the slots as JSON: {e}"),
            )
        })?;
        println!("{text}");
    } else {
        print!("{}", listing(&slots));
    }
    Ok(ExitCode::SUCCESS)
}

/// The slots as lines under the scene and each composition that has any.
fn listing(slots: &[Listed]) -> String {
    let mut text = String::new();
    let mut owner = None;
    for listed in slots {
        if owner != Some(&listed.comp) {
            owner = Some(&listed.comp);
            match &listed.comp {
                None => text.push_str("scene\n"),
                Some(comp) => text.push_str(&format!("comps.{comp}\n")),
            }
        }
        let slot = &listed.slot;
        let label = slot
            .label
            .as_deref()
            .map(|l| format!("  {l:?}"))
            .unwrap_or_default();
        let follow = match listed.followers.len() {
            1 => "1 value follows".to_owned(),
            n => format!("{n} values follow"),
        };
        text.push_str(&format!(
            "  ${} {} {}{label}  ({follow})\n",
            slot.id, slot.ty, slot.value
        ));
    }
    if slots.is_empty() {
        text.push_str("no slots\n");
    }
    text
}
