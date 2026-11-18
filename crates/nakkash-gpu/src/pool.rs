//! Buffers kept for reuse: a frame makes and drops many frame-sized grids,
//! and making a buffer costs more than reusing one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// The most bytes of idle buffers kept; past it, a returned buffer is
/// dropped. Two 4K grids of float pixels per processor thread fit.
const IDLE_BYTES: u64 = 4 << 30;

/// Idle buffers by their size and use.
#[derive(Default)]
pub(crate) struct Pool {
    idle: Mutex<Idle>,
}

#[derive(Default)]
struct Idle {
    buffers: HashMap<(u64, Kind), Vec<wgpu::Buffer>>,
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
    /// A buffer of `bytes` for `kind`: an idle one, or a new one.
    pub(crate) fn take(&self, device: &wgpu::Device, bytes: u64, kind: Kind) -> wgpu::Buffer {
        let reused = {
            let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
            let found = idle.buffers.get_mut(&(bytes, kind)).and_then(Vec::pop);
            if found.is_some() {
                idle.bytes -= bytes;
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

    /// Keeps `buffer` for reuse, unless the idle ones already hold enough.
    pub(crate) fn give(&self, buffer: wgpu::Buffer, kind: Kind) {
        let bytes = buffer.size();
        let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
        if idle.bytes + bytes <= IDLE_BYTES {
            idle.bytes += bytes;
            idle.buffers.entry((bytes, kind)).or_default().push(buffer);
        }
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
