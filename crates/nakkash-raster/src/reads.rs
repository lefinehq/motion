//! The drawings of layers that effects read, kept for one frame.
//!
//! An effect that reads another layer (a matte, a displacement map, a blur
//! map) gets that layer's drawing on a canvas of its own. When several
//! effects read the same layer, the frame shares one node for it
//! ([`Effect::with_input`](crate::effect::Effect::with_input)), and [`Reads`]
//! draws it once for each transform it is read at, then hands out copies.
//! Drawing is deterministic, so a copy is the drawing pixel for pixel.
//!
//! Only nodes read more than once are kept. Each is let go after its last
//! read, so a frame holds at most the drawings still to be read again.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nakkash_format::Result;
use vello_cpu::kurbo::Affine;

use crate::list::Node;
use crate::pixels::Pixels;

/// A drawing's key: the read node's address, the size of the canvas it is
/// drawn on, and the bits of the transform it is drawn at.
type Key = (usize, (u16, u16), [u64; 6]);

/// The drawings of read nodes, kept for one frame. It is shared by the
/// painters of the frame and safe to use from several threads.
#[derive(Debug, Default)]
pub(crate) struct Reads {
    /// How many reads of each node are still to come, by its address.
    left: Mutex<HashMap<usize, usize>>,
    /// The drawings kept, each with its node held so that no other node
    /// takes its address while the drawing is kept.
    drawn: Mutex<HashMap<Key, Kept>>,
}

/// A kept drawing, with the node it is of.
type Kept = (Arc<Node>, Arc<Pixels>);

impl Reads {
    /// The reads of the frame whose nodes are `nodes`: every effect input
    /// they hold, counted once per effect that reads it. A node read inside
    /// a read node is counted once, as the read node is drawn once.
    pub(crate) fn of(nodes: &[Node]) -> Reads {
        let mut left = HashMap::new();
        let mut seen = HashSet::new();
        count(nodes, &mut left, &mut seen);
        left.retain(|_, n| *n > 1);
        Reads {
            left: Mutex::new(left),
            drawn: Mutex::default(),
        }
    }

    /// The drawing of `node` at `transform` on a canvas of `size`: the one
    /// kept, or `draw`'s, kept while more reads of `node` are to come.
    ///
    /// # Errors
    /// The one `draw` returns.
    pub(crate) fn read(
        &self,
        node: &Arc<Node>,
        transform: Affine,
        size: (u16, u16),
        draw: impl FnOnce() -> Result<Pixels>,
    ) -> Result<Pixels> {
        let address = Arc::as_ptr(node) as usize;
        let more = {
            let mut left = locked(&self.left);
            match left.get_mut(&address) {
                Some(n) => {
                    *n -= 1;
                    let more = *n > 0;
                    if !more {
                        left.remove(&address);
                    }
                    Some(more)
                }
                None => None,
            }
        };
        // A node read once, or not counted, is drawn as it is read.
        let Some(more) = more else {
            return draw();
        };
        let key = (address, size, transform.as_coeffs().map(f64::to_bits));
        let kept = locked(&self.drawn)
            .get(&key)
            .map(|(_, pixels)| Arc::clone(pixels));
        let pixels = match kept {
            Some(pixels) => pixels,
            // Drawn without holding the lock: the drawing may read nodes in
            // turn.
            None => Arc::new(draw()?),
        };
        let mut drawn = locked(&self.drawn);
        if more {
            drawn
                .entry(key)
                .or_insert_with(|| (Arc::clone(node), Arc::clone(&pixels)));
        } else {
            drawn.retain(|&(at, _, _), _| at != address);
        }
        drop(drawn);
        Ok(Arc::try_unwrap(pixels).unwrap_or_else(|shared| (*shared).clone()))
    }
}

/// Counts into `left` the reads `nodes` hold, going into each read node the
/// first time it is met (`seen`).
fn count(nodes: &[Node], left: &mut HashMap<usize, usize>, seen: &mut HashSet<usize>) {
    for node in nodes {
        let Node::Layer(layer) = node else {
            continue;
        };
        for input in layer.effects.iter().filter_map(|effect| effect.input()) {
            let address = Arc::as_ptr(input) as usize;
            *left.entry(address).or_default() += 1;
            if seen.insert(address) {
                count(std::slice::from_ref(&**input), left, seen);
            }
        }
        if let Some(matte) = &layer.matte {
            count(std::slice::from_ref(&*matte.source), left, seen);
        }
        count(&layer.children, left, seen);
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
