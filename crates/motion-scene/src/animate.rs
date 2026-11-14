//! Text animators at one frame: how much each selector chooses each part of
//! the text, and what that does to each glyph.
//!
//! A part is a character, a character other than a space, a word or a line.
//! Every glyph knows which part of each count it belongs to; selectors give
//! each part an amount, and an animator applies its values scaled by it.

use kurbo::{Affine, Rect, Vec2};
use motion_format::item::props;
use motion_format::property::Prop;
use motion_format::text::Align;

use crate::state::ItemState;
use crate::text::TextNode;

/// The counts a selector can choose from, in the order of its `by` choices.
pub const COUNTS: [&str; 4] = ["char", "char-no-space", "word", "line"];

/// Which part of each count one glyph belongs to; `None` where a count
/// leaves the glyph out, as `word` does spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphParts {
    /// Its index in each of [`COUNTS`].
    pub index: [Option<usize>; 4],
}

/// Every glyph's parts, in the layout's run order, and how many parts each count has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parts {
    /// One per glyph, run by run.
    pub glyphs: Vec<GlyphParts>,
    /// How many parts each of [`COUNTS`] has.
    pub counts: [usize; 4],
}

impl Parts {
    /// The parts of every glyph of `node`.
    pub fn of(node: &TextNode) -> Parts {
        let text = &node.content.text;
        // For every byte that starts a character: its character, its
        // character other than spaces and its word, each counted from 0.
        let mut at_byte = vec![[None; 3]; text.len() + 1];
        let (mut chars, mut solid, mut words) = (0, 0, 0);
        let mut in_word = false;
        for (byte, c) in text.char_indices() {
            if c == '\n' || c == '\u{2028}' {
                in_word = false;
                continue;
            }
            let space = c.is_whitespace();
            if !space && !in_word {
                words += 1;
            }
            in_word = !space;
            at_byte[byte] = [
                Some(chars),
                (!space).then_some(solid),
                (!space).then(|| words - 1),
            ];
            chars += 1;
            solid += usize::from(!space);
        }
        let glyphs = node
            .layout
            .runs
            .iter()
            .flat_map(|run| {
                let at_byte = &at_byte;
                run.glyphs.iter().map(move |g| {
                    let [c, s, w] = at_byte[g.cluster.start];
                    GlyphParts {
                        index: [c, s, w, Some(run.line)],
                    }
                })
            })
            .collect();
        Parts {
            glyphs,
            counts: [chars, solid, words, node.layout.lines.len()],
        }
    }
}

/// One glyph at one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphState {
    /// From the glyph as laid out to where it is drawn, in the layer's coordinates.
    pub transform: Affine,
    /// How opaque it is, 0 to 1.
    pub opacity: f64,
    /// Its fill colour, straight RGBA from 0 to 1.
    pub fill: [f64; 4],
    /// Its stroke colour.
    pub stroke: [f64; 4],
    /// Its stroke width in px.
    pub stroke_width: f64,
}

/// An animator at one frame: its values, which of them it applies, and its selectors.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimatorState {
    /// Its values, one per row of its registry, and its selectors as items.
    pub item: ItemState,
    /// The properties it sets or animates; only these apply.
    pub applies: Vec<Prop>,
}

/// What one glyph accumulates from the animators before it is placed.
struct Effect {
    offset: Vec2,
    anchor: Vec2,
    scale: Vec2,
    rotate: f64,
    tracking: f64,
    state: GlyphState,
}

/// Every glyph of `node`, in the layout's run order, under `animators`.
pub fn glyph_states(
    node: &TextNode,
    parts: &Parts,
    animators: &[AnimatorState],
) -> Vec<GlyphState> {
    let mut effects: Vec<Effect> = node
        .layout
        .runs
        .iter()
        .enumerate()
        .flat_map(|(r, run)| {
            let style = node.style_of(r);
            run.glyphs.iter().map(move |_| Effect {
                offset: Vec2::ZERO,
                anchor: Vec2::ZERO,
                scale: Vec2::new(1.0, 1.0),
                rotate: 0.0,
                tracking: 0.0,
                state: GlyphState {
                    transform: Affine::IDENTITY,
                    opacity: 1.0,
                    fill: style.fill,
                    stroke: style.stroke,
                    stroke_width: style.stroke_width,
                },
            })
        })
        .collect();
    for animator in animators {
        for (effect, glyph) in effects.iter_mut().zip(&parts.glyphs) {
            let amount = chosen(&animator.item.items, glyph, &parts.counts);
            if amount != 0.0 {
                apply(animator, amount, effect);
            }
        }
    }
    place(node, &mut effects);
    effects.into_iter().map(|e| e.state).collect()
}

