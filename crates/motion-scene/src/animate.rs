//! Text animators at one frame: how much each selector chooses each part of
//! the text, and what that does to each glyph.
//!
//! A part is a character, a character other than a space, a word or a line.
//! Every glyph knows which part of each count it belongs to; selectors give
//! each part an amount, and an animator applies its values scaled by it.

use kurbo::{Affine, Point, Rect, Vec2};
use motion_format::item::props;
use motion_format::property::Prop;
use motion_format::text::{Align, AnchorBy};

use motion_format::Curve;

use crate::curve::Ease;
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
    /// How much it blurs across and down, in px of blurriness.
    pub blur: [f64; 2],
}

/// An animator at one frame: its values, which of them it applies, and its selectors.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimatorState {
    /// Its values, one per row of its registry, and its selectors as items.
    pub item: ItemState,
    /// The properties it sets or animates; only these apply.
    pub applies: Vec<Prop>,
    /// When it staggers, the animator as each part sees it in its turn.
    pub turns: Option<Turns>,
}

/// A staggered animator, as each part of one count sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Turns {
    /// The count whose parts take turns, an index into [`COUNTS`].
    pub by: usize,
    /// The animator, selectors included, as part `k` sees it: as it was
    /// its wait earlier.
    pub parts: Vec<ItemState>,
}

impl AnimatorState {
    /// The animator as the part `glyph` belongs to sees it: in its turn when
    /// it staggers, else as it is. A glyph in no part of the count, such as a
    /// space under `word`, sees it as it is.
    fn seen_by(&self, glyph: &GlyphParts) -> &ItemState {
        self.turns
            .as_ref()
            .and_then(|turns| glyph.index[turns.by].and_then(|k| turns.parts.get(k)))
            .unwrap_or(&self.item)
    }
}

/// How many turns each of `n` parts waits in `order`: `first` waits `k`
/// turns for part `k`, `last` the reverse, `center` the distance from the
/// middle part (half a turn for the two middle parts of an even count),
/// `edges` the distance from the nearer end, and `random` a shuffle of the
/// reading order picked by `seed`.
///
/// # Panics
/// If `order` is not one of the animator's `order` choices.
pub fn waits(order: &str, seed: f64, n: usize) -> Vec<f64> {
    let middle = (n as f64 - 1.0) / 2.0;
    let index = (0..n).map(|k| k as f64);
    match order {
        "first" => index.collect(),
        "last" => index.map(|k| n as f64 - 1.0 - k).collect(),
        "center" => index.map(|k| (k - middle).abs()).collect(),
        "edges" => index.map(|k| middle - (k - middle).abs()).collect(),
        "random" => shuffled(n, seed).into_iter().map(|k| k as f64).collect(),
        order => unreachable!("no order `{order}`"),
    }
}

/// `0..n` in an order picked by `seed`: Fisher-Yates driven by SplitMix64,
/// so every platform and every render agree.
fn shuffled(n: usize, seed: f64) -> Vec<usize> {
    let mut state = seed.to_bits();
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        // `i + 1` is at most `n`, so the remainder fits a `usize`.
        let j = usize::try_from(next() % (i as u64 + 1)).expect("below n");
        order.swap(i, j);
    }
    order
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
                    blur: [0.0, 0.0],
                },
            })
        })
        .collect();
    for animator in animators {
        for (effect, glyph) in effects.iter_mut().zip(&parts.glyphs) {
            let item = animator.seen_by(glyph);
            let amount = chosen(&item.items, glyph, &parts.counts);
            if amount != Vec2::ZERO {
                apply(item, &animator.applies, amount, effect);
            }
        }
    }
    place(node, parts, &mut effects);
    effects.into_iter().map(|e| e.state).collect()
}

