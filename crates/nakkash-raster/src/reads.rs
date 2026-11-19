//! What a frame draws on canvases of their own, drawn once per identity
//! ([D90]).
//!
//! Some nodes are drawn alone, on a canvas the size of the frame's: a
//! layer an effect reads (a matte, a displacement map, a blur map), a
//! matte's source, and the children of a layer whose content is worked on
//! as pixels. Two of them with the same identity ([`crate::identity`])
//! draw the same pixels at the same transform, whatever node or mark they
//! came from, so [`Reads`] draws each identity once for each transform and
//! grid it is drawn at, keeps it while more uses are to come, and hands out
//! copies. Drawing is deterministic, so a copy is the drawing pixel for
//! pixel.
//!
//! The uses are counted before the frame is drawn, by identity: an effect
//! input once per effect that reads it, a matte's source once per layer it
//! mattes, children once per layer that draws them alone. What lies inside
//! a drawing is counted once, as it is drawn once. What is used once is
//! drawn as it is used, and each kept drawing is let go after its last
//! use, or sooner when the memory budget needs the room ([`memory`]); a
//! drawing let go is drawn again if it is used again.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nakkash_format::Result;
use vello_cpu::kurbo::Affine;

use crate::accel::{Accelerator, Surface};
use crate::identity::{Id, Identities};
use crate::list::Node;
use crate::memory;

/// A drawing's key: the identity of what is drawn, the size of the canvas
/// it is drawn on, and the bits of the transform it is drawn at.
type Key = (Id, (u16, u16), [u64; 6]);

/// The drawings of one frame's nodes drawn alone, kept by identity. It is
/// shared by the painters of the frame and safe to use from several
/// threads.
#[derive(Debug, Default)]
pub(crate) struct Reads {
    /// The identities of the frame's nodes.
    ids: Mutex<Identities>,
    /// How many uses of each identity are still to come.
    left: Mutex<HashMap<Id, usize>>,
    /// The drawings kept: on the CPU, or in the accelerator the frame's
    /// float canvas is in.
    drawn: Mutex<HashMap<Key, Arc<Surface>>>,
}

impl Reads {
    /// The uses in the frame whose nodes are `nodes`, drawn with float
    /// colours or not (`floats`): every effect input, counted once per
    /// effect that reads it, every matte's source, once per layer it
    /// mattes, and the children of every layer that draws them alone.
    pub(crate) fn of(nodes: &[Node], floats: bool) -> Reads {
        let mut ids = Identities::default();
        let mut left = HashMap::new();
        let mut seen = HashSet::new();
        count(nodes, floats, &mut ids, &mut left, &mut seen);
        left.retain(|_, n| *n > 1);
        Reads {
            ids: Mutex::new(ids),
            left: Mutex::new(left),
            drawn: Mutex::default(),
        }
    }

