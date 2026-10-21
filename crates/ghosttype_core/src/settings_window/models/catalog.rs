//! Models GhostType suggests: GGUF files with fewer than 10B parameters that download without an account.

use crate::hub::{ByteSize, ModelFile, SizedFile};

/// A model the Models pane recommends, with the one file to download for it.
#[derive(Debug)]
pub(super) struct SuggestedModel {
    pub(super) name: &'static str,
    parameters: &'static str,
    repo: &'static str,
    file_name: &'static str,
    pub(super) size: ByteSize,
    developer: &'static str,
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

    pub(super) fn description(&self) -> String {
        format!("{} · {} parameters", self.developer, self.parameters)
    }
}

pub(super) const SUGGESTED_MODELS: [SuggestedModel; 6] = [
    SuggestedModel {
        name: "Qwen3.5 0.8B",
        parameters: "0.8B",
        repo: "mradermacher/Qwen3.5-0.8B-Base-i1-GGUF",
        file_name: "Qwen3.5-0.8B-Base.i1-IQ4_NL.gguf",
        size: ByteSize(516_002_176),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Qwen3.5 2B",
        parameters: "2B",
        repo: "mradermacher/Qwen3.5-2B-Base-i1-GGUF",
        file_name: "Qwen3.5-2B-Base.i1-Q4_K_S.gguf",
        size: ByteSize(1_212_055_936),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Gemma 4 E2B",
        parameters: "2B effective",
        repo: "unsloth/gemma-4-E2B-it-qat-GGUF",
        file_name: "gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf",
        size: ByteSize(2_620_370_976),
        developer: "Google",
    },
    SuggestedModel {
        name: "Qwen3.5 4B",
        parameters: "4B",
        repo: "mradermacher/Qwen3.5-4B-Base-i1-GGUF",
        file_name: "Qwen3.5-4B-Base.i1-Q4_K_S.gguf",
        size: ByteSize(2_563_888_384),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Gemma 4 E4B",
        parameters: "4B effective",
        repo: "unsloth/gemma-4-E4B-it-qat-GGUF",
        file_name: "gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf",
        size: ByteSize(4_215_695_776),
        developer: "Google",
    },
    SuggestedModel {
        name: "Qwen3.5 9B",
        parameters: "9B",
        repo: "mradermacher/Qwen3.5-9B-Base-i1-GGUF",
        file_name: "Qwen3.5-9B-Base.i1-Q4_K_S.gguf",
        size: ByteSize(5_351_630_144),
        developer: "Alibaba",
    },
];
