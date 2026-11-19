//! ONNX Runtime as a [`Backend`] for nakkash-ml's pipelines ([D81]).
//!
//! The runtime is not linked: [`Onnx::load`] loads the library the model
//! lock pins for this platform from the model store, its hash checked
//! first, so the build needs no runtime and the bake runs exactly the bytes
//! the lock names. Each graph opens where the lock places it: on CoreML on
//! macOS for a graph whose input shapes are fixed, on the CPU otherwise,
//! with the session settings of D81.
//!
//! The crate holds no unsafe code; the binding's own is behind its safe
//! API. Its code runs only with a runtime library present, so the coverage
//! gate leaves it out, as it leaves out the GPU crate.
//!
//! [D81]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d81-one-inference-runtime-loaded-at-run-time-from-a-locked-file

use std::sync::Arc;

use nakkash_baked::Runtime;
use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_ml::{
    Backend, Data, Graph, ModelLock, ModelStore, OpenGraph, Outputs, Placement, Tensor,
};
use ort::environment::Environment;
use ort::logging::LogLevel;
use ort::session::Session;
use ort::session::builder::{AutoDevicePolicy, GraphOptimizationLevel};
use ort::value::{DynValue, TensorElementType, ValueType};

/// The CoreML options every placed graph is compiled with, as the folder
/// name of its compiled form says them.
const COREML_OPTIONS: &str = "mlprogram-all";

/// ONNX Runtime, loaded.
pub struct Onnx {
    environment: Environment,
    runtime: Runtime,
    store: ModelStore,
}

impl std::fmt::Debug for Onnx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Onnx")
            .field("runtime", &self.runtime)
            .finish_non_exhaustive()
    }
}

/// A runtime failure as a diagnostic.
fn failed(what: &str, e: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(
        Kind::Invalid,
        format!("the inference runtime could not {what}: {e}"),
    )
}

impl Onnx {
    /// Loads the runtime library `lock` pins for this platform from
    /// `store`, once its hash is checked. A process loads one runtime.
    ///
    /// # Errors
    /// The store's diagnostic when the library is missing or has changed,
    /// and a [`Kind::Invalid`] one when it does not load.
    pub fn load(store: &ModelStore, lock: &ModelLock) -> Result<Self> {
        let library = store.runtime_library(lock)?;
        let sha256 = lock.library_here()?.sha256;
        let environment = ort::init_from(&library)
            .map_err(|e| failed("load its library", e))?
            .with_name("nakkash")
            .with_logger(Arc::new(
                |level: LogLevel, _category: &str, _id: &str, _at: &str, message: &str| match level
                {
                    LogLevel::Warning => tracing::warn!(target: "nakkash_onnx", "{message}"),
                    LogLevel::Error | LogLevel::Fatal => {
                        tracing::error!(target: "nakkash_onnx", "{message}")
                    }
                    _ => tracing::debug!(target: "nakkash_onnx", "{message}"),
                },
            ))
            .build()
            .map_err(|e| failed("start", e))?;
        tracing::info!(library = %library.display(), version = %lock.runtime.version, "inference runtime loaded");
        Ok(Self {
            environment,
            runtime: Runtime {
                version: lock.runtime.version.clone(),
                sha256,
            },
            store: store.clone(),
        })
    }
}

impl Backend for Onnx {
    fn open(&self, graph: &OpenGraph) -> Result<Box<dyn Graph>> {
        let named = |e: Diagnostic| Diagnostic {
            message: format!("`{}` {}: {}", graph.model, graph.graph, e.message),
            ..e
        };
        let start = std::time::Instant::now();
        let mut builder =
            Session::builder(&self.environment).map_err(|e| failed("make a session", e))?;
        builder = builder
            .with_intra_threads(graph.threads)
            .map_err(|e| named(failed("set up a session", e)))?;
        builder = builder
            .with_inter_threads(1)
            .map_err(|e| named(failed("set up a session", e)))?;
        builder = builder
            .with_parallel_execution(false)
            .map_err(|e| named(failed("set up a session", e)))?;
        builder = builder
            .with_deterministic_compute(true)
            .map_err(|e| named(failed("set up a session", e)))?;
        builder = builder
            .with_optimization_level(GraphOptimizationLevel::All)
            .map_err(|e| named(failed("set up a session", e)))?;
        builder = match graph.placement {
            Placement::CoreMl if cfg!(target_os = "macos") => {
                self.coreml(builder, graph).map_err(named)?
            }
            // The runtime otherwise moves a "CPU" session onto CoreML on
            // macOS (D81).
            _ => builder
                .with_auto_device(AutoDevicePolicy::PreferCPU)
                .map_err(|e| named(failed("set up a session", e)))?,
        };
        let session = builder
            .commit_from_file(&graph.path)
            .map_err(|e| named(failed("load the graph", e)))?;
        let inputs = session
            .inputs()
            .iter()
            .map(|i| i.name().to_owned())
            .collect();
        tracing::debug!(model = %graph.model, graph = %graph.graph, backend = graph.placement.here(), ms = start.elapsed().as_millis() as u64, "graph opened");
        Ok(Box::new(OrtGraph { session, inputs }))
    }