    /// The drawing of `nodes`, drawn alone at `transform` on a canvas of
    /// `size`: the one kept for their identity, or `draw`'s, kept while
    /// more uses of it are to come. A drawing `draw` leaves in `accel` is
    /// kept there and copied there.
    ///
    /// # Errors
    /// The one `draw` returns, or a failed copy.
    pub(crate) fn drawn(
        &self,
        nodes: &[Node],
        transform: Affine,
        size: (u16, u16),
        accel: Option<&dyn Accelerator>,
        draw: impl FnOnce() -> Result<Surface>,
    ) -> Result<Surface> {
        let id = locked(&self.ids).list(nodes);
        let more = {
            let mut left = locked(&self.left);
            match left.get_mut(&id) {
                Some(n) => {
                    *n -= 1;
                    let more = *n > 0;
                    if !more {
                        left.remove(&id);
                    }
                    Some(more)
                }
                None => None,
            }
        };
        // What is used once, or not counted, is drawn as it is used.
        let Some(more) = more else {
            return draw();
        };
        let key = (id, size, transform.as_coeffs().map(f64::to_bits));
        let kept = locked(&self.drawn).get(&key).map(Arc::clone);
        let pixels = match kept {
            // Verify mode: a kept drawing used is drawn again and compared.
            Some(pixels) if crate::identity::verify_hit() => {
                let again = draw()?.into_pixels(accel)?;
                if again != pixels.duplicate(accel)?.into_pixels(accel)? {
                    return Err(nakkash_format::Diagnostic::new(
                        nakkash_format::Kind::Invalid,
                        format!(
                            "a drawing kept by its identity {:032x} differs from drawing it again",
                            id.0
                        ),
                    )
                    .expected("nodes of one identity that draw the same (D90)"));
                }
                crate::identity::note_reused();
                pixels
            }
            Some(pixels) => {
                crate::identity::note_reused();
                pixels
            }
            // Drawn without holding the lock: the drawing may draw nodes
            // alone in turn.
            None => Arc::new(draw()?),
        };
        let mut drawn = locked(&self.drawn);
        if more {
            if !drawn.contains_key(&key) {
                // Kept only within the budget: what is let go, or not
                // kept, is drawn again when it is used again.
                match self.make_room(&mut drawn, pixels.bytes()) {
                    true => {
                        memory::keep(pixels.bytes(), true);
                        drawn.insert(key, Arc::clone(&pixels));
                    }
                    false => memory::note_eviction(),
                }
            }
        } else {
            drawn.retain(|&(at, _, _), kept| {
                let stays = at != id;
                if !stays {
                    memory::keep(kept.bytes(), false);
                }
                stays
            });
        }
        drop(drawn);
        match Arc::try_unwrap(pixels) {
            Ok(pixels) => Ok(pixels),
            Err(shared) => shared.duplicate(accel),
        }
    }

    /// Lets kept drawings go, those with the fewest uses left first, until
    /// `bytes` more fit the share of the budget kept drawings may take
    /// ([`memory::room_to_keep`]). Whether they fit.
    fn make_room(&self, drawn: &mut HashMap<Key, Arc<Surface>>, bytes: u64) -> bool {
        while !memory::room_to_keep(bytes) {
            let victim = {
                let left = locked(&self.left);
                drawn
                    .keys()
                    .min_by_key(|(id, _, _)| left.get(id).copied().unwrap_or(0))
                    .copied()
            };
            let Some(key) = victim else {
                return false;
            };
            if let Some(kept) = drawn.remove(&key) {
                memory::keep(kept.bytes(), false);
                memory::note_eviction();
            }
        }
        true
    }
}

impl Drop for Reads {
    /// What is still kept stops counting as kept.
    fn drop(&mut self) {
        let drawn = self.drawn.get_mut().unwrap_or_else(PoisonError::into_inner);
        for kept in drawn.values() {
            memory::keep(kept.bytes(), false);
        }
    }
}

/// Counts into `left` the uses `nodes` hold, by identity, going into each
/// drawing the first time it is met (`seen`).
fn count(
    nodes: &[Node],
    floats: bool,
    ids: &mut Identities,
    left: &mut HashMap<Id, usize>,
    seen: &mut HashSet<Id>,
) {
    for node in nodes {
        let Node::Layer(layer) = node else {
            continue;
        };
        let mut drawn_alone = |inner: &[Node], ids: &mut Identities, seen: &mut HashSet<Id>| {
            let id = ids.list(inner);
            *left.entry(id).or_default() += 1;
            seen.insert(id)
        };
        let mut inner: Vec<&[Node]> = Vec::new();
        for input in layer.effects.iter().filter_map(|effect| effect.input()) {
            let input = std::slice::from_ref(&**input);
            if drawn_alone(input, ids, seen) {
                inner.push(input);
            }
        }
        if let Some(matte) = &layer.matte {
            let source = std::slice::from_ref(&*matte.source);
            if drawn_alone(source, ids, seen) {
                inner.push(source);
            }
        }
        if !crate::render::draws_children_alone(layer, floats)
            || drawn_alone(&layer.children, ids, seen)
        {
            inner.push(&layer.children);
        }
        for nodes in inner {
            count(nodes, floats, ids, left, seen);
        }
    }
}

/// The value behind `mutex`, also after a thread panicked holding it: the
/// maps stay whole, as each change to them is a single insert or removal.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "../tests/unit/reads.rs"]
mod tests;