/// How much the selectors choose the part `glyph` belongs to, across and
/// down, combined in order by their modes, each from -1 to 1: a `range` by
/// its shape, the same both ways, and a `parts` selector by its item for
/// that part.
fn chosen(selectors: &[ItemState], glyph: &GlyphParts, counts: &[usize; 4]) -> Vec2 {
    let Some(first) = selectors.first() else {
        return Vec2::ZERO;
    };
    // Modes that remove from or narrow a choice start from everything chosen.
    let start = match first.setting("mode") {
        "subtract" | "intersect" | "min" => 1.0,
        _ => 0.0,
    };
    let mut amount = [start, start];
    for selector in selectors {
        let by = COUNTS
            .iter()
            .position(|c| *c == selector.setting("by"))
            .expect("every count has a row");
        let v = match glyph.index[by] {
            Some(i) if selector.kind.name == "parts" => selector
                .items
                .get(i)
                .map_or([0.0; 2], |part| part.point(props::PART_AMOUNT)),
            Some(i) => [range(selector, i, counts[by]); 2],
            None => [0.0; 2],
        };
        for (a, v) in amount.iter_mut().zip(v) {
            *a = match selector.setting("mode") {
                "add" => *a + v,
                "subtract" => *a - v,
                "intersect" => *a * v,
                "min" => a.min(v),
                "max" => a.max(v),
                "difference" => (*a - v).abs(),
                mode => unreachable!("no mode `{mode}`"),
            };
        }
    }
    Vec2::new(amount[0].clamp(-1.0, 1.0), amount[1].clamp(-1.0, 1.0))
}

/// How much a range selector chooses part `i` of `n`, its amount included.
fn range(selector: &ItemState, i: usize, n: usize) -> f64 {
    let n = n as f64;
    // In `index` units the range counts parts; as fractions, it is the same
    // range over `n` parts.
    let per = match selector.setting("units") {
        "index" => n,
        _ => 1.0,
    };
    let offset = selector.number(props::RANGE_OFFSET) / per;
    let a = selector.number(props::RANGE_START) / per + offset;
    let b = selector.number(props::RANGE_END) / per + offset;
    let (start, end) = (a.min(b), a.max(b));
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
    let high = selector.number(props::RANGE_EASE_HIGH);
    let low = selector.number(props::RANGE_EASE_LOW);
    eased(v, high, low) * selector.number(props::RANGE_AMOUNT)
}

/// A shape's choice `v`, 0 to 1, eased by `high` and `low`, each -1 to 1: a
/// cubic from 0 to 1 whose first handle lies `low` along the bottom when
/// positive and `-low` up the start when negative, and whose second lies
/// `high` back along the top or `-high` down the end. Positive values slow
/// the choice where it leaves 0 (`low`) or nears 1 (`high`); negative values
/// speed it there.
fn eased(v: f64, high: f64, low: f64) -> f64 {
    if high == 0.0 && low == 0.0 {
        return v;
    }
    let (x1, y1) = if low >= 0.0 { (low, 0.0) } else { (0.0, -low) };
    let (x2, y2) = if high >= 0.0 {
        (1.0 - high, 1.0)
    } else {
        (1.0, 1.0 + high)
    };
    Ease::from_curve(Curve::Bezier(x1, y1, x2, y2), 1.0, 1.0).sample(0.0, 1.0, v)
}

