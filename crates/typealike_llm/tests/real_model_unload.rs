//! Unloads a real GGUF model. Run it with
//! `TYPEALIKE_TEST_MODEL=/path/to/model.gguf cargo test -p typealike_llm -- --ignored`.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use typealike_core::hub::ModelFile;
use typealike_core::inference::{
    InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, NextWordBias, Prompt,
    SentenceEnd, TokenCount, TypoCheck,
};
use typealike_llm::{BUILT_IN_BACKEND, LlamaCpp};

#[test]
#[ignore = "needs the path of a GGUF model file in TYPEALIKE_TEST_MODEL"]
fn unloaded_model_completes_no_text() {
    let path = PathBuf::from(std::env::var_os("TYPEALIKE_TEST_MODEL").expect("TYPEALIKE_TEST_MODEL is set"));
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
    runner.submit(InferenceRequest::Unload, &sink);
    runner.submit(
        InferenceRequest::Complete {
            prompt: Prompt("Once upon a time".to_owned()),
            max_tokens: TokenCount(8),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Continue,
            line_after: String::new(),
            first_word_gate: None,
        },
        &sink,
    );
    drop(sink);
    let loaded = events.recv().expect("the runner answers the load");
    let completed = events.recv().expect("the runner answers the completion");

    assert_eq!(
        (loaded, completed),
        (
            InferenceEvent::Loaded { file, result: Ok(()) },
            InferenceEvent::Completed(Err(InferenceError::NoModel)),
        )
    );
}
