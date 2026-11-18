//! The compute kernels and how they are dispatched. Each WGSL file is one
//! family of kernels that share a binding layout: a uniform block of
//! parameters at binding 0, the buffer written at 1, and the buffers read
//! after it.

use std::collections::HashMap;
use std::fmt::Write as _;

use nakkash_raster::BLEND_MODES;
use wgpu::util::DeviceExt;

/// The invocations of one workgroup of the per-pixel kernels.
pub(crate) const PIXEL_GROUP: u32 = 256;
/// The invocations of one workgroup of the box pass, one line each.
pub(crate) const LINE_GROUP: u32 = 64;
/// The most workgroups one dimension of a dispatch may have.
const MAX_GROUPS: u32 = 65535;

/// The families, and the entry points of each.
pub(crate) const PIXEL: FamilySpec = FamilySpec {
    source: include_str!("shaders/pixel.wgsl"),
    reads: 2,
    entries: &[
        "lay",
        "mix_towards",
        "cut",
        "scale",
        "glow_threshold",
        "glow_add",
        "glow_alone",
        "shadow_colour",
        "shadow_under",
    ],
};
pub(crate) const AMOUNT: FamilySpec = FamilySpec {
    source: include_str!("shaders/amount.wgsl"),
    reads: 2,
    entries: &["share_values", "share_alpha", "share_matte"],
};
pub(crate) const UNPACK: FamilySpec = FamilySpec {
    source: include_str!("shaders/unpack.wgsl"),
    reads: 2,
    entries: &["unpack"],
};
pub(crate) const PACK: FamilySpec = FamilySpec {
    source: include_str!("shaders/pack.wgsl"),
    reads: 1,
    entries: &["pack"],
};
pub(crate) const TRANSFORM: FamilySpec = FamilySpec {
    source: include_str!("shaders/transform.wgsl"),
    reads: 1,
    entries: &["transformed"],
};
pub(crate) const BOX_PASS: FamilySpec = FamilySpec {
    source: include_str!("shaders/boxpass.wgsl"),
    reads: 1,
    entries: &["box_pass"],
};

/// A WGSL source, how many buffers its kernels read, and its entry points.
pub(crate) struct FamilySpec {
    pub(crate) source: &'static str,
    pub(crate) reads: usize,
    pub(crate) entries: &'static [&'static str],
}

/// One family, compiled.
pub(crate) struct Family {
    layout: wgpu::BindGroupLayout,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    reads: usize,
}

/// Every family, compiled for one device.
pub(crate) struct Kernels {
    pub(crate) pixel: Family,
    pub(crate) amount: Family,
    pub(crate) unpack: Family,
    pub(crate) pack: Family,
    pub(crate) transform: Family,
    pub(crate) box_pass: Family,
}

/// The constants the per-pixel kernels name blend modes by, one per row of
/// the rasteriser's table, numbered by the row: `MODE_COLOR_DODGE` for
/// `color-dodge`.
pub(crate) fn mode_constants() -> String {
    let mut constants = String::new();
    for (i, spec) in BLEND_MODES.iter().enumerate() {
        let name = spec.name.to_uppercase().replace('-', "_");
        writeln!(constants, "const MODE_{name}: u32 = {i}u;").expect("a string takes writes");
    }
    constants
}

/// The row of `mode` in the rasteriser's table of blend modes.
pub(crate) fn mode_row(mode: nakkash_raster::BlendMode) -> u32 {
    let row = BLEND_MODES
        .iter()
        .position(|spec| spec.mode == mode)
        .expect("every blend mode has a row");
    u32::try_from(row).expect("a short table")
}

impl Kernels {
    /// Compiles every family on `device`.
    pub(crate) fn new(device: &wgpu::Device) -> Kernels {
        let pixel_source = format!("{}\n{}", mode_constants(), PIXEL.source);
        Kernels {
            pixel: Family::new(device, &pixel_source, &PIXEL),
            amount: Family::new(device, AMOUNT.source, &AMOUNT),
            unpack: Family::new(device, UNPACK.source, &UNPACK),
            pack: Family::new(device, PACK.source, &PACK),
            transform: Family::new(device, TRANSFORM.source, &TRANSFORM),
            box_pass: Family::new(device, BOX_PASS.source, &BOX_PASS),
        }
    }
}

impl Family {
    fn new(device: &wgpu::Device, source: &str, spec: &FamilySpec) -> Family {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage(1, false),
        ];
        for read in 0..spec.reads {
            entries.push(storage(2 + read as u32, true));
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipelines = spec
            .entries
            .iter()
            .map(|&entry| {
                let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                });
                (entry, pipeline)
            })
            .collect();
        Family {
            layout,
            pipelines,
            reads: spec.reads,
        }
    }

    /// Records one dispatch of `entry` into `encoder`: `params` at binding
    /// 0, `written` at 1 and `read` after it, over `groups` workgroups.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dispatch(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        entry: &str,
        params: &Params,
        written: &wgpu::Buffer,
        read: &[&wgpu::Buffer],
        groups: (u32, u32),
    ) {
        debug_assert_eq!(read.len(), self.reads, "every read binding is bound");
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &params.bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: written.as_entire_binding(),
            },
        ];
        for (i, buffer) in read.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 2 + i as u32,
                resource: buffer.as_entire_binding(),
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &entries,
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(
            self.pipelines
                .get(entry)
                .expect("every entry point is compiled"),
        );
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(groups.0, groups.1, 1);
    }
}

/// The workgroups that cover `count` pixels, one invocation each, and how
/// many invocations one row of them covers.
pub(crate) fn pixel_groups(count: u32) -> ((u32, u32), u32) {
    let groups = count.div_ceil(PIXEL_GROUP).max(1);
    let x = groups.min(MAX_GROUPS);
    ((x, groups.div_ceil(x)), x * PIXEL_GROUP)
}

/// The bytes of every uniform block: more than the largest kernel's.
const PARAMS_BYTES: usize = 64;

/// A kernel's uniform block, written field by field as WGSL lays out a
/// struct of 32-bit scalars and `vec4<f32>`s.
#[derive(Debug, Clone, Default)]
pub(crate) struct Params(Vec<u8>);

impl Params {
    /// An empty block.
    pub(crate) fn new() -> Params {
        Params(Vec::new())
    }

    /// The block with an unsigned integer after what it holds.
    pub(crate) fn u32(mut self, value: u32) -> Params {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// The block with a float after what it holds.
    pub(crate) fn f32(mut self, value: f32) -> Params {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// The block with a `vec4<f32>` after what it holds, aligned to 16
    /// bytes as WGSL aligns one.
    pub(crate) fn vec4(mut self, value: [f32; 4]) -> Params {
        while !self.0.len().is_multiple_of(16) {
            self.0.push(0);
        }
        for v in value {
            self = self.f32(v);
        }
        self
    }

    /// Its bytes, padded with zeros to [`PARAMS_BYTES`]: a kernel's
    /// fields that a caller leaves out read as zero.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        debug_assert!(self.0.len() <= PARAMS_BYTES, "a uniform block fits");
        let mut bytes = self.0.clone();
        bytes.resize(PARAMS_BYTES, 0);
        bytes
    }
}

#[cfg(test)]
#[path = "../tests/unit/kernels.rs"]
mod tests;
