//! The parameters that load a model and create its context, and the CPU threads that run it.

use std::ffi::CString;
use std::num::NonZeroU32;
use std::path::Path;
use std::pin::Pin;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::model::params::{FitError, LlamaModelParams};
use llama_cpp_2::{LlamaBackendDevice, LlamaBackendDeviceType, list_llama_ggml_backend_devices, max_devices};
use llama_cpp_sys_2::GGML_LOG_LEVEL_WARN;
use typealike_core::inference::{Backend, CONTEXT_TOKENS, CpuCores, InferenceError};

/// The most prompt tokens that one decode takes on the CPU. No larger batch was faster.
const BATCH_TOKENS: u32 = 128;
/// The most prompt tokens that one decode takes on Vulkan, CUDA and Metal. Most prompts have 129 to
/// 256 tokens, and one decode of them is faster than two. The context keeps GPU memory for the
/// outputs of a whole batch while the model stays loaded. A batch of 512 tokens was a little faster
/// only for longer prompts, for about 250 MiB more.
const LARGE_BATCH_TOKENS: u32 = 256;
/// The fewest threads for the tokens of a prompt on the CPU, when the CPU has that many cores: the
/// default of llama.cpp. On a CPU with few performance cores, efficiency cores then also help.
const MIN_PROMPT_THREADS: usize = 4;
/// The most threads for each added token on the CPU. An added token waits mostly for memory, so
/// more threads use much more CPU time for a small gain.
const MAX_GENERATION_THREADS: usize = 4;
/// The sequences of a context: the prompt and its completion.
const SEQUENCES: u32 = 2;
/// The GPU memory that a model must leave free, as llama.cpp leaves by default, so that the
/// desktop and other apps keep working.
const GPU_MEMORY_MARGIN_BYTES: usize = 1024 * 1024 * 1024;
const BYTES_IN_MIB: usize = 1024 * 1024;

/// The most prompt tokens that one decode takes. A newer request can stop a prompt decode only
/// between two batches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptBatch {
    /// The batch of the backend that each load names.
    OfBackend,
    /// The same batch on each backend, to measure another batch.
    Fixed(NonZeroU32),
}

impl PromptBatch {
    /// Returns the most prompt tokens that one decode takes on `backend`.
    pub(super) fn tokens_on(self, backend: Backend) -> u32 {
        match (self, backend) {
            (Self::Fixed(tokens), _) => tokens.get(),
            (Self::OfBackend, Backend::Vulkan | Backend::Cuda | Backend::Metal) => LARGE_BATCH_TOKENS,
            (Self::OfBackend, Backend::Cpu) => BATCH_TOKENS,
        }
    }
}

/// How each context of a loaded model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ContextSetup {
    /// The threads on the CPU, or `None` for the threads of llama.cpp.
    pub(super) cpu_threads: Option<CpuThreads>,
    /// The most prompt tokens that one decode takes.
    pub(super) batch_tokens: u32,
    /// Whether a load warms up the backend before it answers.
    pub(super) warm_up: WarmUp,
}

/// Whether a load makes the context and runs the first decodes before it answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WarmUp {
    /// The load warms up the backend. A GPU backend prepares its kernels and its memory at the
    /// first decodes, which makes the first completion after a load slow.
    AtLoad,
    /// The first completion makes the context. On the CPU, a warm-up costs more CPU time at each
    /// load than it saves at the first completion.
    Off,
}

impl WarmUp {
    /// Returns the warm-up of a load on `backend`.
    pub(super) fn on(backend: Backend) -> Self {
        match backend {
            Backend::Vulkan | Backend::Cuda | Backend::Metal => Self::AtLoad,
            Backend::Cpu => Self::Off,
        }
    }
}

/// Logs each device that llama.cpp found. A release build of llama.cpp loads its GPU backends
/// without a log, so a GPU backend that is not in this list failed to load.
pub(super) fn log_devices() {
    for device in list_llama_ggml_backend_devices() {
        tracing::info!(
            backend = %device.backend,
            name = %device.name,
            description = %device.description,
            device_type = ?device.device_type,
            free_mib = device.memory_free / BYTES_IN_MIB,
            total_mib = device.memory_total / BYTES_IN_MIB,
            "found a llama.cpp device"
        );
    }
}

