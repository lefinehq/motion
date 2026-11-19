//! Buffers kept for reuse: a frame makes and drops many frame-sized grids,
//! and making a buffer costs more than reusing one (the GPU clears a new
//! buffer before it is first read).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

/// The most bytes of idle buffers kept; past it, the buffers idle longest
/// are dropped. Two 4K grids of float pixels per processor thread fit.
const IDLE_BYTES: u64 = 4 << 30;

/// Idle buffers by their size and use.
#[derive(Default)]
pub(crate) struct Pool {
    idle: Mutex<Idle>,
}

#[derive(Default)]
struct Idle {
    /// The idle buffers of each size and use, the one given back last at
    /// the end.
    buffers: HashMap<(u64, Kind), Vec<wgpu::Buffer>>,
    /// The size and use of every idle buffer, in the order they were given
    /// back: the first has been idle longest.
    order: VecDeque<(u64, Kind)>,
    bytes: u64,
}

/// What a pooled buffer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    /// Pixels and shares that kernels read and write.
    Storage,
    /// Pixels copied back to the CPU.
    Readback,
}

impl Pool {
    /// A buffer of `bytes` for `kind`: the idle one given back last, or a
    /// new one.
    pub(crate) fn take(&self, device: &wgpu::Device, bytes: u64, kind: Kind) -> wgpu::Buffer {
        let reused = {
            let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
            let found = idle.buffers.get_mut(&(bytes, kind)).and_then(Vec::pop);
            if found.is_some() {
                idle.bytes -= bytes;
                if let Some(at) = idle.order.iter().rposition(|&k| k == (bytes, kind)) {
                    idle.order.remove(at);
                }
            }
            found
        };
        reused.unwrap_or_else(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: bytes,
                usage: match kind {
                    Kind::Storage => {
                        wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST
                    }
                    Kind::Readback => wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                },
                mapped_at_creation: false,
            })
        })
    }

    /// Keeps `buffer` for reuse, dropping those idle longest while the
    /// idle ones hold more than [`IDLE_BYTES`]: the sizes a frame uses now
    /// are the ones it will ask for next.
    pub(crate) fn give(&self, buffer: wgpu::Buffer, kind: Kind) {
        let bytes = buffer.size();
        let mut dropped = Vec::new();
        {
            let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
            idle.bytes += bytes;
            idle.buffers.entry((bytes, kind)).or_default().push(buffer);
            idle.order.push_back((bytes, kind));
            while idle.bytes > IDLE_BYTES {
                let Some(key) = idle.order.pop_front() else {
                    break;
                };
                let Some(list) = idle.buffers.get_mut(&key) else {
                    continue;
                };
                if list.is_empty() {
                    continue;
                }
                // The one of that size idle longest.
                dropped.push(list.remove(0));
                idle.bytes -= key.0;
            }
        }
        // Dropped outside the lock: a buffer's destruction may wait on the
        // device.
        drop(dropped);
    }
}

/// A pooled buffer, given back to its pool when dropped.
pub(crate) struct Pooled {
    buffer: Option<wgpu::Buffer>,
    kind: Kind,
    pool: Arc<Pool>,
}

impl Pooled {
    /// A buffer of `bytes` for `kind` from `pool`.
    pub(crate) fn new(pool: &Arc<Pool>, device: &wgpu::Device, bytes: u64, kind: Kind) -> Pooled {
        Pooled {
            buffer: Some(pool.take(device, bytes, kind)),
            kind,
            pool: Arc::clone(pool),
        }
    }

    /// The buffer.
    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        self.buffer
            .as_ref()
            .expect("a pooled buffer is held until dropped")
    }
}

impl Drop for Pooled {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.give(buffer, self.kind);
        }
    }
}
