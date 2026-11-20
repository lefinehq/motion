//! Fitting a text to the size laid out for it ([`Fit`]): the largest scale
//! of every size at which the lines fit, found by bisection on the real
//! layout, since line breaking and kerning do not follow the size linearly.
//!
//! A text that fits as written is that layout, untouched, so fitting moves
//! nothing until a text is too large.

use kurbo::Rect;
use nakkash_format::Result;

use crate::layout::TextLayout;
use crate::style::{Fit, StyledText, TextFrame};
use crate::typeset::Typesetter;

/// How close the scale found comes to the largest that fits, as a fraction
/// of it: the search stops once the scale that fits and the one that does
/// not are this close.
pub(crate) const PRECISION: f64 = 0.001;

/// How far, in pixels, lines may pass the size they fit and still fit, so
/// a size written from the lines' own [`TextLayout::size`], rounded, never
/// shrinks them.
const SLACK: f64 = 1e-6;

/// How many times the first guess may halve when even it does not fit; past
/// that the smallest scale tried is taken.
const HALVINGS: u32 = 40;

/// What a fit asks of a layout, and which point of the lines as written a
/// scaled layout keeps.
struct Target {
    /// Where point text breaks its lines, in pixels.
    wrap: Option<f64>,
    /// The widest the lines may be.
    width: f64,
    /// The tallest the lines may be.
    height: f64,
    /// What a scaled layout keeps of the written one.
    keep: Keep,
}

/// What a scaled layout keeps of the lines as written.
#[derive(Clone, Copy)]
enum Keep {
    /// The vertical middle of their extent, from the first line's ascent to
    /// the last line's descent.
    Middle,
    /// The top of that extent.
    Top,
    /// Nothing: box text is placed in its box already.
    Placed,
}

impl Target {
    /// The target of `fit` on text in `frame`, which [`StyledText::check`]
    /// has matched to it.
    fn of(fit: Fit, frame: TextFrame) -> Target {
        match (fit, frame) {
            (Fit::Shrink { width, height }, _) => Target {
                wrap: None,
                width,
                height,
                keep: Keep::Middle,
            },
            (Fit::Wrap { width, height }, _) => Target {
                wrap: Some(width),
                width,
                height,
                keep: Keep::Top,
            },
            (Fit::Box, TextFrame::Box { width, height, .. }) => Target {
                wrap: None,
                width,
                height,
                keep: Keep::Placed,
            },
            // Checked away; a box fit on point text has no size to fit, so
            // nothing is too large for it.
            (Fit::Box, TextFrame::Point) => Target {
                wrap: None,
                width: f64::INFINITY,
                height: f64::INFINITY,
                keep: Keep::Placed,
            },
        }
    }

    /// Whether `layout`'s lines fit: whether the size of their
    /// [`TextLayout::bounds`] is within the target's, give or take [`SLACK`].
    fn holds(&self, layout: &TextLayout) -> bool {
        let size = layout.size();
        size.width <= self.width + SLACK && size.height <= self.height + SLACK
    }

    /// The scale at which a layout of `size` would just fit if it scaled
    /// linearly, the search's first guess; below 1 for a size that does not
    /// fit.
    fn guess(&self, layout: &TextLayout) -> f64 {
        let size = layout.size();
        let ratio = |limit: f64, length: f64| match length > 0.0 {
            true => limit / length,
            false => f64::INFINITY,
        };
        let guess = ratio(self.width, size.width).min(ratio(self.height, size.height));
        match guess.is_finite() && guess > 0.0 {
            true => guess.min(1.0),
            false => 0.5,
        }
    }
}

impl Keep {
    /// How far down a scaled layout whose lines cover `scaled` moves to keep
    /// what it keeps of `written`.
    fn offset(self, written: Rect, scaled: Rect) -> f64 {
        match self {
            Keep::Middle => written.center().y - scaled.center().y,
            Keep::Top => written.y0 - scaled.y0,
            Keep::Placed => 0.0,
        }
    }
}

impl Typesetter {
    /// Lays out `text`, checked, to fit as `fit` says: as written when it
    /// fits, else with every size scaled by the largest factor below 1 that
    /// makes its lines fit, to within [`PRECISION`], and moved to keep the
    /// middle or the top of its lines as written.
    pub(crate) fn fitted(&mut self, text: &StyledText, fit: Fit) -> Result<TextLayout> {
        let target = Target::of(fit, text.frame);
        let written = self.set(text, target.wrap)?;
        if target.holds(&written) {
            return Ok(written);
        }
        let (scale, mut layout) = self.largest_fitting(text, &target, &written)?;
        let dy = target.keep.offset(written.bounds(), layout.bounds());
        if dy != 0.0 {
            layout.shift_down(dy);
        }
        layout.scale = scale;
        Ok(layout)
    }

    /// Lays out `text`, checked, as its fit sets it at `scale` instead of
    /// the scale it would find on its own: every size scaled by `scale`,
    /// and the lines moved to keep what the fit keeps of them as written.
    /// Several texts that take turns in one place, such as the keys of a
    /// layer's words, share the smallest of their own scales this way, so
    /// none of them is set larger than another would be. At a `scale` of 1
    /// or more, or without a fit, it is [`Typesetter::layout`].
    ///
    /// # Errors
    /// As [`Typesetter::layout`].
    pub fn layout_at(&mut self, text: &StyledText, scale: f64) -> Result<TextLayout> {
        let Some(fit) = text.fit.filter(|_| scale < 1.0) else {
            return self.layout(text);
        };
        text.check()?;
        let target = Target::of(fit, text.frame);
        let written = self.set(text, target.wrap)?;
        let mut layout = self.set(&text.scaled(scale), target.wrap)?;
        let dy = target.keep.offset(written.bounds(), layout.bounds());
        if dy != 0.0 {
            layout.shift_down(dy);
        }
        layout.scale = scale;
        Ok(layout)
    }

    /// The largest scale, to within [`PRECISION`], at which `text` fits
    /// `target`, and its layout. `written`, the layout at scale 1, does not
    /// fit. A guess from its size is halved until it fits, then the scale
    /// is bisected between the largest that fits and the smallest that
    /// does not, so the search is the same every time.
    fn largest_fitting(
        &mut self,
        text: &StyledText,
        target: &Target,
        written: &TextLayout,
    ) -> Result<(f64, TextLayout)> {
        let mut too_large = 1.0;
        let mut scale = target.guess(written);
        let mut layout = self.set(&text.scaled(scale), target.wrap)?;
        for _ in 0..HALVINGS {
            if target.holds(&layout) {
                break;
            }
            too_large = scale;
            scale /= 2.0;
            layout = self.set(&text.scaled(scale), target.wrap)?;
        }
        while too_large - scale > PRECISION * scale {
            let middle = 0.5 * (scale + too_large);
            let tried = self.set(&text.scaled(middle), target.wrap)?;
            if target.holds(&tried) {
                (scale, layout) = (middle, tried);
            } else {
                too_large = middle;
            }
        }
        Ok((scale, layout))
    }
}

#[cfg(test)]
#[path = "../tests/unit/fit.rs"]
mod tests;
