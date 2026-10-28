//! Models TypeAlike suggests: GGUF files with fewer than 10B parameters that download without an account.

use crate::hub::{ByteSize, ModelFile, SizedFile};
use crate::inference::{Backend, GPU_MEMORY_MARGIN};

/// A model the Models pane recommends, with the one file to download for it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct SuggestedModel {
    pub(super) name: &'static str,
    repo: &'static str,
    file_name: &'static str,
    pub(super) size: ByteSize,
    pub(super) developer: &'static str,
}

impl SuggestedModel {
    pub(super) fn sized_file(&self) -> SizedFile {
        SizedFile {
            file: ModelFile {
                repo: self.repo.to_owned(),
                name: self.file_name.to_owned(),
            },
            size: self.size,
        }
    }
}

/// The model that TypeAlike recommends when the models run on `backend`, with the
/// `free_gpu_memory` of that backend when it is known. On the processor, a model with less than
/// one billion parameters, so that suggestions show soon. On a graphics card, the first model of
/// [`GRAPHICS_CARD_MODELS`] that fits in the free memory, else the model for the processor. When the
/// free memory is unknown, a model that fits in the memory of most graphics cards.
pub(super) fn recommended_model(
    backend: Backend,
    free_gpu_memory: Option<ByteSize>,
) -> &'static SuggestedModel {
    let index = match (backend, free_gpu_memory) {
        (Backend::Cpu, _) => PROCESSOR_MODEL,
        (Backend::Vulkan | Backend::Cuda | Backend::Metal, None) => GRAPHICS_CARD_MODEL,
        (Backend::Vulkan | Backend::Cuda | Backend::Metal, Some(free)) => GRAPHICS_CARD_MODELS
            .into_iter()
            .find(|&index| fits(&SUGGESTED_MODELS[index], free))
            .unwrap_or(PROCESSOR_MODEL),
    };
    &SUGGESTED_MODELS[index]
}

/// Whether `model` fits in the `free` memory of a GPU with its context, and leaves the margin of a
/// load free.
fn fits(model: &SuggestedModel, free: ByteSize) -> bool {
    model.size.0 + CONTEXT_MEMORY.0 + GPU_MEMORY_MARGIN.0 <= free.0
}

/// The GPU memory that a suggested model takes besides its file: its context and the buffers of a
/// decode. Gemma 4 E4B takes the most, 480 MiB on Metal.
const CONTEXT_MEMORY: ByteSize = ByteSize(512 * 1024 * 1024);
/// The index of the model for the processor in [`SUGGESTED_MODELS`]: Qwen3.5 0.8B.
const PROCESSOR_MODEL: usize = 0;
/// The index in [`SUGGESTED_MODELS`] of the model for a graphics card with unknown free memory:
/// Gemma 4 E2B.
const GRAPHICS_CARD_MODEL: usize = 2;
/// The indexes in [`SUGGESTED_MODELS`] of the models for a graphics card, from the most preferred:
/// Gemma 4 E4B, Gemma 4 E2B and Qwen3.5 2B.
const GRAPHICS_CARD_MODELS: [usize; 3] = [4, GRAPHICS_CARD_MODEL, 1];

pub(super) const SUGGESTED_MODELS: [SuggestedModel; 6] = [
    SuggestedModel {
        name: "Qwen3.5 0.8B",
        repo: "mradermacher/Qwen3.5-0.8B-Base-i1-GGUF",
        file_name: "Qwen3.5-0.8B-Base.i1-IQ4_NL.gguf",
        size: ByteSize(516_002_176),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Qwen3.5 2B",
        repo: "mradermacher/Qwen3.5-2B-Base-i1-GGUF",
        file_name: "Qwen3.5-2B-Base.i1-Q4_K_S.gguf",
        size: ByteSize(1_212_055_936),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Gemma 4 E2B",
        repo: "unsloth/gemma-4-E2B-it-qat-GGUF",
        file_name: "gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf",
        size: ByteSize(2_620_370_976),
        developer: "Google",
    },
    SuggestedModel {
        name: "Qwen3.5 4B",
        repo: "mradermacher/Qwen3.5-4B-Base-i1-GGUF",
        file_name: "Qwen3.5-4B-Base.i1-Q4_K_S.gguf",
        size: ByteSize(2_563_888_384),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Gemma 4 E4B",
        repo: "unsloth/gemma-4-E4B-it-qat-GGUF",
        file_name: "gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf",
        size: ByteSize(4_215_695_776),
        developer: "Google",
    },
    SuggestedModel {
        name: "Qwen3.5 9B",
        repo: "mradermacher/Qwen3.5-9B-Base-i1-GGUF",
        file_name: "Qwen3.5-9B-Base.i1-Q4_K_S.gguf",
        size: ByteSize(5_351_630_144),
        developer: "Alibaba",
    },
];
