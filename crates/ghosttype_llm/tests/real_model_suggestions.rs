//! Suggests text with a real GGUF model while the user types. Run it with
//! `GHOSTTYPE_TEST_MODEL=/path/to/model.gguf cargo test -p ghosttype_llm -- --ignored --test-threads=1`.
//! One test at a time, because llama.cpp starts only once in a process.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use ghosttype_core::app::{App, Field};
use ghosttype_core::desktop::{Desktop, Surroundings};
use ghosttype_core::engine::Reaction;
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, Prompt,
    TokenCount,
};
use ghosttype_core::key::Key;
use ghosttype_llm::LlamaCpp;

struct Editor;

impl Desktop for Editor {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings::default()
    }

    fn clipboard_text(&self) -> Option<String> {
        None
    }
}

struct LoadedModel {
    runner: LlamaCpp,
    sink: InferenceEventSink,
    events: mpsc::Receiver<InferenceEvent>,
}

impl LoadedModel {
    fn load() -> Self {
        let path =
            PathBuf::from(std::env::var_os("GHOSTTYPE_TEST_MODEL").expect("GHOSTTYPE_TEST_MODEL is set"));
        let mut runner = LlamaCpp::new(move |_| Some(path.clone()));
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });
        let file = ModelFile {
            repo: "test/model".to_owned(),
            name: "model.gguf".to_owned(),
        };
        runner.submit(
            InferenceRequest::Load {
                file: file.clone(),
                backend: None,
            },
            &sink,
        );
        let loaded = events.recv().expect("the runner answers the load");
        assert_eq!(loaded, InferenceEvent::Loaded { file, result: Ok(()) });
        Self { runner, sink, events }
    }

    fn complete(&mut self, reaction: Reaction) -> Result<Completion, InferenceError> {
        let Reaction::Request(request) = reaction else {
            panic!("expected a request, got {reaction:?}");
        };
        self.runner.submit(request, &self.sink);
        let event = self.events.recv().expect("the runner answers the completion");
        let InferenceEvent::Completed(result) = event else {
            panic!("expected a completion, got {event:?}");
        };
        result
    }
}

const SHORT_PROMPT: &str = "Thank you for the quick reply. I read the new terms with the";
const LONG_PROMPT: &str =
    "Thank you for the quick reply. I read the new terms with the finance team this morning, and we";

fn complete_text(model: &mut LoadedModel, prompt: &str) -> String {
    let request = InferenceRequest::Complete {
        prompt: Prompt(prompt.to_owned()),
        max_tokens: TokenCount(16),
    };
    model
        .complete(Reaction::Request(request))
        .expect("the model completes the prompt")
        .text
}

/// Types `text` into `app` while `model` completes, and returns the reaction to the completion
/// of all of it.
fn suggest_after(model: &mut LoadedModel, app: &mut App, text: &str) -> Reaction {
    let mut characters = text.chars();
    let first = characters.next().expect("the text is not empty");
    let first_request = app.on_key(Key::Char(first), &Editor);
    characters.for_each(|character| assert_eq!(app.on_key(Key::Char(character), &Editor), Reaction::Hide));
    let stale = app.on_completion(model.complete(first_request));
    app.on_completion(model.complete(stale))
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn real_model_suggests_text_after_typed_words() {
    let mut model = LoadedModel::load();

    let reaction = suggest_after(&mut model, &mut App::new(), "Thank you for your");

    assert!(matches!(reaction, Reaction::Show(_)), "unexpected {reaction:?}");
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn real_model_finishes_a_word_cut_inside_a_token() {
    let mut model = LoadedModel::load();

    let reaction = suggest_after(&mut model, &mut App::new(), "The weather today is beauti");

    assert!(
        matches!(&reaction, Reaction::Show(text) if text.starts_with("ful")),
        "unexpected {reaction:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_that_extends_a_cached_prompt_completes_as_without_the_cache() {
    let mut model = LoadedModel::load();
    let without_cache = complete_text(&mut model, LONG_PROMPT);
    complete_text(&mut model, SHORT_PROMPT);

    let with_cache = complete_text(&mut model, LONG_PROMPT);

    assert_eq!(with_cache, without_cache);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn repeated_prompt_completes_as_the_first_time() {
    let mut model = LoadedModel::load();
    let first = complete_text(&mut model, LONG_PROMPT);

    let repeated = complete_text(&mut model, LONG_PROMPT);

    assert_eq!(repeated, first);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn long_prompt_after_another_long_prompt_completes() {
    let mut model = LoadedModel::load();
    complete_text(
        &mut model,
        &"The quick brown fox jumps over the lazy dog. ".repeat(120),
    );

    let completion = model.complete(Reaction::Request(InferenceRequest::Complete {
        prompt: Prompt("We hold these truths to be self-evident, that all are equal. ".repeat(110)),
        max_tokens: TokenCount(16),
    }));

    assert!(completion.is_ok(), "unexpected {completion:?}");
}