/// How much the selectors choose the part `glyph` belongs to, combined in
/// order by their modes, from -1 to 1.
fn chosen(selectors: &[ItemState], glyph: &GlyphParts, counts: &[usize; 4]) -> f64 {
    let Some(first) = selectors.first() else {
        return 0.0;
    };
    // Modes that remove from or narrow a choice start from everything chosen.
    let mut amount = match first.setting("mode") {
        "subtract" | "intersect" | "min" => 1.0,
        _ => 0.0,
    };
    for selector in selectors {
        let by = COUNTS
            .iter()
            .position(|c| *c == selector.setting("by"))
            .expect("every count has a row");
        let v = match glyph.index[by] {
            Some(i) => range(selector, i, counts[by]),
            None => 0.0,
        };
        amount = match selector.setting("mode") {
            "add" => amount + v,
            "subtract" => amount - v,
            "intersect" => amount * v,
            "min" => amount.min(v),
            "max" => amount.max(v),
            "difference" => (amount - v).abs(),
            mode => unreachable!("no mode `{mode}`"),
        };
    }
    amount.clamp(-1.0, 1.0)
}

/// How much a range selector chooses part `i` of `n`, its amount included.
fn range(selector: &ItemState, i: usize, n: usize) -> f64 {
    let offset = selector.number(props::RANGE_OFFSET);
    let a = selector.number(props::RANGE_START) + offset;
    let b = selector.number(props::RANGE_END) + offset;
    let (start, end) = (a.min(b), a.max(b));
    let n = n as f64;
    let (lo, hi) = (i as f64 / n, (i as f64 + 1.0) / n);
    let middle = (lo + hi) / 2.0;
    // Where the part's middle falls across the range, 0 at its start and 1 at its end.
    let across = match end - start {
        0.0 => f64::from(u8::from(middle >= end)),
        width => (middle - start) / width,
    };
    let inside = (0.0..=1.0).contains(&across) && end > start;
    let bell = |f: f64| if inside { f } else { 0.0 };
    let v = match selector.setting("shape") {
        "square" => {
            let cover = ((hi.min(end) - lo.max(start)) / (hi - lo)).max(0.0);
            match selector.number(props::RANGE_SMOOTH) {
                0.0 => f64::from(u8::from(middle >= start && middle < end)),
                smooth => (0.5 + (cover - 0.5) / smooth).clamp(0.0, 1.0),
            }
        }
        "ramp-up" => across.clamp(0.0, 1.0),
        "ramp-down" => 1.0 - across.clamp(0.0, 1.0),
        "triangle" => bell(1.0 - (2.0 * across - 1.0).abs()),
        "round" => bell((1.0 - (2.0 * across - 1.0).powi(2)).max(0.0).sqrt()),
        "smooth" => bell((1.0 - (std::f64::consts::TAU * across).cos()) / 2.0),
        shape => unreachable!("no shape `{shape}`"),
    };
    v * selector.number(props::RANGE_AMOUNT)
}

