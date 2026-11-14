//! The readable tree of a [`Project`] that `motion inspect` prints: items in
//! their folders, compositions with their layers, and each layer's property
//! tree with static values, keyframe counts and expressions. Groups with no
//! properties below them are left out; `--json` prints everything.
//!
//! Each model type prints its own line; [`Project`] lays the lines out.

use std::collections::HashMap;
use std::fmt;

use motion_format::fmt_num;

use crate::model::{
    Child, Composition, Footage, Group, ItemKind, Layer, Project, Property, Source, Stream,
    TextDocument, TextFrame, Value,
};

/// The width of one level of indentation.
const INDENT: usize = 2;

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let depth = self
            .bits_per_channel
            .map_or_else(|| "unknown".to_owned(), |bits| bits.to_string());
        writeln!(f, "project {}, {depth} bits per channel", self.version)?;
        let mut levels: HashMap<u32, usize> = HashMap::new();
        self.items.iter().try_for_each(|item| {
            let level = item
                .folder
                .and_then(|folder| levels.get(&folder))
                .map_or(0, |level| level + 1);
            levels.insert(item.id, level);
            let pad = level * INDENT;
            write!(f, "{:pad$}{} {} {:?}", "", item.kind, item.id, item.name)?;
            match &item.kind {
                ItemKind::Folder => writeln!(f),
                ItemKind::Footage(footage) => writeln!(f, " {footage}"),
                ItemKind::Composition(composition) => {
                    writeln!(f, " {composition}")?;
                    composition.layers.iter().try_for_each(|layer| {
                        writeln!(f, "{:pad$}{layer}", "", pad = pad + INDENT)?;
                        write_children(f, &layer.properties, level + 2)
                    })
                }
            }
        })
    }
}

/// The children of `group` that hold properties, at `level`.
fn write_children(f: &mut fmt::Formatter<'_>, group: &Group, level: usize) -> fmt::Result {
    let pad = level * INDENT;
    group.children.iter().try_for_each(|child| match child {
        Child::Group(group) if group.properties().next().is_none() => Ok(()),
        Child::Group(group) => {
            writeln!(
                f,
                "{:pad$}{}",
                "",
                Heading(&group.match_name, group.name.as_deref(), group.enabled)
            )?;
            write_children(f, group, level + 1)
        }
        Child::Property(property) => writeln!(f, "{:pad$}{property}", ""),
    })
}

/// A composition's size, rate, duration and work area.
impl fmt::Display for Composition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}x{}, {} fps, {}s, work area {}s-{}s",
            self.width,
            self.height,
            fmt_num(self.frame_rate),
            fmt_num(self.duration),
            fmt_num(self.work_area.start),
            fmt_num(self.work_area.end)
        )
    }
}

/// A footage item's source, size and timing.
impl fmt::Display for Footage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Source::Solid { color } => write!(
                f,
                "solid {}",
                Numbers(&[color.r, color.g, color.b, color.a])
            ),
            Source::File { path } => write!(f, "file {:?}", path.as_deref().unwrap_or("?")),
            Source::Placeholder => write!(f, "placeholder"),
        }?;
        write!(
            f,
            " {}x{}, {} fps, {}s",
            self.width,
            self.height,
            fmt_num(self.frame_rate),
            fmt_num(self.duration)
        )
    }
}

/// A layer's id, name, kind and time span, its links and the switches on.
impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "layer {} {:?} {}, {}s-{}s",
            self.id,
            self.name,
            self.kind,
            fmt_num(self.in_point),
            fmt_num(self.out_point)
        )?;
        let links = [
            ("source", self.source),
            ("parent", self.parent),
            ("matte", self.matte_layer),
        ];
        links
            .iter()
            .filter_map(|(name, id)| id.map(|id| (name, id)))
            .try_for_each(|(name, id)| write!(f, ", {name} {id}"))?;
        let s = self.switches;
        let switches = [
            ("off", !s.enabled),
            ("3d", s.three_d),
            ("solo", s.solo),
            ("shy", s.shy),
            ("guide", s.guide),
            ("motion blur", s.motion_blur),
            ("null", s.null),
            ("adjustment", s.adjustment),
        ];
        switches
            .iter()
            .filter(|(_, on)| *on)
            .try_for_each(|(name, _)| write!(f, ", {name}"))
    }
}

