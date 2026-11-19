//! The drawings of layers that effects read, kept for one frame.
//!
//! An effect that reads another layer (a matte, a displacement map, a blur
//! map) gets that layer's drawing on a canvas of its own. When several
//! effects read the same layer, the frame shares one node for it
//! ([`Effect::with_input`](crate::effect::Effect::with_input)), and [`Reads`]
//! draws it once for each transform it is read at, then hands out copies.
//! Drawing is deterministic, so a copy is the drawing pixel for pixel.
//!
//! Layers whose children draw the same, such as one composition at one
//! time shown by several layers, carry one [`Shared`] mark, and [`Reads`]
//! draws their children once for each transform in the same way.
//!
//! Only nodes read, and children shared, more than once are kept. Each is
//! let go after its last use, so a frame holds at most the drawings still to
//! be used again.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nakkash_format::Result;
use vello_cpu::kurbo::Affine;

use crate::accel::{Accelerator, Surface};
use crate::list::{Node, Shared};
use crate::memory;

/// A drawing's key: the address of the read node or the shared mark, the
/// size of the canvas it is drawn on, and the bits of the transform it is
/// drawn at.
type Key = (usize, (u16, u16), [u64; 6]);

/// The drawings of read nodes, kept for one frame. It is shared by the
/// painters of the frame and safe to use from several threads.
#[derive(Debug, Default)]
pub(crate) struct Reads {
    /// How many reads of each node are still to come, by its address.
    left: Mutex<HashMap<usize, usize>>,
    /// The drawings kept, each with its node or mark held so that nothing
    /// else takes its address while the drawing is kept: on the CPU, or in
    /// the accelerator the frame's float canvas is in.
    drawn: Mutex<HashMap<Key, (Owner, Arc<Surface>)>>,
}

/// What a kept drawing is of, held while it is kept.
#[derive(Debug)]
enum Owner {
    /// A node effects read.
    Read(#[expect(dead_code, reason = "held only to keep its address")] Arc<Node>),
    /// The children layers share.
    Shared(#[expect(dead_code, reason = "held only to keep its address")] Shared),
}

impl Reads {
    /// The reads of the frame whose nodes are `nodes`: every effect input
    /// they hold, counted once per effect that reads it, and every shared
    /// mark, counted once per layer that carries it. What lies inside a
    /// read node or shared children is counted once, as they are drawn
    /// once.
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
    /// kept, or `draw`'s, kept while more reads of `node` are to come. A
    /// drawing `draw` leaves in `accel` is kept there and copied there.
    ///
    /// # Errors
    /// The one `draw` returns, or a failed copy.
    pub(crate) fn read(
        &self,
        node: &Arc<Node>,
        transform: Affine,
        size: (u16, u16),
        accel: Option<&dyn Accelerator>,
        draw: impl FnOnce() -> Result<Surface>,
    ) -> Result<Surface> {
        let address = Arc::as_ptr(node) as usize;
        self.kept(
            address,
            || Owner::Read(Arc::clone(node)),
            transform,
            size,
            accel,
            draw,
        )
    }

    /// The drawing of the children marked `shared` at `transform` on a
    /// canvas of `size`: the one kept, or `draw`'s, kept while more layers
    /// with the mark are to be drawn. A drawing `draw` leaves in `accel` is
    /// kept there and copied there.
    ///
    /// # Errors
    /// The one `draw` returns, or a failed copy.
    pub(crate) fn shared(
        &self,
        shared: &Shared,
        transform: Affine,
        size: (u16, u16),
        accel: Option<&dyn Accelerator>,
        draw: impl FnOnce() -> Result<Surface>,
    ) -> Result<Surface> {
        self.kept(
            shared.address(),
            || Owner::Shared(shared.clone()),
            transform,
            size,
            accel,
            draw,
        )
    }

    /// The drawing of what is at `address`, kept by `owner` while more uses
    /// of it are to come.
    fn kept(
        &self,
        address: usize,
        owner: impl FnOnce() -> Owner,
        transform: Affine,
        size: (u16, u16),
        accel: Option<&dyn Accelerator>,
        draw: impl FnOnce() -> Result<Surface>,
    ) -> Result<Surface> {
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
        // What is used once, or not counted, is drawn as it is used.
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
            if !drawn.contains_key(&key) {
                // Kept only within the budget: what is let go, or not
                // kept, is drawn again when it is read again.
                match self.make_room(&mut drawn) {
                    true => {
                        memory::keep(pixels.bytes(), true);
                        drawn.insert(key, (owner(), Arc::clone(&pixels)));
                    }
                    false => memory::note_eviction(),
                }
            }
        } else {
            drawn.retain(|&(at, _, _), (_, kept)| {
                let stays = at != address;
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
}

impl Reads {
    /// Lets kept drawings go, those with the fewest uses left first, while
    /// the render holds more than its budget ([`memory::over`]). Whether
    /// there is room to keep another.
    fn make_room(&self, drawn: &mut HashMap<Key, (Owner, Arc<Surface>)>) -> bool {
        while memory::over() {
            let victim = {
                let left = locked(&self.left);
                drawn
                    .keys()
                    .min_by_key(|(at, _, _)| left.get(at).copied().unwrap_or(0))
                    .copied()
            };
            let Some(key) = victim else {
                return false;
            };
            if let Some((_, kept)) = drawn.remove(&key) {
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
        for (_, kept) in drawn.values() {
            memory::keep(kept.bytes(), false);
        }
    }
}

/// Counts into `left` the reads and shared marks `nodes` hold, going into
/// each read node and shared children the first time they are met (`seen`).
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
        match &layer.shared {
            Some(shared) => {
                let address = shared.address();
                *left.entry(address).or_default() += 1;
                if seen.insert(address) {
                    count(&layer.children, left, seen);
                }
            }
            None => count(&layer.children, left, seen),
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