/// Applies the properties of the animator `item` that `applies` lists to
/// one glyph, chosen by `amounts` across and down: offsets and scales take
/// each axis its own, every other value the first.
fn apply(item: &ItemState, applies: &[Prop], amounts: Vec2, effect: &mut Effect) {
    let amount = amounts.x;
    let toward = |from: f64, to: f64, amount: f64| from + (to - from) * amount;
    // Colours and widths blend, so a negative choice cannot overshoot them.
    let blend = amount.clamp(0.0, 1.0);
    let mix =
        |from: [f64; 4], to: [f64; 4]| std::array::from_fn(|c| from[c] + (to[c] - from[c]) * blend);
    for &prop in applies {
        let vec = || {
            let [x, y] = item.point(prop);
            Vec2::new(x, y)
        };
        match prop {
            p if p == props::ANIMATOR_POSITION => effect.offset += across(vec(), amounts),
            p if p == props::ANIMATOR_ANCHOR => effect.anchor += across(vec(), amounts),
            p if p == props::ANIMATOR_SCALE => {
                let s = vec();
                effect.scale = Vec2::new(
                    effect.scale.x * toward(1.0, s.x, amounts.x),
                    effect.scale.y * toward(1.0, s.y, amounts.y),
                );
            }
            p if p == props::ANIMATOR_ROTATE => effect.rotate += item.number(prop) * amount,
            p if p == props::ANIMATOR_OPACITY => {
                effect.state.opacity =
                    (effect.state.opacity * toward(1.0, item.number(prop), amount)).clamp(0.0, 1.0);
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
            p if p == props::ANIMATOR_BLUR => {
                let b = across(vec(), amounts);
                effect.state.blur = [effect.state.blur[0] + b.x, effect.state.blur[1] + b.y];
            }
            p if p == props::ANIMATOR_TRACKING => effect.tracking += item.number(prop) * amount,
            // When each part moves, not what it does.
            p if p == props::ANIMATOR_STAGGER || p == props::ANIMATOR_SEED => {}
            p => unreachable!("`{}` is not an animator property", p.name()),
        }
    }
}

/// `v` with each axis scaled by its own amount.
fn across(v: Vec2, amounts: Vec2) -> Vec2 {
    Vec2::new(v.x * amounts.x, v.y * amounts.y)
}

/// Turns each glyph's accumulated effect into its transform: tracking
/// shifts the glyphs after it on its line, keeping the line's alignment,
/// and each glyph scales and turns about its anchor ([`anchors`]).
fn place(node: &TextNode, parts: &Parts, effects: &mut [Effect]) {
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
    let anchors = anchors(node, parts);
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
        for _ in &run.glyphs {
            let effect = &mut effects[at];
            let pivot = anchors[at].to_vec2() + effect.anchor;
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

/// Every glyph's anchor, in the layout's run order: the middle of the
/// baseline of the part it takes its anchor from (`anchor-by`): itself,
/// its word, its line or the whole text, whose baseline is the first
/// glyph's. A glyph in no word, such as a space, takes its own. Each anchor
/// then moves by `anchor-align`: across by fractions of the part's width,
/// from its glyphs' first start to their last end, and down by fractions
/// of the size of the part's first glyph.
pub fn anchors(node: &TextNode, parts: &Parts) -> Vec<Point> {
    let whole = node.content.whole;
    let glyphs: Vec<(&motion_text::Glyph, f64)> = node
        .layout
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(move |g| (g, run.size)))
        .collect();
    // The part each glyph takes its anchor from, as a key shared by its glyphs.
    let part = |i: usize| -> (u8, usize) {
        let own = (0, i);
        match whole.anchor_by {
            AnchorBy::Char => own,
            AnchorBy::Word => parts.glyphs[i].index[2].map_or(own, |w| (1, w)),
            AnchorBy::Line => parts.glyphs[i].index[3].map_or(own, |l| (2, l)),
            AnchorBy::All => (3, 0),
        }
    };
    // Each part's extent across, its first glyph's baseline and size.
    let mut extents: std::collections::HashMap<(u8, usize), (f64, f64, f64, f64)> =
        std::collections::HashMap::new();
    for (i, (glyph, size)) in glyphs.iter().enumerate() {
        extents
            .entry(part(i))
            .and_modify(|(lo, hi, _, _)| {
                *lo = lo.min(glyph.x);
                *hi = hi.max(glyph.x + glyph.advance);
            })
            .or_insert((glyph.x, glyph.x + glyph.advance, glyph.y, *size));
    }
    let [ax, ay] = whole.anchor_align;
    (0..glyphs.len())
        .map(|i| {
            let (lo, hi, baseline, size) = extents[&part(i)];
            Point::new((lo + hi) / 2.0 + ax * (hi - lo), baseline + ay * size)
        })
        .collect()
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