/// A property's match name, its value or keyframes, then its expression.
impl fmt::Display for Property {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            Heading(&self.match_name, self.name.as_deref(), self.enabled)
        )?;
        match &self.value {
            Stream::Static(value) => write!(f, " = {value}"),
            Stream::Animated(keyframes) => {
                write!(f, ": {} keyframes", keyframes.len())?;
                if let (Some(first), Some(last)) = (keyframes.first(), keyframes.last()) {
                    write!(f, " {}s-{}s", fmt_num(first.time), fmt_num(last.time))?;
                }
                match keyframes.first().map(|key| &key.value) {
                    Some(value @ (Value::Opaque { .. } | Value::Text(_))) => {
                        write!(f, ", first {value}")
                    }
                    _ => Ok(()),
                }
            }
        }?;
        let Some(expression) = &self.expression else {
            return Ok(());
        };
        let mut lines = expression.text.lines();
        let first = lines.next().unwrap_or_default();
        let more = if lines.next().is_some() { " ..." } else { "" };
        let off = if expression.enabled { "" } else { " (off)" };
        write!(f, ", expression{off} {first:?}{more}")
    }
}

/// A value: a number, `[1, 2.5]`, `rgba [...]`, `path closed, 4 vertices`
/// or `opaque gradient`.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(x) => f.write_str(&fmt_num(*x)),
            Value::Vector2(v) => write!(f, "{}", Numbers(v)),
            Value::Vector3(v) => write!(f, "{}", Numbers(v)),
            Value::Color(c) => write!(f, "rgba {}", Numbers(&[c.r, c.g, c.b, c.a])),
            Value::Path(path) => {
                let shape = if path.closed { "closed" } else { "open" };
                write!(f, "path {shape}, {} vertices", path.vertices.len())
            }
            Value::Text(document) => write!(f, "{document}"),
            Value::Opaque { reason } => write!(f, "opaque {reason}"),
        }
    }
}

/// A text document: its text, line count, frame, and the style its text
/// starts with, as `text "Hi" (1 line), Inter-Bold 44, fill [1, 1, 1, 1], center`.
impl fmt::Display for TextDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines = self.lines.len();
        let plural = if lines == 1 { "" } else { "s" };
        write!(f, "text {:?} ({lines} line{plural})", self.text)?;
        if let TextFrame::Box { width, height, .. } = self.frame {
            write!(f, ", box {}x{}", fmt_num(width), fmt_num(height))?;
        }
        if let Some(style) = self.characters.first().map(|run| &run.style) {
            write!(f, ", {} {}", style.font, fmt_num(style.size))?;
            let paints = [("fill", style.fill), ("stroke", style.stroke)];
            paints
                .iter()
                .filter_map(|(name, color)| color.map(|c| (name, c)))
                .try_for_each(|(name, c)| {
                    write!(f, ", {name} {}", Numbers(&[c.r, c.g, c.b, c.a]))
                })?;
        }
        match self.paragraphs.first() {
            Some(run) => write!(f, ", {}", run.style.justification),
            None => Ok(()),
        }
    }
}

/// A match name, with the user's name and an `(off)` mark when they apply.
struct Heading<'a>(&'a str, Option<&'a str>, bool);

impl fmt::Display for Heading<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Heading(match_name, name, enabled) = self;
        f.write_str(match_name)?;
        if let Some(name) = name {
            write!(f, " {name:?}")?;
        }
        if !enabled {
            f.write_str(" (off)")?;
        }
        Ok(())
    }
}

/// Numbers as `[1, 2.5, 0]`.
struct Numbers<'a>(&'a [f64]);

impl fmt::Display for Numbers<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(|&n| fmt_num(n)).collect();
        write!(f, "[{}]", parts.join(", "))
    }
}

#[cfg(test)]
#[path = "../tests/unit/tree.rs"]
mod tests;
