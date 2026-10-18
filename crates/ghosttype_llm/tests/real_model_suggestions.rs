//! Suggests text with a real GGUF model while the user types. Run it with
//! `GHOSTTYPE_TEST_MODEL=/path/to/model.gguf cargo test -p ghosttype_llm -- --ignored --test-threads=1`.
//! One test at a time, because llama.cpp starts only once in a process.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use ghosttype_core::app::{App, Field, SettledWork};
use ghosttype_core::desktop::{CopiedText, Desktop, Surroundings};
use ghosttype_core::engine::Reaction;
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, Prompt,
    TokenCount,
};
use ghosttype_core::key::Key;
use ghosttype_core::settings::{PromptPreparation, Settings};
use ghosttype_llm::{BUILT_IN_BACKEND, LlamaCpp};

struct Editor;

impl Desktop for Editor {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings::default()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

struct LoadedModel {
    runner: LlamaCpp,
    sink: InferenceEventSink,
    events: mpsc::Receiver<InferenceEvent>,
}

fn model_file() -> ModelFile {
    ModelFile {
        repo: "test/model".to_owned(),
        name: "model.gguf".to_owned(),
    }
}

impl LoadedModel {
    fn load() -> Self {
        let path =
            PathBuf::from(std::env::var_os("GHOSTTYPE_TEST_MODEL").expect("GHOSTTYPE_TEST_MODEL is set"));
        let runner = LlamaCpp::new(move |_| Some(path.clone()), None);
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });
        let mut model = Self { runner, sink, events };
        model.reload();
        model
    }

    fn reload(&mut self) {
        self.runner.submit(
            InferenceRequest::Load {
                file: model_file(),
                backend: BUILT_IN_BACKEND,
            },
            &self.sink,
        );
        let loaded = self.events.recv().expect("the runner answers the load");
        assert_eq!(
            loaded,
            InferenceEvent::Loaded {
                file: model_file(),
                result: Ok(())
            }
        );
    }

    fn submit(&mut self, reaction: Reaction) {
        let Reaction::Request(request) = reaction else {
            panic!("expected a request, got {reaction:?}");
        };
        self.runner.submit(request, &self.sink);
    }

    fn next_completion(&self) -> Result<Completion, InferenceError> {
        self.next_parts_and_completion().1
    }

    /// Returns the texts of the partial completions before the next completion, and the completion.
    fn next_parts_and_completion(&self) -> (Vec<String>, Result<Completion, InferenceError>) {
        let mut parts = Vec::new();
        loop {
            match self.events.recv().expect("the runner answers the completion") {
                InferenceEvent::Partial(text) => parts.push(text),
                InferenceEvent::Completed(result) => return (parts, result),
                event @ InferenceEvent::Loaded { .. } => panic!("expected a completion, got {event:?}"),
            }
        }
    }

    fn complete(&mut self, reaction: Reaction) -> Result<Completion, InferenceError> {
        self.submit(reaction);
        self.next_completion()
    }
}

const SHORT_PROMPT: &str = "Thank you for the quick reply. I read the new terms with the";
const LONG_PROMPT: &str =
    "Thank you for the quick reply. I read the new terms with the finance team this morning, and we";
const ONE_WORD_REMOVED: &str = "Thank you for the quick reply. I read the new terms with";
const TWO_WORDS_REMOVED: &str = "Thank you for the quick reply. I read the new terms";
const LAST_WORD_CHANGED: &str = "Thank you for the quick reply. I read the new terms for the";

fn completion_request(prompt: &str) -> Reaction {
    Reaction::Request(InferenceRequest::Complete {
        prompt: Prompt(prompt.to_owned()),
        max_tokens: TokenCount(16),
        word_bias: None,
    })
}

fn complete_text(model: &mut LoadedModel, prompt: &str) -> String {
    model
        .complete(completion_request(prompt))
        .expect("the model completes the prompt")
        .text
}

