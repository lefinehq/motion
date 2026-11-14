//! Capitals, applied before shaping: the text as it will be set, and for
//! every character of it the source character it came from and whether it is
//! a synthesised small capital. Upper-casing may change lengths (`ß` becomes
//! `SS`), so positions are mapped back through this table, never assumed equal.

use std::ops::Range;

use crate::style::{Caps, StyledText};

/// Which characters a capitals setting changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Letters {
    /// None.
    None,
    /// Lower-case letters only.
    Lower,
    /// Every letter.
    All,
}

/// One capitals setting: which characters it upper-cases, and which of those
/// it sets as small capitals.
struct CapsRule {
    caps: Caps,
    upper: Letters,
    small: Letters,
}

/// Every capitals setting, as data.
const RULES: &[CapsRule] = &[
    CapsRule {
        caps: Caps::Normal,
        upper: Letters::None,
        small: Letters::None,
    },
    CapsRule {
        caps: Caps::All,
        upper: Letters::All,
        small: Letters::None,
    },
    CapsRule {
        caps: Caps::Small,
        upper: Letters::Lower,
        small: Letters::Lower,
    },
    CapsRule {
        caps: Caps::AllSmall,
        upper: Letters::All,
        small: Letters::All,
    },
];

impl Letters {
    fn includes(self, c: char) -> bool {
        match self {
            Letters::None => false,
            Letters::Lower => c.is_lowercase(),
            Letters::All => c.is_alphabetic(),
        }
    }
}

/// The text as set, with where each set character came from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cased {
    /// The text as it will be shaped.
    pub(crate) text: String,
    /// One span per set character, in order.
    pub(crate) spans: Vec<Span>,
}

/// One character of [`Cased::text`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    /// Its bytes in the set text.
    pub(crate) set: Range<usize>,
    /// The bytes of the source character it came from.
    pub(crate) source: Range<usize>,
    /// The source run it belongs to.
    pub(crate) run: usize,
    /// Whether it is a synthesised small capital.
    pub(crate) small: bool,
}

/// Applies each run's capitals to `styled`'s text, which
/// [`StyledText::check`] has validated.
pub(crate) fn apply(styled: &StyledText) -> Cased {
    let mut text = String::with_capacity(styled.text.len());
    let mut spans = Vec::with_capacity(styled.text.len());
    for (run_index, run) in styled.runs.iter().enumerate() {
        let rule = RULES
            .iter()
            .find(|r| r.caps == run.style.caps)
            .expect("every capitals setting has a rule");
        for (offset, c) in styled.text[run.range.clone()].char_indices() {
            let source_start = run.range.start + offset;
            let source = source_start..source_start + c.len_utf8();
            let small = rule.small.includes(c);
            let mut push = |set: char| {
                let start = text.len();
                text.push(set);
                spans.push(Span {
                    set: start..text.len(),
                    source: source.clone(),
                    run: run_index,
                    small,
                });
            };
            if rule.upper.includes(c) {
                c.to_uppercase().for_each(&mut push);
            } else {
                push(c);
            }
        }
    }
    Cased { text, spans }
}

impl Cased {
    /// The span of the set character that starts at or covers set byte `at`.
    pub(crate) fn span_at(&self, at: usize) -> &Span {
        let i = self.spans.partition_point(|s| s.set.end <= at);
        &self.spans[i.min(self.spans.len() - 1)]
    }

    /// The source bytes the set bytes `range` came from.
    pub(crate) fn source_of(&self, range: Range<usize>) -> Range<usize> {
        if range.is_empty() {
            let at = self.span_at(range.start).source.start;
            return at..at;
        }
        let first = self.span_at(range.start).source.start;
        let last = self.span_at(range.end - 1).source.end;
        first..last
    }
}

#[cfg(test)]
#[path = "../tests/unit/caps.rs"]
mod tests;
