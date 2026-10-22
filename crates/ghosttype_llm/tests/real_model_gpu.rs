//! Completes text with a real GGUF model on each GPU backend. Run it from a build with the GPU
//! features, on a computer with a GPU for each backend:
//! `GHOSTTYPE_TEST_MODEL=/path/to/model.gguf cargo test -p ghosttype_llm --features vulkan,cuda -- --ignored`.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Backend, Completion, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, NextWordBias,
    Prompt, TokenCount, TypoCheck,
};
use ghosttype_llm::LlamaCpp;

fn completion_events_on(backend: Backend) -> (InferenceEvent, InferenceEvent) {
    let path = PathBuf::from(std::env::var_os("GHOSTTYPE_TEST_MODEL").expect("GHOSTTYPE_TEST_MODEL is set"));
    let file = ModelFile {
        repo: "test/model".to_owned(),
        name: "model.gguf".to_owned(),
    };
    let mut runner = LlamaCpp::new(move |_| Some(path.clone()), None);
    let (sender, events) = mpsc::channel();
    let sink: InferenceEventSink = Arc::new(move |event| {
        sender.send(event).ok();
    });
    runner.submit(InferenceRequest::Load { file, backend }, &sink);
    runner.submit(
        InferenceRequest::Complete {
            prompt: Prompt("Once upon a time".to_owned()),
            max_tokens: TokenCount(8),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            line_after: String::new(),
        },
        &sink,
    );
    drop(sink);
    let loaded = events.recv().expect("the runner answers the load");
    let completed = events
        .iter()
        .find(|event| !matches!(event, InferenceEvent::Partial(_)))
        .expect("the runner answers the completion");
    (loaded, completed)
}

fn is_completed_text(event: &InferenceEvent) -> bool {
    matches!(
        event,
        InferenceEvent::Completed(Ok(Completion {
            tokens: TokenCount(1..=8),
            ..
        }))
    )
}

#[test]
#[ignore = "needs the vulkan feature, a Vulkan GPU and a GGUF model in GHOSTTYPE_TEST_MODEL"]
fn real_model_continues_text_on_vulkan() {
    let (loaded, completed) = completion_events_on(Backend::Vulkan);

    assert!(
        matches!(loaded, InferenceEvent::Loaded { result: Ok(()), .. }),
        "unexpected {loaded:?}"
    );
    assert!(is_completed_text(&completed), "unexpected {completed:?}");
}

#[test]
#[ignore = "needs the cuda feature, an NVIDIA GPU and a GGUF model in GHOSTTYPE_TEST_MODEL"]
fn real_model_continues_text_on_cuda() {
    let (loaded, completed) = completion_events_on(Backend::Cuda);

    assert!(
        matches!(loaded, InferenceEvent::Loaded { result: Ok(()), .. }),
        "unexpected {loaded:?}"
    );
    assert!(is_completed_text(&completed), "unexpected {completed:?}");
}
