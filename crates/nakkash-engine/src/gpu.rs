//! The optional GPU backend ([D74]), behind the engine's `gpu` feature: a
//! tool asks for it here and hands it to a [`Session`](crate::Session).
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

use std::sync::Arc;

use nakkash_raster::Accelerator;

pub use nakkash_gpu::{Adapter, Refusal};

/// A GPU opened for renders, and the adapter it is on.
pub struct Opened {
    /// What renders hand their work to.
    pub accelerator: Arc<dyn Accelerator>,
    /// The adapter it draws on.
    pub adapter: Adapter,
}

/// Opens the best hardware GPU for renders, logging a warning with the
/// reason when there is none: a machine without one, such as a render
/// server, stays on the CPU.
///
/// # Errors
/// The [`Refusal`] that keeps renders on the CPU.
pub fn open() -> Result<Opened, Refusal> {
    match nakkash_gpu::Gpu::open() {
        Ok(gpu) => {
            let adapter = gpu.adapter().clone();
            tracing::info!(adapter = %adapter, "the GPU draws approximate frames (D74)");
            Ok(Opened {
                accelerator: Arc::new(gpu),
                adapter,
            })
        }
        Err(refusal) => {
            tracing::warn!("{refusal}; drawing on the CPU");
            Err(refusal)
        }
    }
}