/// Applies `animator`'s properties to one glyph, chosen by `amount`.
fn apply(animator: &AnimatorState, amount: f64, effect: &mut Effect) {
    let item = &animator.item;
    let toward = |from: f64, to: f64| from + (to - from) * amount;
    // Colours and widths blend, so a negative choice cannot overshoot them.
    let blend = amount.clamp(0.0, 1.0);
    let mix =
        |from: [f64; 4], to: [f64; 4]| std::array::from_fn(|c| from[c] + (to[c] - from[c]) * blend);
    for &prop in &animator.applies {
        let vec = || {
            let [x, y] = item.point(prop);
            Vec2::new(x, y)
        };
        match prop {
            p if p == props::ANIMATOR_POSITION => effect.offset += vec() * amount,
            p if p == props::ANIMATOR_ANCHOR => effect.anchor += vec() * amount,
            p if p == props::ANIMATOR_SCALE => {
                let s = vec();
                effect.scale = Vec2::new(
                    effect.scale.x * toward(1.0, s.x),
                    effect.scale.y * toward(1.0, s.y),
                );
            }
            p if p == props::ANIMATOR_ROTATE => effect.rotate += item.number(prop) * amount,
            p if p == props::ANIMATOR_OPACITY => {
                effect.state.opacity =
                    (effect.state.opacity * toward(1.0, item.number(prop))).clamp(0.0, 1.0);
            }
            p if p == props::ANIMATOR_FILL => {
                effect.state.fill = mix(effect.state.fill, item.color(prop))
            }
            p if p == props::ANIMATOR_STROKE => {
                effect.state.stroke = mix(effect.state.stroke, item.color(prop))
            }
            p if p == props::ANIMATOR_STROKE_WIDTH => {
                let w = effect.state.stroke_width;
                effect.state.stroke_width = w + (item.number(prop) - w) * blend;
            }
            p if p == props::ANIMATOR_TRACKING => effect.tracking += item.number(prop) * amount,
            p => unreachable!("`{}` is not an animator property", p.name()),
        }
    }
}

/// Turns each glyph's accumulated effect into its transform: tracking
/// shifts the glyphs after it on its line, keeping the line's alignment,
/// and each glyph scales and turns about the middle of its baseline.
fn place(node: &TextNode, effects: &mut [Effect]) {
    // Each glyph's shift from the tracking before it on its line, and each line's total.
    let mut tracked = vec![0.0; node.layout.lines.len()];
    let mut shifts = Vec::with_capacity(effects.len());
    let mut at = 0;
    for run in &node.layout.runs {
        for _ in &run.glyphs {
            shifts.push(tracked[run.line]);
            // Tracking is in thousandths of an em of the run's size.
            tracked[run.line] += effects[at].tracking * run.size / 1000.0;
            at += 1;
        }
    }
    let mut at = 0;
    for run in &node.layout.runs {
        let line = &node.layout.lines[run.line];
        let align = node.content.paragraphs[line.paragraph].align;
        let total = tracked[run.line];
        let realign = match align {
            Align::Center | Align::JustifyCenter => -total / 2.0,
            Align::Right | Align::JustifyRight => -total,
            _ => 0.0,
        };
        for glyph in &run.glyphs {
            let effect = &mut effects[at];
            let pivot = Vec2::new(glyph.x + glyph.advance / 2.0, glyph.y) + effect.anchor;
            effect.state.transform =
                Affine::translate(effect.offset + Vec2::new(shifts[at] + realign, 0.0))
                    * Affine::translate(pivot)
                    * Affine::rotate(effect.rotate.to_radians())
                    * Affine::scale_non_uniform(effect.scale.x, effect.scale.y)
                    * Affine::translate(-pivot);
            at += 1;
        }
    }
}

/// The box the visible glyphs cover in the layer's coordinates: each glyph's
/// advance across and its line's ascent and descent, through its transform.
pub fn bounds(node: &TextNode, glyphs: &[GlyphState]) -> Rect {
    let mut states = glyphs.iter();
    node.layout
        .runs
        .iter()
        .flat_map(|run| {
            let line = &node.layout.lines[run.line];
            run.glyphs.iter().map(move |g| {
                Rect::new(g.x, g.y - line.ascent, g.x + g.advance, g.y + line.descent)
            })
        })
        .filter_map(|cell| {
            let state = states.next().expect("one state per glyph");
            (state.opacity > 0.0).then(|| state.transform.transform_rect_bbox(cell))
        })
        .reduce(|a, b| a.union(b))
        .unwrap_or(Rect::ZERO)
}

#[cfg(test)]
#[path = "../tests/unit/animate.rs"]
mod tests;
