//! Writes the output layer of a model file in 4 bits.
//!
//! Each added token reads the whole output layer. Most model files keep this layer in 6 or 8 bits
//! for each weight, and in a small model with a large vocabulary it is up to half of the weights
//! that each token reads. In 4 bits, each token reads less memory, and on the CPU llama.cpp runs
//! the layer with faster code. The prompt eval showed almost no change in the suggestions.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr::{self, NonNull};
use std::time::Instant;
use std::{fs, io};

use llama_cpp_sys_2::{
    GGML_TYPE_COUNT, GGML_TYPE_IQ4_NL, GGUF_TYPE_UINT32, ggml_blck_size, ggml_is_quantized, ggml_type,
    ggml_type_size, gguf_context, gguf_find_key, gguf_free, gguf_get_kv_type, gguf_get_n_tensors,
    gguf_get_tensor_name, gguf_get_tensor_type, gguf_get_val_u32, gguf_init_from_file, gguf_init_params,
    llama_ftype, llama_model_quantize, llama_model_quantize_default_params, llama_model_tensor_override,
};

/// The type of the output layer after the rewrite. In the prompt eval, it changed the suggestions
/// least of the 4-bit types, and llama.cpp runs it with fast code on x86 and ARM CPUs.
const OUTPUT_TYPE: ggml_type = GGML_TYPE_IQ4_NL;
/// The most bytes of tensor rows that the rewrite keeps in memory at one time. The default of
/// llama.cpp is 8 GiB.
const MAX_REWRITE_BUFFER_BYTES: usize = 256 * 1024 * 1024;
/// The end of the name of the new file until it replaces the model file. It is not the extension
/// of a model or of a download, so the models directory does not list it.
const REWRITE_SUFFIX: &str = ".rewrite";
const OUTPUT_NAME: &str = "output.weight";
const TOKEN_EMBEDDING_NAME: &str = "token_embd.weight";
const FILE_TYPE_KEY: &CStr = c"general.file_type";
const REGEX_METACHARACTERS: &str = r"\^$.|?*+()[]{}";

/// Why the output layer of a model file stays as it is.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OutputLayerError {
    #[error("the path of the model is not UTF-8")]
    Path,
    #[error("the file is not a GGUF model")]
    NotGguf,
    #[error("llama.cpp failed to write the output layer")]
    Rewrite,
    #[error("failed to replace the model file")]
    Replace(#[source] io::Error),
}

/// Replaces the model file at `path` with a copy that has its output layer in 4 bits, when the layer
/// is quantized with more bits. Each other tensor keeps its type. An output layer with 4 bits or
/// fewer, or one that is not quantized, stays. After a failure, the file stays as it is.
pub(crate) fn write_in_4_bits(path: &Path) -> Result<(), OutputLayerError> {
    let source = c_path(path)?;
    let header = ModelHeader::read(&source)?;
    let (Some(file_type), Some(output)) = (header.file_type, larger_output_layer(&header.tensors)) else {
        return Ok(());
    };
    let started = Instant::now();
    let rewritten = rewrite_path(path);
    let result = c_path(&rewritten)
        .and_then(|destination| {
            rewrite(
                &source,
                &destination,
                file_type,
                &kept_types(&header.tensors, output),
            )
        })
        .and_then(|()| fs::rename(&rewritten, path).map_err(OutputLayerError::Replace));
    if result.is_err()
        && let Err(error) = fs::remove_file(&rewritten)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to remove a rewritten model file");
    }
    result?;
    tracing::info!(
        seconds = started.elapsed().as_secs_f32(),
        "wrote the output layer of the model in 4 bits"
    );
    Ok(())
}

/// The type of the file and the name and type of each tensor, from the header of a model file.
struct ModelHeader {
    file_type: Option<llama_ftype>,
    tensors: Vec<(String, ggml_type)>,
}

impl ModelHeader {
    fn read(path: &CStr) -> Result<Self, OutputLayerError> {
        let params = gguf_init_params {
            no_alloc: true,
            ctx: ptr::null_mut(),
        };
        // SAFETY: `path` is a C string. Without a context to fill, llama.cpp reads only the header.
        let context = unsafe { gguf_init_from_file(path.as_ptr(), params) };
        let context = GgufContext(NonNull::new(context).ok_or(OutputLayerError::NotGguf)?);
        Ok(Self {
            file_type: context.file_type(),
            tensors: context.tensors(),
        })
    }
}

