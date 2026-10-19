//! Completes text with a real GGUF model. Run it with
//! `GHOSTTYPE_TEST_MODEL=/path/to/model.gguf cargo test -p ghosttype_llm -- --ignored`.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Completion, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, NextWordBias, Prompt,
    TokenCount,
};
use ghosttype_llm::{BUILT_IN_BACKEND, LlamaCpp};

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn real_model_continues_text() {
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

    runner.submit(
        InferenceRequest::Load {
            file: file.clone(),
            backend: BUILT_IN_BACKEND,
        },
        &sink,
    );
    runner.submit(
        InferenceRequest::Complete {
            prompt: Prompt("Once upon a time".to_owned()),
            max_tokens: TokenCount(8),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
        },
        &sink,
    );
    drop(sink);
    let loaded = events.recv().expect("the runner answers the load");
    let completed = events
        .iter()
        .find(|event| !matches!(event, InferenceEvent::Partial(_)))
        .expect("the runner answers the completion");

    assert_eq!(loaded, InferenceEvent::Loaded { file, result: Ok(()) });
    assert!(
        matches!(
            completed,
            InferenceEvent::Completed(Ok(Completion {
                tokens: TokenCount(1..=8),
                ..
            }))
        ),
        "unexpected {completed:?}"
    );
}
