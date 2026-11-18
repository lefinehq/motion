//! Finding a GPU: a hardware adapter of a backend we draw on, never a
//! software one, with the limits a frame needs ([D74]).
//!
//! [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference

use std::fmt;

use crate::block::block_on;

/// Why no GPU is used: the render then stays on the CPU, and the caller
/// says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No adapter of Metal, Vulkan or DX12 was found.
    NoAdapter(String),
    /// The only adapter draws in software on the CPU, as a server without a
    /// GPU offers: it would be slower than the CPU path and still not the
    /// reference.
    Software(String),
    /// The adapter would not give a device.
    NoDevice(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::NoAdapter(why) => write!(f, "no GPU adapter was found ({why})"),
            Refusal::Software(name) => write!(
                f,
                "the only GPU adapter, `{name}`, draws in software on the CPU"
            ),
            Refusal::NoDevice(why) => write!(f, "the GPU would not open a device ({why})"),
        }
    }
}

/// The adapter a device was opened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    /// Its name, as the driver gives it.
    pub name: String,
    /// The backend that drives it: Metal, Vulkan or DX12.
    pub backend: String,
}

impl fmt::Display for Adapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.backend)
    }
}

/// An open device and what it was opened on.
pub(crate) struct Opened {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) adapter: Adapter,
    /// The largest storage buffer one binding may hold, in bytes.
    pub(crate) max_binding: u64,
}

/// Whether an adapter of this type draws on hardware. A CPU adapter is a
/// software rasteriser; an unknown type is not trusted to be anything
/// else.
pub(crate) fn is_hardware(kind: wgpu::DeviceType) -> bool {
    matches!(
        kind,
        wgpu::DeviceType::DiscreteGpu
            | wgpu::DeviceType::IntegratedGpu
            | wgpu::DeviceType::VirtualGpu
    )
}

/// Opens a device on the best hardware adapter of Metal, Vulkan or DX12.
/// The GL backend is never asked: it is how most software rasterisers are
/// offered.
///
/// # Errors
/// A [`Refusal`] when there is no adapter, when the only one is software,
/// or when it will not open a device.
pub(crate) fn open() -> Result<Opened, Refusal> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::METAL | wgpu::Backends::VULKAN | wgpu::Backends::DX12;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..wgpu::RequestAdapterOptions::default()
    }))
    .map_err(|e| Refusal::NoAdapter(e.to_string()))?;
    let info = adapter.get_info();
    if !is_hardware(info.device_type) {
        return Err(Refusal::Software(info.name));
    }
    // Everything the adapter allows: a 4K frame of float pixels is 127 MiB
    // in one binding, near the smallest limit an adapter may have.
    let limits = adapter.limits();
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("nakkash"),
        required_features: wgpu::Features::empty(),
        required_limits: limits.clone(),
        ..wgpu::DeviceDescriptor::default()
    }))
    .map_err(|e| Refusal::NoDevice(e.to_string()))?;
    Ok(Opened {
        device,
        queue,
        adapter: Adapter {
            name: info.name,
            backend: info.backend.to_string(),
        },
        max_binding: limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size),
    })
}

#[cfg(test)]
#[path = "../tests/unit/device.rs"]
mod tests;
