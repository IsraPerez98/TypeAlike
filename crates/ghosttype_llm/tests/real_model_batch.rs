//! Completes a prompt of several batches with a real GGUF model. Run it with
//! `GHOSTTYPE_TEST_MODEL=/path/to/model.gguf cargo test -p ghosttype_llm -- --ignored`.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, NextWordBias, Prompt, SentenceEnd,
    TokenCount, TypoCheck,
};
use ghosttype_llm::{BUILT_IN_BACKEND, LlamaCpp};

const SMALL_BATCH_TOKENS: u32 = 8;
const PROMPT: &str = "The library opens at nine in the morning and closes at six in the evening. On \
     Saturdays it opens at ten and closes at two. It is closed on Sundays and on public holidays. \
     To borrow a book, bring your card to the front desk. The library is";

fn completed_text(mut runner: LlamaCpp) -> String {
    let file = ModelFile {
        repo: "test/model".to_owned(),
        name: "model.gguf".to_owned(),
    };
    let (sender, events) = mpsc::channel();
    let sink: InferenceEventSink = Arc::new(move |event| {
        sender.send(event).ok();
    });
    runner.submit(
        InferenceRequest::Load {
            file,
            backend: BUILT_IN_BACKEND,
        },
        &sink,
    );
    runner.submit(
        InferenceRequest::Complete {
            prompt: Prompt(PROMPT.to_owned()),
            max_tokens: TokenCount(4),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Continue,
            line_after: String::new(),
        },
        &sink,
    );
    drop(sink);
    let completed = events
        .iter()
        .find(|event| matches!(event, InferenceEvent::Completed(_)))
        .expect("the runner answers the completion");
    match completed {
        InferenceEvent::Completed(Ok(completion)) => completion.text,
        event => panic!("unexpected {event:?}"),
    }
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_of_several_batches_completes_as_in_one_batch() {
    let path = PathBuf::from(std::env::var_os("GHOSTTYPE_TEST_MODEL").expect("GHOSTTYPE_TEST_MODEL is set"));
    let one_batch_path = path.clone();
    let small_batch = NonZeroU32::new(SMALL_BATCH_TOKENS).expect("the batch is not zero");

    let in_one_batch = completed_text(LlamaCpp::new(move |_| Some(one_batch_path.clone()), None));
    let in_small_batches = completed_text(LlamaCpp::with_prompt_batch(
        move |_| Some(path.clone()),
        None,
        small_batch,
    ));

    assert_eq!(in_small_batches, in_one_batch);
}