/// A header that llama.cpp read, freed on drop.
struct GgufContext(NonNull<gguf_context>);

impl GgufContext {
    fn file_type(&self) -> Option<llama_ftype> {
        let context = self.0.as_ptr();
        // SAFETY: `context` is a live header and the key is a C string.
        let key = unsafe { gguf_find_key(context, FILE_TYPE_KEY.as_ptr()) };
        // SAFETY: llama.cpp found `key` in `context`.
        let is_u32 = key >= 0 && unsafe { gguf_get_kv_type(context, key) } == GGUF_TYPE_UINT32;
        // SAFETY: `key` holds a u32. llama.cpp stops the process for a key of another type.
        let file_type = is_u32.then(|| unsafe { gguf_get_val_u32(context, key) })?;
        // The enum is `i32` with MSVC and `u32` with clang, so the conversion goes through `i64`.
        llama_ftype::try_from(i64::from(file_type)).ok()
    }

    fn tensors(&self) -> Vec<(String, ggml_type)> {
        let context = self.0.as_ptr();
        // SAFETY: `context` is a live header.
        let count = unsafe { gguf_get_n_tensors(context) };
        (0..count)
            .map(|index| {
                // SAFETY: `index` is below the tensor count of `context`. The name is a C string
                // that lives as long as `context`, and the copy below outlives neither.
                let name = unsafe { CStr::from_ptr(gguf_get_tensor_name(context, index)) };
                // SAFETY: `index` is below the tensor count of `context`.
                let kind = unsafe { gguf_get_tensor_type(context, index) };
                (name.to_string_lossy().into_owned(), kind)
            })
            .collect()
    }
}

impl Drop for GgufContext {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `gguf_init_from_file`, and only this drop frees it.
        unsafe { gguf_free(self.0.as_ptr()) };
    }
}

/// Returns the name of the output layer of a model with `tensors`, when it is quantized with more
/// bits than [`OUTPUT_TYPE`]. A model without its own output layer uses its token embeddings.
fn larger_output_layer(tensors: &[(String, ggml_type)]) -> Option<&str> {
    let named = |wanted: &str| tensors.iter().find(|(name, _)| name == wanted);
    let (name, kind) = named(OUTPUT_NAME).or_else(|| named(TOKEN_EMBEDDING_NAME))?;
    has_more_bits_than_the_output_type(*kind).then_some(name.as_str())
}

/// Whether `kind` is a quantized type with more bits for each weight than [`OUTPUT_TYPE`].
fn has_more_bits_than_the_output_type(kind: ggml_type) -> bool {
    // SAFETY: `kind` comes from a model header that llama.cpp checked, or is a constant. The
    // function only reads a table of types.
    let is_quantized = unsafe { ggml_is_quantized(kind) };
    let (bytes, weights) = block_size(kind);
    let (output_bytes, output_weights) = block_size(OUTPUT_TYPE);
    is_quantized && bytes * output_weights > output_bytes * weights
}

/// Returns the bytes of one block of `kind` and the weights in that block.
fn block_size(kind: ggml_type) -> (u64, u64) {
    // SAFETY: `kind` comes from a model header that llama.cpp checked, or is a constant. The
    // functions only read a table of types.
    let (bytes, weights) = unsafe { (ggml_type_size(kind), ggml_blck_size(kind)) };
    (
        u64::try_from(bytes).expect("a block size fits in u64"),
        u64::try_from(weights).expect("a block holds a positive count of weights"),
    )
}

/// Returns a pattern for each tensor except `output`, with the type that it has now. llama.cpp
/// otherwise selects new types for some tensors.
fn kept_types(tensors: &[(String, ggml_type)], output: &str) -> Vec<(String, ggml_type)> {
    tensors
        .iter()
        .filter(|(name, _)| name != output)
        .map(|(name, kind)| (exact_pattern(name), *kind))
        .collect()
}

