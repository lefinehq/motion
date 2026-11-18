//! The text `fit` shorthand, `"shrink 600x300"`: how a text layer's words
//! are made to fit the size laid out for them, so longer ones stay inside
//! it ([D77]).
//!
//! [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary

use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, number, parse_whole};
use crate::{Diagnostic, Result};
use winnow::combinator::{alt, delimited, opt, preceded, separated_pair};
use winnow::prelude::*;

pub(crate) const EXPECTED: &str = "`shrink <width>x<height>` or `wrap <width>x<height>` for point text, `shrink` for box text, e.g. `shrink 600x300`";

/// How a text is made to fit the size laid out for it. A text never grows:
/// one that fits is set as written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextFit {
    /// `shrink <width>x<height>`, for point text: lines wider than `width`
    /// or taller than `height` scale every size down until they fit, about
    /// the vertical middle of the lines as written.
    Shrink {
        /// The widest the lines may be, in px, above zero.
        width: f64,
        /// The tallest the lines may be, in px, above zero.
        height: f64,
    },
    /// `wrap <width>x<height>`, for point text: lines break at `width`, as
    /// box text's do, each aligned about the origin; the first baseline
    /// stays, and text taller than `height` shrinks, keeping its top.
    Wrap {
        /// Where the lines break, in px, above zero.
        width: f64,
        /// The tallest the lines may be, in px, above zero.
        height: f64,
    },
    /// `shrink`, for box text: every size scales down until the wrapped
    /// lines fit the box.
    ShrinkToBox,
}

impl TextFit {
    /// The written form: `shrink 600x300`, `wrap 600x300` or `shrink`.
    pub fn written(self) -> String {
        let size = |width: f64, height: f64| {
            format!("{}x{}", crate::fmt_num(width), crate::fmt_num(height))
        };
        match self {
            TextFit::Shrink { width, height } => format!("shrink {}", size(width, height)),
            TextFit::Wrap { width, height } => format!("wrap {}", size(width, height)),
            TextFit::ShrinkToBox => "shrink".to_owned(),
        }
    }
}

impl FromStr for TextFit {
    type Err = Diagnostic;

    /// Parses `shrink [<width>x<height>]` or `wrap <width>x<height>`.
    fn from_str(text: &str) -> Result<TextFit> {
        parse_whole(text, fit, "fit", EXPECTED)
    }
}

fn fit(input: &mut &str) -> Parsed<TextFit> {
    delimited(blank0, alt((wrap, shrink)), blank0).parse_next(input)
}

fn wrap(input: &mut &str) -> Parsed<TextFit> {
    preceded(("wrap", blank1), size)
        .map(|(width, height)| TextFit::Wrap { width, height })
        .parse_next(input)
}

fn shrink(input: &mut &str) -> Parsed<TextFit> {
    preceded("shrink", opt(preceded(blank1, size)))
        .map(|size| match size {
            Some((width, height)) => TextFit::Shrink { width, height },
            None => TextFit::ShrinkToBox,
        })
        .parse_next(input)
}

fn size(input: &mut &str) -> Parsed<(f64, f64)> {
    separated_pair(side, 'x', side).parse_next(input)
}

fn side(input: &mut &str) -> Parsed<f64> {
    number.verify(|v: &f64| *v > 0.0).parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/fit.rs"]
mod tests;
