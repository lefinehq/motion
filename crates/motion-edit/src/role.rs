//! Edits of a role ([D50]): a line whose target is `@headline` edits what
//! the role names, a layer, `@headline text="Sale"`, or a slot,
//! `@accent=#2A9D8F`.
//!
//! [D50]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d50-roles-name-what-a-masters-layers-do

use std::borrow::Cow;

use motion_format::{Diagnostic, Document, Id, Kind, Result, RoleTarget};

/// `line` with a role target written as what the role names in `doc`:
/// `@headline text=…` becomes `title text=…`, and `@accent=…` becomes
/// `$accent=…`. Any other line is itself.
///
/// # Errors
/// An unknown-name diagnostic for a role the scene does not have, and an
/// invalid one for a slot's role given keys or a layer's role given a
/// value.
pub fn resolve_role<'l>(doc: &Document, line: &'l str) -> Result<Cow<'l, str>> {
    let Some(after) = line.trim_start().strip_prefix('@') else {
        return Ok(Cow::Borrowed(line));
    };
    let end = after
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(after.len());
    let (name, rest) = after.split_at(end);
    let scene = doc.scene();
    let Some(role) = Id::is_valid(name).then(|| scene.role(name)).flatten() else {
        let known = scene.roles.iter().map(|r| format!("@{}", r.id));
        return Err(Diagnostic::unknown("role", format!("@{name}"), known));
    };
    match (&role.target, rest.starts_with('=')) {
        (RoleTarget::Layer(address), false) => Ok(Cow::Owned(format!("{address}{rest}"))),
        (RoleTarget::Slot(slot), true) => Ok(Cow::Owned(format!("${slot}{rest}"))),
        (RoleTarget::Layer(address), true) => Err(Diagnostic::new(
            Kind::Invalid,
            format!("role `@{name}` is the layer `{address}`, which takes keys, not a value"),
        )
        .expected(format!(
            "`@{name} <key>=<value>`, such as `@{name} text=\"Sale\"`"
        ))),
        (RoleTarget::Slot(slot), false) => Err(Diagnostic::new(
            Kind::Invalid,
            format!("role `@{name}` is the slot `${slot}`, which takes a value, not keys"),
        )
        .expected(format!("`@{name}=<value>`"))),
    }
}

/// `doc` with the edit `line` made: its role resolved, read, and applied.
///
/// # Errors
/// As [`resolve_role`], [`crate::parse_edit`] and [`crate::apply_edit`].
pub fn edit_line(doc: &Document, line: &str) -> Result<Document> {
    let line = resolve_role(doc, line)?;
    crate::apply_edit(doc, &crate::parse_edit(&line)?)
}

#[cfg(test)]
#[path = "../tests/unit/role.rs"]
mod tests;
