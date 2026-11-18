//! The content axis ([D78]): rows of a CSV or JSON file, each a variant.
//!
//! A column names where its values go: a slot (`$title`), a role
//! (`@headline`) or a layer's key (`card-2/title.text`). Each value is made
//! as an edit line, so it goes through the locks as a batch's edits do
//! ([D52]). A new image is fitted inside the box of the one it replaces,
//! keeping its own aspect.
//!
//! [D52]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d52-a-batch-makes-every-variant-from-one-master
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use nakkash_format::{Content, Diagnostic, Document, Kind, Result, RoleTarget};
use serde_json::{Value as Json, json};

use super::places::Held;
use crate::session::Session;

/// The rows of `path`: a CSV file whose first line names the columns, or a
/// JSON list of objects.
///
/// # Errors
/// A diagnostic when the file cannot be read or is neither.
pub(crate) fn read_rows(path: &Path) -> Result<Vec<BTreeMap<String, String>>> {
    let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
    let bad = |why: String| {
        Diagnostic::new(
            Kind::Invalid,
            format!("`{}` is not a list of rows: {why}", path.display()),
        )
        .expected(r#"a CSV file with a header line, or JSON such as [{"$title":"Sale"}]"#)
    };
    if path.extension().is_some_and(|e| e == "json") {
        let rows: Vec<serde_json::Map<String, Json>> =
            serde_json::from_str(&text).map_err(|e| bad(e.to_string()))?;
        return Ok(rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|(k, v)| {
                        let v = match v {
                            Json::String(s) => s,
                            other => other.to_string(),
                        };
                        (k, v)
                    })
                    .collect()
            })
            .collect());
    }
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = csv_fields(lines.next().ok_or_else(|| bad("it is empty".into()))?);
    lines
        .enumerate()
        .map(|(i, line)| {
            let fields = csv_fields(line);
            if fields.len() != header.len() {
                return Err(bad(format!(
                    "row {} has {} fields, the header {}",
                    i + 1,
                    fields.len(),
                    header.len()
                )));
            }
            Ok(header.iter().cloned().zip(fields).collect())
        })
        .collect()
}

/// The fields of one CSV line: commas separate them, and a field in double
/// quotes may hold commas and doubled quotes.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut field)),
            (c, _) => field.push(c),
        }
    }
    fields.push(field);
    fields
}