/// Types `text` into `app` while `model` completes, and returns the reaction to the completion
/// of all of it.
fn suggest_after(model: &mut LoadedModel, app: &mut App, text: &str) -> Reaction {
    text.chars()
        .map(|character| app.on_key(Key::Char(character), &Editor))
        .for_each(|reaction| model.submit(reaction));
    text.chars()
        .map(|_| app.on_completion(model.next_completion()))
        .last()
        .expect("the text is not empty")
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
fn prompt_without_the_last_word_completes_as_without_the_cache() {
    let mut model = LoadedModel::load();
    let without_cache = complete_text(&mut model, ONE_WORD_REMOVED);
    complete_text(&mut model, SHORT_PROMPT);

    let with_cache = complete_text(&mut model, ONE_WORD_REMOVED);

    assert_eq!(with_cache, without_cache);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_without_the_last_two_words_completes_as_without_the_cache() {
    let mut model = LoadedModel::load();
    let without_cache = complete_text(&mut model, TWO_WORDS_REMOVED);
    complete_text(&mut model, ONE_WORD_REMOVED);
    complete_text(&mut model, SHORT_PROMPT);
    complete_text(&mut model, ONE_WORD_REMOVED);

    let with_cache = complete_text(&mut model, TWO_WORDS_REMOVED);

    assert_eq!(with_cache, without_cache);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_with_another_last_word_completes_as_without_the_cache() {
    let mut model = LoadedModel::load();
    let without_cache = complete_text(&mut model, LAST_WORD_CHANGED);
    complete_text(&mut model, SHORT_PROMPT);

    let with_cache = complete_text(&mut model, LAST_WORD_CHANGED);

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
        word_bias: None,
    }));

    assert!(completion.is_ok(), "unexpected {completion:?}");
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn partial_completions_grow_into_the_completion() {
    let mut model = LoadedModel::load();
    model.submit(completion_request(LONG_PROMPT));

    let (parts, completion) = model.next_parts_and_completion();

    let text = completion.expect("the model completes the prompt").text;
    assert!(
        !parts.is_empty() && parts.iter().all(|part| text.starts_with(part.as_str())),
        "parts {parts:?} do not grow into {text:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn newer_completion_replaces_one_that_did_not_finish() {
    let mut model = LoadedModel::load();
    model.runner.submit(
        InferenceRequest::Load {
            file: model_file(),
            backend: BUILT_IN_BACKEND,
        },
        &model.sink,
    );
    model.submit(completion_request(SHORT_PROMPT));
    model.submit(completion_request(LONG_PROMPT));
    model.events.recv().expect("the runner answers the load");

    let replaced = model.next_completion();
    let newest = model.next_completion();

    assert_eq!(
        (replaced.err(), newest.is_ok()),
        (Some(InferenceError::Superseded), true)
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn completion_of_no_tokens_adds_no_text() {
    let mut model = LoadedModel::load();

    let completion = model.complete(Reaction::Request(InferenceRequest::Complete {
        prompt: Prompt(SHORT_PROMPT.to_owned()),
        max_tokens: TokenCount(0),
        word_bias: None,
    }));

    assert!(
        matches!(&completion, Ok(Completion { text, tokens: TokenCount(0), .. }) if text.is_empty()),
        "unexpected {completion:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn first_key_in_a_prepared_field_suggests_text() {
    let mut model = LoadedModel::load();
    let mut app = App::new();
    app.use_settings(&Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    });
    let Some(SettledWork::Prepare(preparation)) = app.on_input_settled(&Editor) else {
        panic!("the app prepares the open field");
    };
    model.runner.submit(preparation, &model.sink);
    app.on_completion(model.next_completion());

    let reaction = suggest_after(&mut model, &mut app, "Thank you for your");

    assert!(matches!(reaction, Reaction::Show(_)), "unexpected {reaction:?}");
}