    fn runtime(&self) -> Option<Runtime> {
        Some(self.runtime.clone())
    }
}

impl Onnx {
    /// `builder` with the CoreML provider, its compiled models kept under
    /// the store by the graph's hash ([D81]).
    #[cfg(target_os = "macos")]
    fn coreml(
        &self,
        builder: ort::session::builder::SessionBuilder,
        graph: &OpenGraph,
    ) -> Result<ort::session::builder::SessionBuilder> {
        use ort::ep::coreml::{ComputeUnits, CoreML, ModelFormat};
        let cache = self
            .store
            .compiled(&graph.sha256, COREML_OPTIONS, &self.runtime.version);
        std::fs::create_dir_all(&cache).map_err(|e| Diagnostic::io("create", &cache, &e))?;
        let provider = CoreML::default()
            .with_model_format(ModelFormat::MLProgram)
            .with_compute_units(ComputeUnits::All)
            .with_model_cache_dir(cache.display())
            .build()
            .error_on_failure();
        builder
            .with_execution_providers([provider])
            .map_err(|e| failed("place a graph on CoreML", e))
    }

    /// No CoreML off macOS: the graph runs on the CPU.
    #[cfg(not(target_os = "macos"))]
    fn coreml(
        &self,
        builder: ort::session::builder::SessionBuilder,
        _graph: &OpenGraph,
    ) -> Result<ort::session::builder::SessionBuilder> {
        let _ = (COREML_OPTIONS, &self.store);
        builder
            .with_auto_device(AutoDevicePolicy::PreferCPU)
            .map_err(|e| failed("set up a session", e))
    }
}

/// An open session.
struct OrtGraph {
    session: Session,
    inputs: Vec<String>,
}

impl Graph for OrtGraph {
    fn inputs(&self) -> Vec<String> {
        self.inputs.clone()
    }

    fn run(&mut self, inputs: Vec<(String, Tensor)>) -> Result<Outputs> {
        let mut values: Vec<(String, DynValue)> = Vec::with_capacity(inputs.len());
        for (name, tensor) in inputs {
            values.push((name, value(tensor)?));
        }
        let outputs = self
            .session
            .run(values)
            .map_err(|e| failed("run a graph", e))?;
        let mut out = Vec::new();
        for (name, value) in outputs.iter() {
            let tensor = match value.dtype() {
                ValueType::Tensor {
                    ty: TensorElementType::Int64,
                    ..
                } => {
                    let (shape, data) = value
                        .try_extract_tensor::<i64>()
                        .map_err(|e| failed("read an output", e))?;
                    Tensor::i64(dims(shape), data.to_vec())
                }
                ValueType::Tensor {
                    ty: TensorElementType::Int32,
                    ..
                } => {
                    let (shape, data) = value
                        .try_extract_tensor::<i32>()
                        .map_err(|e| failed("read an output", e))?;
                    Tensor::i32(dims(shape), data.to_vec())
                }
                _ => {
                    let (shape, data) = value
                        .try_extract_tensor::<f32>()
                        .map_err(|e| failed("read an output", e))?;
                    Tensor::f32(dims(shape), data.to_vec())
                }
            };
            out.push((name.to_string(), tensor));
        }
        Ok(Outputs(out))
    }
}

/// A shape as the runtime gives it, as sizes.
fn dims(shape: &ort::value::Shape) -> Vec<usize> {
    shape.iter().map(|&d| d.max(0) as usize).collect()
}

/// `tensor` as a runtime value.
fn value(tensor: Tensor) -> Result<DynValue> {
    let shape = tensor.shape;
    let made = match tensor.data {
        Data::F32(v) => ort::value::Tensor::from_array((shape, v)).map(|t| t.into_dyn()),
        Data::I64(v) => ort::value::Tensor::from_array((shape, v)).map(|t| t.into_dyn()),
        Data::I32(v) => ort::value::Tensor::from_array((shape, v)).map(|t| t.into_dyn()),
    };
    made.map_err(|e| failed("take an input", e))
}
