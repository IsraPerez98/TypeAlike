//! Warms up a real GGUF model at its load on each GPU backend. Run it from a build with the GPU
//! features, on a computer with a GPU for each backend, one test at a time, because llama.cpp
//! starts only one time in a process:
//! `TYPEALIKE_TEST_MODEL=/path/to/model.gguf cargo test -p typealike_llm --features vulkan,cuda
//! --test real_model_warm_up -- --ignored --skip metal --test-threads=1`.
//! On macOS, run only the Metal test, without the features.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use typealike_core::hub::ModelFile;
use typealike_core::inference::{
    Backend, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, NextWordBias, Prompt,
    SentenceEnd, TokenCount, TypoCheck,
};
use typealike_llm::LlamaCpp;

/// When the completion request goes to the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    /// With the load request, so that it waits while the model loads and the load does no warm-up.
    WithLoad,
    /// After the answer of the load, which comes after the warm-up.
    AfterLoad,
}

fn completion_event(backend: Backend, request: Request) -> InferenceEvent {
    let path = PathBuf::from(std::env::var_os("TYPEALIKE_TEST_MODEL").expect("TYPEALIKE_TEST_MODEL is set"));
    let file = ModelFile {
        repo: "test/model".to_owned(),
        name: "model.gguf".to_owned(),
    };
    let complete = InferenceRequest::Complete {
        prompt: Prompt("Thank you for the notes from the meeting. Could we meet on".to_owned()),
        max_tokens: TokenCount(8),
        word_bias: None,
        next_word_bias: NextWordBias::default(),
        min_log_probability: None,
        typo_check: TypoCheck::Off,
        sentence_end: SentenceEnd::Continue,
        line_after: String::new(),
        first_word_gate: None,
    };
    let mut runner = LlamaCpp::new(move |_| Some(path.clone()), None);
    let (sender, events) = mpsc::channel();
    let sink: InferenceEventSink = Arc::new(move |event| {
        sender.send(event).ok();
    });
    runner.submit(InferenceRequest::Load { file, backend }, &sink);
    if request == Request::WithLoad {
        runner.submit(complete.clone(), &sink);
    }
    let loaded = events.recv().expect("the runner answers the load");
    assert!(
        matches!(loaded, InferenceEvent::Loaded { result: Ok(()), .. }),
        "unexpected {loaded:?}"
    );
    if request == Request::AfterLoad {
        runner.submit(complete, &sink);
    }
    drop(sink);
    events
        .iter()
        .find(|event| !matches!(event, InferenceEvent::Partial(_)))
        .expect("the runner answers the completion")
}

fn completed_text(event: InferenceEvent) -> Option<String> {
    match event {
        InferenceEvent::Completed(Ok(completion)) => Some(completion.text),
        _ => None,
    }
}

fn assert_warm_up_keeps_the_completion(backend: Backend) {
    let without_warm_up = completed_text(completion_event(backend, Request::WithLoad));

    let after_warm_up = completed_text(completion_event(backend, Request::AfterLoad));

    assert!(without_warm_up.is_some(), "the completion failed");
    assert_eq!(after_warm_up, without_warm_up);
}

#[test]
#[ignore = "needs the vulkan feature, a Vulkan GPU and a GGUF model in TYPEALIKE_TEST_MODEL"]
fn warm_up_keeps_the_completion_on_vulkan() {
    assert_warm_up_keeps_the_completion(Backend::Vulkan);
}

#[test]
#[ignore = "needs the cuda feature, an NVIDIA GPU and a GGUF model in TYPEALIKE_TEST_MODEL"]
fn warm_up_keeps_the_completion_on_cuda() {
    assert_warm_up_keeps_the_completion(Backend::Cuda);
}

#[test]
#[ignore = "needs macOS and a GGUF model in TYPEALIKE_TEST_MODEL"]
fn warm_up_keeps_the_completion_on_metal() {
    assert_warm_up_keeps_the_completion(Backend::Metal);
}
