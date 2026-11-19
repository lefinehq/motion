//! Work recorded for the GPU and not yet submitted, shared by every thread
//! that draws with one [`Gpu`](crate::Gpu).
//!
//! Every operation records into one command encoder, and the encoder is
//! submitted only when pixels must come back to the CPU, or when the work
//! recorded since the last submission grows past [`FLUSH_OPERATIONS`] or
//! its staged bytes past [`FLUSH_BYTES`], so the GPU starts on a long
//! frame before the CPU has recorded all of it. A frame whose work stays on
//! the GPU is one submission, at the end, when it is read back.
//!
//! Nothing is written to a buffer outside the encoder: pixels and values
//! from the CPU go through a staging belt whose copies are commands in it.
//! So the GPU runs every write and read in the order they were recorded,
//! and a pooled buffer let go of while work that reads it is still
//! recorded can be taken again at once: whatever writes it next is
//! recorded, and runs, after that work. The recorder's lock is held from
//! recording to submitting, so no thread's work is submitted ahead of work
//! recorded before it.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Operations recorded between submissions, at most: enough to keep the
/// GPU's queue short, few enough that the GPU starts on a heavy frame while
/// the CPU still draws.
pub(crate) const FLUSH_OPERATIONS: u32 = 256;

/// Bytes staged from the CPU between submissions, at most: the staging
/// belt's memory is reused only once a submission that read it is done.
pub(crate) const FLUSH_BYTES: u64 = 512 << 20;

/// The staging belt's unit of memory: a 1080p grid of float pixels fits in
/// one, a 4K grid takes a chunk of its own.
const CHUNK_BYTES: u64 = 64 << 20;

/// The encoder being recorded into and what it staged.
pub(crate) struct Recorder {
    device: wgpu::Device,
    encoder: Option<wgpu::CommandEncoder>,
    belt: wgpu::util::StagingBelt,
    /// Operations recorded since the last submission.
    operations: u32,
    /// Bytes staged since the last submission.
    staged: u64,
}

impl Recorder {
    pub(crate) fn new(device: &wgpu::Device) -> Recorder {
        Recorder {
            device: device.clone(),
            encoder: None,
            belt: wgpu::util::StagingBelt::new(device.clone(), CHUNK_BYTES),
            operations: 0,
            staged: 0,
        }
    }
}

/// The recorder, locked for one operation's recording.
pub(crate) struct Batch<'a> {
    recorder: MutexGuard<'a, Recorder>,
}

impl<'a> Batch<'a> {
    /// Locks `recorder` to record into it.
    pub(crate) fn lock(recorder: &'a Mutex<Recorder>) -> Batch<'a> {
        Batch {
            recorder: recorder.lock().unwrap_or_else(PoisonError::into_inner),
        }
    }

    /// The encoder, begun if nothing is recorded yet.
    pub(crate) fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        let recorder = &mut *self.recorder;
        recorder.encoder.get_or_insert_with(|| {
            recorder
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        })
    }

    /// Records a copy of `bytes` into `target` from the start.
    pub(crate) fn stage(&mut self, target: &wgpu::Buffer, bytes: &[u8]) {
        let Some(size) = wgpu::BufferSize::new(bytes.len() as u64) else {
            return;
        };
        self.encoder();
        let recorder = &mut *self.recorder;
        let encoder = recorder.encoder.as_mut().expect("begun above");
        recorder
            .belt
            .write_buffer(encoder, target, 0, size)
            .copy_from_slice(bytes);
        recorder.staged += bytes.len() as u64;
    }

    /// Ends one operation: whether enough is recorded to submit.
    pub(crate) fn done(&mut self) -> bool {
        self.recorder.operations += 1;
        self.recorder.operations >= FLUSH_OPERATIONS || self.recorder.staged >= FLUSH_BYTES
    }

    /// Submits everything recorded to `queue`, `None` when nothing is.
    pub(crate) fn submit(mut self, queue: &wgpu::Queue) -> Option<wgpu::SubmissionIndex> {
        let recorder = &mut *self.recorder;
        let encoder = recorder.encoder.take()?;
        recorder.belt.finish();
        let index = queue.submit([encoder.finish()]);
        recorder.belt.recall();
        recorder.operations = 0;
        recorder.staged = 0;
        Some(index)
    }
}