/// Returns the parameters that load the model at `path` on `backend`. For Vulkan and CUDA, it first
/// makes sure that the whole model fits in the GPU with a context that decodes `batch_tokens` at a
/// time, because llama.cpp otherwise loads a part of it into slow shared memory, or fails at a
/// later step. Metal keeps the parameters of llama.cpp, which put the whole model on the GPU of the
/// Mac.
pub(super) fn model_params(
    path: &Path,
    backend: Backend,
    batch_tokens: u32,
) -> Result<LlamaModelParams, InferenceError> {
    match backend {
        Backend::Cpu => on_devices(&[]),
        Backend::Metal => Ok(LlamaModelParams::default()),
        gpu_backend @ (Backend::Vulkan | Backend::Cuda) => {
            let devices = list_llama_ggml_backend_devices();
            let device = gpu_device(&devices, gpu_backend).ok_or_else(|| {
                tracing::warn!(
                    backend = ?gpu_backend,
                    "no GPU device for the backend: llama.cpp did not load it, or found no GPU for it"
                );
                InferenceError::BackendUnavailable
            })?;
            tracing::info!(
                backend = ?gpu_backend,
                device = %device.description,
                free_mib = device.memory_free / BYTES_IN_MIB,
                "loading the model on a GPU"
            );
            check_fit(path, device.index, GPU_MEMORY_MARGIN_BYTES, batch_tokens)?;
            on_devices(&[device.index])
        }
    }
}

/// Returns the device that runs `backend`: its first discrete GPU, else its first integrated GPU.
/// The CPU backend has none, and llama.cpp selects the GPU for Metal.
fn gpu_device(devices: &[LlamaBackendDevice], backend: Backend) -> Option<&LlamaBackendDevice> {
    let registry = match backend {
        Backend::Cpu | Backend::Metal => return None,
        Backend::Vulkan => "Vulkan",
        Backend::Cuda => "CUDA",
    };
    let of_type = |device_type| {
        devices
            .iter()
            .find(|device| device.backend == registry && device.device_type == device_type)
    };
    of_type(LlamaBackendDeviceType::Gpu).or_else(|| of_type(LlamaBackendDeviceType::IntegratedGpu))
}

/// Returns parameters that put the model on the GPU `devices` only. With no devices, the model
/// runs on the CPU only.
fn on_devices(devices: &[usize]) -> Result<LlamaModelParams, InferenceError> {
    LlamaModelParams::default()
        .with_devices(devices)
        .map_err(|error| {
            tracing::warn!(%error, "failed to select the GPU");
            InferenceError::BackendUnavailable
        })
}

/// Makes sure that the model at `path`, with all its layers and a context that decodes
/// `batch_tokens` at a time, fits in the free memory of `device` and leaves `margin_bytes` free. It
/// reads the model but does not load it.
fn check_fit(
    path: &Path,
    device: usize,
    margin_bytes: usize,
    batch_tokens: u32,
) -> Result<(), InferenceError> {
    let model_path = path
        .to_str()
        .and_then(|path| CString::new(path).ok())
        .ok_or(InferenceError::ModelInvalid)?;
    let mut params = on_devices(&[device])?;
    let all_layers = params.n_gpu_layers();
    let setup = ContextSetup {
        cpu_threads: None,
        batch_tokens,
        warm_up: WarmUp::Off,
    };
    let mut context = context_params(CONTEXT_TOKENS.0, setup);
    let mut margins = vec![margin_bytes; max_devices()];
    let fitted = Pin::new(&mut params).fit_params(
        &model_path,
        &mut context,
        &mut margins,
        CONTEXT_TOKENS.0,
        GGML_LOG_LEVEL_WARN,
    );
    let gpu_layers = params.n_gpu_layers();
    tracing::info!(
        gpu_layers,
        all_layers,
        ?fitted,
        "checked that the model fits in the GPU"
    );
    fit_verdict(fitted.map(|_| gpu_layers == all_layers))
}

/// Reads the outcome of llama.cpp's fit: whether it kept every layer on the GPU, or why it failed.
/// llama.cpp moves layers to the CPU when the model does not fit. TypeAlike refuses that instead.
fn fit_verdict(fitted: Result<bool, FitError>) -> Result<(), InferenceError> {
    match fitted {
        Ok(true) => Ok(()),
        Ok(false) | Err(FitError::Failure) => Err(InferenceError::ModelTooLarge),
        Err(FitError::Error) => Err(InferenceError::ModelInvalid),
    }
}

/// Returns the parameters of a context that runs as `setup` tells.
pub(super) fn context_params(context_tokens: u32, setup: ContextSetup) -> LlamaContextParams {
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context_tokens))
        .with_n_batch(setup.batch_tokens)
        .with_n_ubatch(setup.batch_tokens)
        .with_n_seq_max(SEQUENCES)
        .with_kv_unified(true);
    match setup.cpu_threads {
        Some(threads) => params
            .with_n_threads(threads.generation)
            .with_n_threads_batch(threads.prompt),
        None => params,
    }
}

