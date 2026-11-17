//! The `matte` shorthand, `"reveal alpha-inverted"`: the layer whose drawing
//! decides where this one shows, and how.

use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, parse_whole, word};
use crate::{Diagnostic, Result};
use winnow::combinator::{delimited, opt, preceded};
use winnow::prelude::*;

use crate::doc::Form;

pub(crate) const EXPECTED: &str =
    "`<layer id> [alpha|alpha-inverted|luma|luma-inverted]`, e.g. `reveal alpha`";

/// How a layer uses its matte layer's drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatteMode {
    /// It shows where the matte layer is opaque.
    #[default]
    Alpha,
    /// It shows where the matte layer is transparent.
    AlphaInverted,
    /// It shows where the matte layer is bright.
    Luma,
    /// It shows where the matte layer is dark.
    LumaInverted,
}

/// Every matte mode by the name scene files write.
pub const MATTE_MODES: [(&str, MatteMode); 4] = [
    ("alpha", MatteMode::Alpha),
    ("alpha-inverted", MatteMode::AlphaInverted),
    ("luma", MatteMode::Luma),
    ("luma-inverted", MatteMode::LumaInverted),
];

impl MatteMode {
    /// The name scene files write.
    pub fn name(self) -> &'static str {
        MATTE_MODES
            .iter()
            .find(|(_, mode)| *mode == self)
            .map_or("alpha", |(name, _)| name)
    }
}

/// A layer's matte: the layer of its own list whose drawing is the stencil,
/// and how the stencil is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matte {
    /// The matte layer's id.
    pub layer: String,
    /// How its drawing is read; `alpha` when not written.
    pub mode: MatteMode,
}

impl FromStr for Matte {
    type Err = Diagnostic;

    /// Parses `<layer id> [<mode>]`.
    fn from_str(text: &str) -> Result<Matte> {
        let (layer, mode) = parse_whole(text, matte, "matte", EXPECTED)?;
        let mode = match mode {
            None => MatteMode::Alpha,
            Some(name) => MATTE_MODES
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, mode)| *mode)
                .ok_or_else(|| {
                    Diagnostic::unknown("matte mode", name, MATTE_MODES.iter().map(|(n, _)| *n))
                })?,
        };
        Ok(Matte {
            layer: layer.to_owned(),
            mode,
        })
    }
}

impl Form for Matte {
    const FORM: &str = EXPECTED;
}

fn matte<'s>(input: &mut &'s str) -> Parsed<(&'s str, Option<&'s str>)> {
    delimited(blank0, (word, opt(preceded(blank1, word))), blank0).parse_next(input)
}