/// Returns a regular expression that matches only `name`.
fn exact_pattern(name: &str) -> String {
    let mut pattern = String::from("^");
    for character in name.chars() {
        if REGEX_METACHARACTERS.contains(character) {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('$');
    pattern
}

/// Writes the model at `source` to `destination`, with the output layer in [`OUTPUT_TYPE`] and
/// each tensor of `kept` in its type.
fn rewrite(
    source: &CStr,
    destination: &CStr,
    file_type: llama_ftype,
    kept: &[(String, ggml_type)],
) -> Result<(), OutputLayerError> {
    let patterns: Vec<_> = kept
        .iter()
        .map(|(pattern, kind)| {
            let pattern = CString::new(pattern.as_str()).expect("a name from a model header has no nul");
            (pattern, *kind)
        })
        .collect();
    let mut overrides: Vec<_> = patterns
        .iter()
        .map(|(pattern, kind)| llama_model_tensor_override {
            pattern: pattern.as_ptr(),
            type_: *kind,
        })
        .collect();
    overrides.push(llama_model_tensor_override {
        pattern: ptr::null(),
        type_: GGML_TYPE_COUNT,
    });
    // SAFETY: the function only returns a struct of values.
    let mut params = unsafe { llama_model_quantize_default_params() };
    params.ftype = file_type;
    params.allow_requantize = true;
    params.output_tensor_type = OUTPUT_TYPE;
    params.tt_overrides = overrides.as_ptr();
    params.max_buf_size = MAX_REWRITE_BUFFER_BYTES;
    // SAFETY: the paths are C strings. `overrides` ends with a null pattern, and it and the
    // patterns that it points to live until the call returns.
    let status = unsafe { llama_model_quantize(source.as_ptr(), destination.as_ptr(), &raw const params) };
    if status == 0 {
        Ok(())
    } else {
        Err(OutputLayerError::Rewrite)
    }
}

fn rewrite_path(path: &Path) -> PathBuf {
    let mut rewritten = path.as_os_str().to_owned();
    rewritten.push(REWRITE_SUFFIX);
    PathBuf::from(rewritten)
}

fn c_path(path: &Path) -> Result<CString, OutputLayerError> {
    path.to_str()
        .and_then(|text| CString::new(text).ok())
        .ok_or(OutputLayerError::Path)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};

    use ghosttype_core::hub::ModelFile;
    use ghosttype_core::inference::{InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner};
    use llama_cpp_sys_2::{
        GGML_TYPE_BF16, GGML_TYPE_F16, GGML_TYPE_F32, GGML_TYPE_IQ4_XS, GGML_TYPE_Q3_K, GGML_TYPE_Q4_0,
        GGML_TYPE_Q4_K, GGML_TYPE_Q5_0, GGML_TYPE_Q5_K, GGML_TYPE_Q6_K, GGML_TYPE_Q8_0,
    };

    use super::*;

    fn header_tensors(path: &Path) -> Vec<(String, ggml_type)> {
        ModelHeader::read(&c_path(path).expect("the path is UTF-8"))
            .expect("the file is a model")
            .tensors
    }

    fn tensors(list: &[(&str, ggml_type)]) -> Vec<(String, ggml_type)> {
        list.iter().map(|&(name, kind)| (name.to_owned(), kind)).collect()
    }

    #[test]
    fn only_quantized_types_with_more_than_4_bits_need_a_smaller_output_layer() {
        let kinds = [
            GGML_TYPE_Q8_0,
            GGML_TYPE_Q6_K,
            GGML_TYPE_Q5_K,
            GGML_TYPE_Q5_0,
            GGML_TYPE_Q4_K,
            GGML_TYPE_Q4_0,
            GGML_TYPE_IQ4_NL,
            GGML_TYPE_IQ4_XS,
            GGML_TYPE_Q3_K,
            GGML_TYPE_F16,
            GGML_TYPE_BF16,
            GGML_TYPE_F32,
        ];

        let larger = kinds.map(has_more_bits_than_the_output_type);

        assert_eq!(
            larger,
            [
                true, true, true, true, false, false, false, false, false, false, false, false
            ]
        );
    }

    #[test]
    fn model_without_its_own_output_layer_uses_its_token_embeddings() {
        let model = tensors(&[
            ("token_embd.weight", GGML_TYPE_Q8_0),
            ("blk.0.attn_q.weight", GGML_TYPE_Q4_0),
        ]);

        let output = larger_output_layer(&model);

        assert_eq!(output, Some("token_embd.weight"));
    }

    #[test]
    fn own_output_layer_comes_before_the_token_embeddings() {
        let model = tensors(&[
            ("token_embd.weight", GGML_TYPE_Q6_K),
            ("output.weight", GGML_TYPE_Q6_K),
        ]);

        let output = larger_output_layer(&model);

        assert_eq!(output, Some("output.weight"));
    }

    #[test]
    fn output_layer_in_4_bits_stays() {
        let model = tensors(&[
            ("token_embd.weight", GGML_TYPE_Q6_K),
            ("output.weight", GGML_TYPE_Q4_K),
        ]);

        let output = larger_output_layer(&model);

        assert_eq!(output, None);
    }

    #[test]
    fn each_other_tensor_keeps_its_type() {
        let model = tensors(&[
            ("token_embd.weight", GGML_TYPE_Q6_K),
            ("per_layer_token_embd.weight", GGML_TYPE_Q6_K),
            ("blk.0.ffn_up.weight", GGML_TYPE_Q4_0),
        ]);

        let kept = kept_types(&model, "token_embd.weight");

        assert_eq!(
            kept,
            tensors(&[
                (r"^per_layer_token_embd\.weight$", GGML_TYPE_Q6_K),
                (r"^blk\.0\.ffn_up\.weight$", GGML_TYPE_Q4_0),
            ])
        );
    }

    #[test]
    fn pattern_matches_only_the_exact_name() {
        let pattern = exact_pattern("a.b+c(d)[e]");

        assert_eq!(pattern, r"^a\.b\+c\(d\)\[e\]$");
    }

    #[test]
    fn file_that_is_not_a_model_stays_as_it_is() {
        let path =
            std::env::temp_dir().join(format!("ghosttype-llm-{}-not-a-model.gguf", std::process::id()));
        fs::write(&path, b"not a model").expect("temp dir is writable");

        let result = write_in_4_bits(&path);

        let content = fs::read(&path).expect("the file stays");
        fs::remove_file(&path).ok();
        assert!(
            matches!(result, Err(OutputLayerError::NotGguf)),
            "unexpected {result:?}"
        );
        assert_eq!(content, b"not a model");
    }

    #[test]
    #[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
    fn loading_a_model_writes_its_output_layer_in_4_bits_and_keeps_its_other_tensors() {
        let original =
            PathBuf::from(std::env::var_os("GHOSTTYPE_TEST_MODEL").expect("GHOSTTYPE_TEST_MODEL is set"));
        let copy =
            std::env::temp_dir().join(format!("ghosttype-llm-{}-output-layer.gguf", std::process::id()));
        fs::copy(&original, &copy).expect("temp dir is writable");
        let located = copy.clone();
        let mut runner = crate::LlamaCpp::new(move |_| Some(located.clone()), None);
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });

        runner.submit(
            InferenceRequest::Load {
                file: ModelFile {
                    repo: "test/model".to_owned(),
                    name: "model.gguf".to_owned(),
                },
                backend: crate::BUILT_IN_BACKEND,
            },
            &sink,
        );
        let loaded = events.recv().expect("the runner answers the load");
        drop(runner);

        let before = header_tensors(&original);
        let after = header_tensors(&copy);
        fs::remove_file(&copy).ok();
        let output = larger_output_layer(&before).expect("the test model has a large output layer");
        let output_type = after
            .iter()
            .find(|(name, _)| name == output)
            .map(|(_, kind)| *kind);
        assert!(
            matches!(loaded, InferenceEvent::Loaded { result: Ok(()), .. }),
            "unexpected {loaded:?}"
        );
        assert_eq!(output_type, Some(OUTPUT_TYPE));
        assert_eq!(kept_types(&after, output), kept_types(&before, output));
    }
}