/// The threads that run a model on the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CpuThreads {
    /// The threads for the tokens of a prompt.
    pub(super) prompt: i32,
    /// The threads for each added token.
    pub(super) generation: i32,
}

impl CpuThreads {
    /// Returns the threads for a CPU with `cores`. A prompt runs one thread on each performance
    /// core, because its threads work in lockstep: a thread on a slower core, or on the second
    /// thread of a core, makes the other threads wait.
    fn for_cores(cores: CpuCores) -> Self {
        let physical = cores.performance.get().saturating_add(cores.efficiency);
        let prompt = cores.performance.get().max(physical.min(MIN_PROMPT_THREADS));
        let generation = prompt.min(MAX_GENERATION_THREADS);
        Self {
            prompt: i32::try_from(prompt).unwrap_or(i32::MAX),
            generation: i32::try_from(generation).unwrap_or(i32::MAX),
        }
    }
}

/// Returns the threads for a model on `backend` with the `cpu_cores` of this computer. Returns
/// `None` to keep the threads of llama.cpp: on a GPU backend, and when the cores are unknown.
pub(super) fn threads_for(backend: Backend, cpu_cores: Option<CpuCores>) -> Option<CpuThreads> {
    match backend {
        Backend::Cpu => cpu_cores.map(CpuThreads::for_cores),
        Backend::Vulkan | Backend::Cuda | Backend::Metal => None,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    use super::*;
    use crate::worker::start_backend;

    fn device(index: usize, backend: &str, device_type: LlamaBackendDeviceType) -> LlamaBackendDevice {
        LlamaBackendDevice {
            index,
            name: format!("{backend}{index}"),
            description: String::new(),
            backend: backend.to_owned(),
            memory_total: 0,
            memory_free: 0,
            device_type,
        }
    }

    fn gpu_index(devices: &[LlamaBackendDevice], backend: Backend) -> Option<usize> {
        gpu_device(devices, backend).map(|device| device.index)
    }

    #[test]
    fn discrete_gpu_runs_the_backend_before_an_integrated_one() {
        let devices = [
            device(0, "CPU", LlamaBackendDeviceType::Cpu),
            device(1, "Vulkan", LlamaBackendDeviceType::IntegratedGpu),
            device(2, "Vulkan", LlamaBackendDeviceType::Gpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Vulkan), Some(2));
    }

    #[test]
    fn integrated_gpu_runs_the_backend_without_a_discrete_one() {
        let devices = [
            device(0, "Vulkan", LlamaBackendDeviceType::IntegratedGpu),
            device(1, "CPU", LlamaBackendDeviceType::Cpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Vulkan), Some(0));
    }

    #[test]
    fn gpu_of_another_backend_does_not_run_it() {
        let devices = [
            device(0, "CUDA", LlamaBackendDeviceType::Gpu),
            device(1, "CPU", LlamaBackendDeviceType::Cpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Vulkan), None);
    }

    #[test]
    fn cuda_backend_runs_on_a_cuda_device() {
        let devices = [
            device(0, "Vulkan", LlamaBackendDeviceType::Gpu),
            device(1, "CUDA", LlamaBackendDeviceType::Gpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Cuda), Some(1));
    }

    #[test]
    fn cpu_backend_has_no_gpu() {
        let devices = [device(0, "Vulkan", LlamaBackendDeviceType::Gpu)];

        assert_eq!(gpu_index(&devices, Backend::Cpu), None);
    }

    fn cpu_cores(performance: usize, efficiency: usize) -> CpuCores {
        CpuCores {
            performance: NonZeroUsize::new(performance).expect("a test CPU has a performance core"),
            efficiency,
        }
    }

    #[test]
    fn prompt_runs_on_each_performance_core_and_on_at_least_four_cores() {
        let cores = [
            cpu_cores(1, 0),
            cpu_cores(2, 0),
            cpu_cores(2, 8),
            cpu_cores(4, 8),
            cpu_cores(8, 8),
            cpu_cores(16, 0),
        ];

        let threads = cores.map(|cores| CpuThreads::for_cores(cores).prompt);

        assert_eq!(threads, [1, 2, 4, 4, 8, 16]);
    }

    #[test]
    fn each_added_token_runs_on_at_most_four_threads() {
        let cores = [cpu_cores(2, 0), cpu_cores(2, 8), cpu_cores(16, 0)];

        let threads = cores.map(|cores| CpuThreads::for_cores(cores).generation);

        assert_eq!(threads, [2, 4, 4]);
    }

    #[test]
    fn cpu_backend_runs_the_threads_for_its_cores() {
        let threads = threads_for(Backend::Cpu, Some(cpu_cores(8, 8)));

        assert_eq!(
            threads,
            Some(CpuThreads {
                prompt: 8,
                generation: 4
            })
        );
    }

    #[test]
    fn gpu_backends_and_unknown_cores_keep_the_threads_of_llama_cpp() {
        let loads = [
            (Backend::Cpu, None),
            (Backend::Vulkan, Some(cpu_cores(8, 8))),
            (Backend::Cuda, Some(cpu_cores(8, 8))),
            (Backend::Metal, Some(cpu_cores(4, 6))),
        ];

        let threads = loads.map(|(backend, cores)| threads_for(backend, cores));

        assert_eq!(threads, [None; 4]);
    }

    fn setup(cpu_threads: Option<CpuThreads>, batch_tokens: u32) -> ContextSetup {
        ContextSetup {
            cpu_threads,
            batch_tokens,
            warm_up: WarmUp::Off,
        }
    }

    #[test]
    fn context_runs_the_cpu_threads() {
        let threads = CpuThreads {
            prompt: 8,
            generation: 4,
        };

        let params = context_params(CONTEXT_TOKENS.0, setup(Some(threads), 128));

        assert_eq!((params.n_threads_batch(), params.n_threads()), (8, 4));
    }

    #[test]
    fn context_without_cpu_threads_keeps_the_threads_of_llama_cpp() {
        let defaults = LlamaContextParams::default();

        let params = context_params(CONTEXT_TOKENS.0, setup(None, 128));

        assert_eq!(
            (params.n_threads_batch(), params.n_threads()),
            (defaults.n_threads_batch(), defaults.n_threads())
        );
    }

    #[test]
    fn context_decodes_a_prompt_in_batches_of_its_setup() {
        let params = context_params(CONTEXT_TOKENS.0, setup(None, 512));

        assert_eq!((params.n_batch(), params.n_ubatch()), (512, 512));
    }

    #[test]
    fn fixed_batch_replaces_the_batch_of_each_backend() {
        let batch = PromptBatch::Fixed(NonZeroU32::new(256).expect("256 is not zero"));
        let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda, Backend::Metal];

        let tokens = backends.map(|backend| batch.tokens_on(backend));

        assert_eq!(tokens, [256; 4]);
    }

    #[test]
    fn gpu_backends_decode_a_prompt_in_larger_batches() {
        let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda, Backend::Metal];

        let tokens = backends.map(|backend| PromptBatch::OfBackend.tokens_on(backend));

        assert_eq!(tokens, [128, 256, 256, 256]);
    }

    #[test]
    fn only_a_load_on_a_gpu_backend_warms_up() {
        let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda, Backend::Metal];

        let warm_ups = backends.map(WarmUp::on);

        assert_eq!(
            warm_ups,
            [WarmUp::Off, WarmUp::AtLoad, WarmUp::AtLoad, WarmUp::AtLoad]
        );
    }

    #[test]
    fn only_a_fit_that_keeps_every_layer_on_the_gpu_passes() {
        let fits = [Ok(true), Ok(false), Err(FitError::Failure), Err(FitError::Error)];

        let verdicts = fits.map(fit_verdict);

        assert_eq!(
            verdicts,
            [
                Ok(()),
                Err(InferenceError::ModelTooLarge),
                Err(InferenceError::ModelTooLarge),
                Err(InferenceError::ModelInvalid),
            ]
        );
    }

    #[test]
    #[ignore = "needs a GPU build, a GPU and the path of a GGUF model file in TYPEALIKE_TEST_MODEL"]
    fn model_does_not_fit_when_the_margin_takes_all_gpu_memory() {
        const ALL_MEMORY_BYTES: usize = 1 << 50;
        let path =
            PathBuf::from(std::env::var_os("TYPEALIKE_TEST_MODEL").expect("TYPEALIKE_TEST_MODEL is set"));
        let mut llama = None;
        start_backend(&mut llama).expect("llama.cpp starts");
        let devices = list_llama_ggml_backend_devices();
        let device = gpu_device(&devices, Backend::Vulkan)
            .or_else(|| gpu_device(&devices, Backend::Cuda))
            .expect("the build has a GPU backend and the computer has a GPU");

        let verdict = check_fit(&path, device.index, ALL_MEMORY_BYTES, BATCH_TOKENS);

        assert_eq!(verdict, Err(InferenceError::ModelTooLarge));
    }
}
