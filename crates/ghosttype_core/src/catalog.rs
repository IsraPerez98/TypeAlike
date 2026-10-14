//! Models GhostType suggests: GGUF files with fewer than 10B parameters that download without an account.

use crate::hub::{ByteSize, ModelFile, SizedFile};

/// A model the Models pane recommends, with the one file to download for it.
#[derive(Debug)]
pub(crate) struct SuggestedModel {
    pub(crate) name: &'static str,
    pub(crate) parameters: &'static str,
    pub(crate) repo: &'static str,
    pub(crate) file_name: &'static str,
    pub(crate) size: ByteSize,
    pub(crate) developer: &'static str,
}

impl SuggestedModel {
    pub(crate) fn sized_file(&self) -> SizedFile {
        SizedFile {
            file: ModelFile {
                repo: self.repo.to_owned(),
                name: self.file_name.to_owned(),
            },
            size: self.size,
        }
    }

    pub(crate) fn description(&self) -> String {
        format!("{} · {} parameters", self.developer, self.parameters)
    }
}

pub(crate) const SUGGESTED_MODELS: [SuggestedModel; 7] = [
    SuggestedModel {
        name: "Qwen3.5 0.8B",
        parameters: "0.8B",
        repo: "ggml-org/Qwen3.5-0.8B-Base-GGUF",
        file_name: "Qwen3.5-0.8B-Base-Q4_0.gguf",
        size: ByteSize(563_035_840),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Qwen3.5 2B",
        parameters: "2B",
        repo: "mradermacher/Qwen3.5-2B-Base-GGUF",
        file_name: "Qwen3.5-2B-Base.Q4_K_M.gguf",
        size: ByteSize(1_274_396_800),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Gemma 4 E2B",
        parameters: "2B effective",
        repo: "google/gemma-4-E2B-it-qat-q4_0-gguf",
        file_name: "gemma-4-E2B_q4_0-it.gguf",
        size: ByteSize(3_349_516_256),
        developer: "Google",
    },
    SuggestedModel {
        name: "Granite 4.2 3B",
        parameters: "3B",
        repo: "ibm-granite/granite-4.2-3b-GGUF",
        file_name: "granite-4.2-3b-Q4_K_M.gguf",
        size: ByteSize(2_244_011_552),
        developer: "IBM",
    },
    SuggestedModel {
        name: "Qwen3.5 4B",
        parameters: "4B",
        repo: "mradermacher/Qwen3.5-4B-Base-GGUF",
        file_name: "Qwen3.5-4B-Base.Q4_K_M.gguf",
        size: ByteSize(2_708_804_608),
        developer: "Alibaba",
    },
    SuggestedModel {
        name: "Gemma 4 E4B",
        parameters: "4B effective",
        repo: "google/gemma-4-E4B-it-qat-q4_0-gguf",
        file_name: "gemma-4-E4B_q4_0-it.gguf",
        size: ByteSize(5_154_941_280),
        developer: "Google",
    },
    SuggestedModel {
        name: "Qwen3.5 9B",
        parameters: "9B",
        repo: "mradermacher/Qwen3.5-9B-Base-GGUF",
        file_name: "Qwen3.5-9B-Base.Q4_K_M.gguf",
        size: ByteSize(5_629_109_312),
        developer: "Alibaba",
    },
];