/// Writes one row's `values` into `raw` through edit lines, and says what
/// was set and what a lock refused.
pub(crate) fn apply(
    raw: &mut Json,
    master: &Session,
    held: &Held,
    values: &BTreeMap<String, String>,
    base: &Path,
) -> Result<String> {
    let mut doc = Document::from_value(raw.clone())?;
    let mut set = 0;
    let mut refused = Vec::new();
    for (column, value) in values {
        if slot_of(&doc, column).is_some_and(|id| held.holds_slot(None, &id)) {
            refused.push(format!(
                "{column}: held, by a lock or as a slot that changes nothing drawn"
            ));
            continue;
        }
        let lines = match lines_for(&doc, master, column, value, base) {
            Ok(lines) => lines,
            Err(why) => {
                refused.push(format!("{column}: {}", why.message));
                continue;
            }
        };
        let mut next = doc.clone();
        let mut ok = true;
        for line in &lines {
            match nakkash_edit::edit_line(&next, line) {
                Ok(d) => next = d,
                Err(why) => {
                    refused.push(format!("{column}: {}", why.message));
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            doc = next;
            set += 1;
        }
    }
    *raw = doc.raw().clone();
    Ok(match refused.is_empty() {
        true => format!("content: {set} values set"),
        false => format!("content: {set} values set, {}", refused.join("; ")),
    })
}

/// The scene's slot `column` writes: `$id`, or a role naming a slot.
fn slot_of(doc: &Document, column: &str) -> Option<String> {
    if let Some(id) = column.strip_prefix('$') {
        return Some(id.to_owned());
    }
    let role = column.strip_prefix('@')?;
    let found = doc.scene().roles.iter().find(|r| r.id.as_ref() == role)?;
    match &found.target {
        RoleTarget::Slot(id) => Some(id.to_string()),
        RoleTarget::Layer(_) => None,
    }
}

/// The edit lines that write `value` where `column` names.
fn lines_for(
    doc: &Document,
    master: &Session,
    column: &str,
    value: &str,
    base: &Path,
) -> Result<Vec<String>> {
    let quoted = json!(value).to_string();
    if column.starts_with('$') {
        let typed = match doc
            .slots()
            .iter()
            .find(|l| l.comp.is_none() && l.slot.id.as_ref() == &column[1..])
        {
            Some(listed) if listed.slot.ty.name() == "text" => quoted,
            Some(_) => value.to_owned(),
            None => quoted,
        };
        return Ok(vec![format!("{column}={typed}")]);
    }
    if let Some(role) = column.strip_prefix('@') {
        let scene = doc.scene();
        let found = scene
            .roles
            .iter()
            .find(|r| r.id.as_ref() == role)
            .ok_or_else(|| {
                Diagnostic::new(Kind::Missing, format!("the scene has no role `{role}`"))
                    .expected("a role `nakkash roles` lists")
            })?;
        return match &found.target {
            RoleTarget::Slot(_) => Ok(vec![format!("@{role}={value}")]),
            RoleTarget::Layer(address) => match scene.layer(&address.layers).map(|l| &l.content) {
                Some(Content::Image(image)) => {
                    let mut lines = vec![format!("@{role} image={quoted}")];
                    // An image with no file yet has no box to keep.
                    if let Some(old) = &image.path {
                        lines.extend(fit_image(master, &address.to_string(), old, value, base));
                    }
                    Ok(lines)
                }
                Some(Content::Video(_)) => Ok(vec![format!("@{role} video={quoted}")]),
                _ => Ok(vec![format!("@{role} text={quoted}")]),
            },
        };
    }
    let (address, key) = column.rsplit_once('.').ok_or_else(|| {
        Diagnostic::new(Kind::Invalid, format!("`{column}` names no place"))
            .expected("`$slot`, `@role` or `address.key`, such as `card-2/title.text`")
    })?;
    // A value that reads as JSON goes as it is; any other as words.
    let written = match serde_json::from_str::<Json>(value) {
        Ok(Json::String(_)) | Err(_) => quoted,
        Ok(_) => value.to_owned(),
    };
    Ok(vec![format!("{address} {key}={written}")])
}

/// The edit that keeps `new`, an image replacing `old` on the layer at
/// `address`, inside the old one's box at its own aspect.
fn fit_image(master: &Session, address: &str, old: &str, new: &str, base: &Path) -> Option<String> {
    let folder = master
        .path()
        .and_then(Path::parent)
        .unwrap_or(Path::new(""));
    let (ow, oh) = image_size(&folder.join(old))?;
    let (nw, nh) = image_size(&base.join(new)).or_else(|| image_size(&folder.join(new)))?;
    let scale = (ow / nw).min(oh / nh);
    let (w, h) = (nw * scale, nh * scale);
    if ((w - ow).abs() < 0.5) && ((h - oh).abs() < 0.5) {
        return None;
    }
    // The box keeps its middle: it starts as far in as the margin left.
    Some(format!(
        "{address} box=[{},{},{},{}]",
        round((ow - w) / 2.0),
        round((oh - h) / 2.0),
        round(w),
        round(h)
    ))
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The size in px of a PNG or JPEG file, read from its header.
fn image_size(path: &Path) -> Option<(f64, f64)> {
    let bytes = fs::read(path).ok()?;
    if bytes.starts_with(b"\x89PNG") && bytes.len() >= 24 {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some((f64::from(w), f64::from(h)));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            let len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]);
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]);
                return Some((f64::from(w), f64::from(h)));
            }
            i += 2 + len;
        }
    }
    None
}

#[cfg(test)]
#[path = "../../tests/unit/vary/content.rs"]
mod tests;
