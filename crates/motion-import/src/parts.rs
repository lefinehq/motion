//! The parts of a text an expression selector chooses: which count it uses
//! and how many parts the layer's text has in it, as the bake evaluates its
//! amount once per part and the map writes one `part` item per part
//! ([D64]).
//!
//! The counts are the scene's: characters other than line breaks, those
//! other than spaces, words between spaces and line breaks, and lines as the
//! project last laid the text out (its paragraphs when it stores no layout).
//!
//! [D64]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d64-a-selector-may-choose-each-part-by-its-own-amount

use crate::model::{Child, Group, Property, Stream, TextDocument, Value};

/// The match name of its amount, the property its expression is on.
pub(crate) const AMOUNT: &str = "ADBE Text Expressible Amount";
/// The match name of the group that holds an animator's selectors.
const SELECTORS: &str = "ADBE Text Selectors";
/// The match name of what a selector counts.
const BASED_ON: &str = "ADBE Text Range Type2";
/// The match name of a text layer's document.
const DOCUMENT: &str = "ADBE Text Document";

/// The counts a selector may use, in the order of their stored values 1 to 4.
pub(crate) const COUNTS: [&str; 4] = ["char", "char-no-space", "word", "line"];

/// What an expression selector counts, and how many parts there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counted {
    /// The count, one of [`COUNTS`].
    pub by: &'static str,
    /// How many parts the layer's text has in it.
    pub total: usize,
}

/// What the expression selector holding `amount` counts, in `text`, a text
/// layer's property tree or its text properties, or why its parts cannot be
/// known at import.
pub(crate) fn count(text: &Group, amount: &Property) -> Result<Counted, String> {
    let (selectors, selector) = find(text, amount)
        .ok_or_else(|| "its expression selector could not be found".to_owned())?;
    // `selectorValue` reads what the selectors before it choose, which the
    // bake can only know when there are none.
    let first = selectors.children.iter().find_map(|child| match child {
        Child::Group(group) if !group.hidden => Some(group),
        _ => None,
    });
    if !first.is_some_and(|group| std::ptr::eq(group, selector)) {
        return Err(
            "its expression selector comes after other selectors, whose choice it \
                    replaces and reads as `selectorValue`"
                .into(),
        );
    }
    let based_on = selector
        .properties()
        .find(|p| p.match_name == BASED_ON)
        .map_or(Ok(1.0), |p| match &p.value {
            Stream::Static(Value::Number(n)) => Ok(*n),
            _ => Err("what its expression selector counts is animated".to_owned()),
        })?;
    let by = COUNTS
        .iter()
        .zip(1_u8..)
        .find(|(_, stored)| f64::from(*stored) == based_on)
        .map(|(by, _)| *by)
        .ok_or_else(|| format!("its expression selector counts by {based_on}, which is unknown"))?;
    let document = document(text)?;
    Ok(Counted {
        by,
        total: total(document, by),
    })
}

/// How many parts of the count `by` `document` has.
pub(crate) fn total(document: &TextDocument, by: &str) -> usize {
    let text = &document.text;
    let breaks = |c: char| c == '\n' || c == '\u{2028}';
    match by {
        "char" => text.chars().filter(|&c| !breaks(c)).count(),
        "char-no-space" => text.chars().filter(|c| !c.is_whitespace()).count(),
        "word" => text
            .split(char::is_whitespace)
            .filter(|w| !w.is_empty())
            .count(),
        _ if document.lines.is_empty() => text.split(breaks).count(),
        _ => document.lines.len(),
    }
}

/// The selectors group and the selector in it that hold `amount`.
fn find<'g>(group: &'g Group, amount: &Property) -> Option<(&'g Group, &'g Group)> {
    group.children.iter().find_map(|child| match child {
        Child::Group(selectors) if selectors.match_name == SELECTORS => {
            selectors.children.iter().find_map(|child| match child {
                Child::Group(selector)
                    if selector.properties().any(|p| std::ptr::eq(p, amount)) =>
                {
                    Some((selectors, selector))
                }
                _ => None,
            })
        }
        Child::Group(inner) => find(inner, amount),
        Child::Property(_) => None,
    })
}

/// The layer's text document, as the scene keeps it: its first.
fn document(group: &Group) -> Result<&TextDocument, String> {
    let property = group
        .properties()
        .find(|p| p.match_name == DOCUMENT)
        .ok_or_else(|| "its layer has no text".to_owned())?;
    match &property.value {
        Stream::Static(Value::Text(document)) => Ok(document),
        Stream::Animated(keys) if keys.len() == 1 => match &keys[0].value {
            Value::Text(document) => Ok(document),
            _ => Err("its layer's text could not be read".into()),
        },
        Stream::Animated(_) => Err("its layer's text changes over time".into()),
        Stream::Static(_) => Err("its layer's text could not be read".into()),
    }
}

#[cfg(test)]
#[path = "../tests/unit/parts.rs"]
mod tests;
