//! Text geometry against the layout a project stored ([D35]): each imported
//! text layer's lines, as the scene lays them out, held line by line against
//! the lines the project laid the same text out in. Stored layout is exact,
//! so text is held to a stricter limit than a compressed reference allows,
//! and a gap shows in pixels on the line it is on rather than as a fringe.
//!
//! Positions are in the layer's own coordinates, where both layouts put
//! them: point text's first baseline starts at the origin, box text's box is
//! where the project put it.
//!
//! [D35]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d35-fidelity-is-measured-against-reference-renders

use motion_format::{Diagnostic, Kind, Result};
use motion_import::map::StoredText;
use motion_scene::{Node, NodeContent, SceneGraph};
use motion_text::TextLayout;
use serde::Serialize;

use crate::Session;

/// How far, in pixels, a line may be from the stored one before it fails:
/// a quarter pixel, under what anti-aliasing shows.
pub const TEXT_TOLERANCE: f64 = 0.25;

/// Every imported text layer's lines against the stored ones.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextGeometry {
    /// How far a line may be, in pixels, before it fails.
    pub tolerance: f64,
    /// One per stored text, in import order.
    pub texts: Vec<TextCheck>,
}

/// One text layer's lines against the stored ones.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextCheck {
    /// The scene composition holding the layer, or none for the scene's own.
    pub comp: Option<String>,
    /// The layer's id.
    pub layer: String,
    /// Line by line, as many as the longer layout has.
    pub lines: Vec<LineCheck>,
}

/// One line of both layouts; a side is missing when that layout has fewer
/// lines.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineCheck {
    /// The project's line.
    pub stored: Option<LineGeometry>,
    /// The scene's line.
    pub ours: Option<LineGeometry>,
}

/// Where a line sits, as both layouts can say it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineGeometry {
    /// How many characters (UTF-16 code units) it holds, a paragraph's end
    /// counting as one, as projects count them.
    pub characters: usize,
    /// Where its first character starts.
    pub start: f64,
    /// Its baseline's y.
    pub baseline: f64,
    /// How far its characters advance.
    pub width: f64,
    /// Each character's advance, cumulative from `start`; empty where the
    /// project stores none.
    pub advances: Vec<f64>,
}

impl LineCheck {
    /// The largest distance between the two lines, in pixels: of their
    /// starts, ends, baselines and shared advances; infinite when a side is
    /// missing or the lines break at other characters.
    pub fn off(&self) -> f64 {
        let (Some(stored), Some(ours)) = (&self.stored, &self.ours) else {
            return f64::INFINITY;
        };
        if stored.characters != ours.characters {
            return f64::INFINITY;
        }
        let advances = stored
            .advances
            .iter()
            .zip(&ours.advances)
            .map(|(a, b)| (a - b).abs());
        [
            (stored.start - ours.start).abs(),
            (stored.start + stored.width - ours.start - ours.width).abs(),
            (stored.baseline - ours.baseline).abs(),
        ]
        .into_iter()
        .chain(advances)
        .fold(0.0, f64::max)
    }

    /// The scene's width less the stored one, or none when a side is missing.
    pub fn width_gap(&self) -> Option<f64> {
        Some(self.ours.as_ref()?.width - self.stored.as_ref()?.width)
    }
}

impl TextCheck {
    /// The largest [`LineCheck::off`] of its lines; zero without lines.
    pub fn off(&self) -> f64 {
        self.lines.iter().map(LineCheck::off).fold(0.0, f64::max)
    }

    /// The line with the largest [`LineCheck::off`], counted from 0.
    pub fn worst_line(&self) -> Option<usize> {
        (0..self.lines.len()).max_by(|&a, &b| self.lines[a].off().total_cmp(&self.lines[b].off()))
    }
}

impl TextGeometry {
    /// The texts with a line past the tolerance.
    pub fn failing(&self) -> impl Iterator<Item = &TextCheck> {
        self.texts.iter().filter(|text| text.off() > self.tolerance)
    }
}

impl Session {
    /// Holds each of `stored`'s text layers, laid out by this scene, against
    /// the lines the project stored for it.
    ///
    /// # Errors
    /// As [`Session::graph`], or a missing-value diagnostic when a stored
    /// text names a layer the scene has not, or one that is not text.
    pub fn text_geometry(&self, stored: &[StoredText]) -> Result<TextGeometry> {
        let graph = self.graph()?;
        let texts = stored
            .iter()
            .map(|text| {
                let layout = layout_of(graph, text)?;
                Ok(TextCheck {
                    comp: text.comp.clone(),
                    layer: text.layer.clone(),
                    lines: lines(text, layout),
                })
            })
            .collect::<Result<_>>()?;
        Ok(TextGeometry {
            tolerance: TEXT_TOLERANCE,
            texts,
        })
    }
}

/// The scene's layout of the layer `text` names.
fn layout_of<'g>(graph: &'g SceneGraph, text: &StoredText) -> Result<&'g TextLayout> {
    let nodes: &[Node] = match &text.comp {
        None => &graph.nodes,
        Some(id) => graph
            .comps
            .iter()
            .find(|comp| &comp.id == id)
            .map(|comp| comp.nodes.as_slice())
            .ok_or_else(|| {
                Diagnostic::unknown("composition", id, graph.comps.iter().map(|c| c.id.as_str()))
            })?,
    };
    let node = nodes
        .iter()
        .find(|node| node.id == text.layer)
        .ok_or_else(|| {
            Diagnostic::unknown("layer", &text.layer, nodes.iter().map(|n| n.id.as_str()))
        })?;
    match &node.content {
        NodeContent::Text { text, .. } => Ok(&text.layout),
        _ => Err(Diagnostic::new(
            Kind::Missing,
            format!("layer `{}` holds no text to check", text.layer),
        )),
    }
}

/// Both layouts' lines, side by side.
fn lines(text: &StoredText, layout: &TextLayout) -> Vec<LineCheck> {
    let stored: Vec<LineGeometry> = text
        .lines
        .iter()
        .map(|line| {
            let start = line
                .segments
                .first()
                .map_or(0.0, |segment| segment.origin[0]);
            LineGeometry {
                characters: line.segments.iter().map(|s| s.characters).sum(),
                start: line.origin[0] + start,
                baseline: line.baseline,
                width: line.segments.iter().map(|s| s.width).sum(),
                // Cumulative from the line's start already, as ours are.
                advances: line
                    .segments
                    .iter()
                    .flat_map(|s| s.advances.iter().copied())
                    .collect(),
            }
        })
        .collect();
    let ours: Vec<LineGeometry> = layout
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let ends_paragraph = layout
                .lines
                .get(i + 1)
                .is_none_or(|next| next.paragraph != line.paragraph);
            LineGeometry {
                characters: line.characters + usize::from(ends_paragraph),
                start: line.x,
                baseline: line.baseline,
                width: line.width,
                advances: line.advances.clone(),
            }
        })
        .collect();
    (0..stored.len().max(ours.len()))
        .map(|i| LineCheck {
            stored: stored.get(i).cloned(),
            ours: ours.get(i).cloned(),
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/geometry.rs"]
mod tests;
