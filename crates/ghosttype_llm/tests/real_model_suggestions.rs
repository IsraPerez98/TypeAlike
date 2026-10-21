//! Suggests text with a real GGUF model while the user types. Run it with
//! `GHOSTTYPE_TEST_MODEL=/path/to/model.gguf cargo test -p ghosttype_llm -- --ignored --test-threads=1`.
//! One test at a time, because llama.cpp starts only once in a process.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use ghosttype_core::app::{App, SettledWork};
use ghosttype_core::desktop::{CopiedText, Desktop, Field, Surroundings};
use ghosttype_core::engine::Reaction;
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, LogProbability,
    ModelRunner, NextWordBias, Prompt, TokenCount,
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

struct ModelSession {
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

impl ModelSession {
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
        self.next_completion_with_parts().1
    }

    /// Returns the texts of the partial completions before the next completion, and the completion.
    fn next_completion_with_parts(&self) -> (Vec<String>, Result<Completion, InferenceError>) {
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

const BLANK_LINE_PROMPT: &str = "I type the text below on my computer, for example an email.\nWhere I am typing: \
     Format: email\nMy text after the cursor: “On 4 May, Tom Reyes wrote:\n> Hi Sara, here are the April \
     sales numbers that you asked for.”\n\nThe part of my writing that I am typing now:\nHi Tom,\n\n";
const SHORT_PROMPT: &str = "Thank you for the quick reply. I read the new terms with the";
const LONG_PROMPT: &str =
    "Thank you for the quick reply. I read the new terms with the finance team this morning, and we";
const ONE_WORD_REMOVED: &str = "Thank you for the quick reply. I read the new terms with";
const TWO_WORDS_REMOVED: &str = "Thank you for the quick reply. I read the new terms";
const LAST_WORD_CHANGED: &str = "Thank you for the quick reply. I read the new terms for the";

fn completion_request(prompt: &str) -> Reaction {
    request_of(prompt, Typos::Ignore)
}

/// What a request does with a typo in the last word of its prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Typos {
    Ignore,
    Hide,
    Fix,
}

fn request_of(prompt: &str, typos: Typos) -> Reaction {
    Reaction::Request(InferenceRequest::Complete {
        prompt: Prompt(prompt.to_owned()),
        max_tokens: TokenCount(16),
        word_bias: None,
        next_word_bias: NextWordBias::default(),
        min_log_probability: None,
        min_typed_log_probability: (typos != Typos::Ignore).then_some(TYPO_CUT),
        fixes_typos: typos == Typos::Fix,
    })
}

const TYPO_CUT: LogProbability = LogProbability(-11.0);

fn complete_text(model: &mut ModelSession, prompt: &str) -> String {
    model
        .complete(completion_request(prompt))
        .expect("the model completes the prompt")
        .text
}

/// Types `text` into `app` while `model` completes, and returns the reaction to the completion
/// of all of it.
fn suggest_after(model: &mut ModelSession, app: &mut App, text: &str) -> Reaction {
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
    let mut model = ModelSession::load();

    let reaction = suggest_after(&mut model, &mut App::new(), "Thank you for your");

    assert!(matches!(reaction, Reaction::Show(_)), "unexpected {reaction:?}");
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn next_word_bias_picks_only_the_first_word() {
    let mut model = ModelSession::load();
    let request = Reaction::Request(InferenceRequest::Complete {
        prompt: Prompt(format!("{SHORT_PROMPT} ")),
        max_tokens: TokenCount(16),
        word_bias: None,
        next_word_bias: NextWordBias(vec![("zebra".to_owned(), 30.0)]),
        min_log_probability: None,
        min_typed_log_probability: None,
        fixes_typos: false,
    });

    let text = model
        .complete(request)
        .expect("the model completes the prompt")
        .text;

    assert!(
        text.starts_with('z') && !text.contains(" z"),
        "unexpected {text:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn real_model_finishes_a_word_cut_inside_a_token() {
    let mut model = ModelSession::load();

    let reaction = suggest_after(&mut model, &mut App::new(), "The weather today is beauti");

    assert!(
        matches!(&reaction, Reaction::Show(text) if text.starts_with("ful")),
        "unexpected {reaction:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_that_ends_with_a_blank_line_continues_on_the_new_line() {
    let mut model = ModelSession::load();

    let text = complete_text(&mut model, BLANK_LINE_PROMPT);

    assert!(text.starts_with(char::is_alphanumeric), "unexpected {text:?}");
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_that_extends_a_cached_prompt_completes_as_without_the_cache() {
    let mut model = ModelSession::load();
    let without_cache = complete_text(&mut model, LONG_PROMPT);
    complete_text(&mut model, SHORT_PROMPT);

    let with_cache = complete_text(&mut model, LONG_PROMPT);

    assert_eq!(with_cache, without_cache);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_without_the_last_word_completes_as_without_the_cache() {
    let mut model = ModelSession::load();
    let without_cache = complete_text(&mut model, ONE_WORD_REMOVED);
    complete_text(&mut model, SHORT_PROMPT);

    let with_cache = complete_text(&mut model, ONE_WORD_REMOVED);

    assert_eq!(with_cache, without_cache);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn prompt_without_the_last_two_words_completes_as_without_the_cache() {
    let mut model = ModelSession::load();
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
    let mut model = ModelSession::load();
    let without_cache = complete_text(&mut model, LAST_WORD_CHANGED);
    complete_text(&mut model, SHORT_PROMPT);

    let with_cache = complete_text(&mut model, LAST_WORD_CHANGED);

    assert_eq!(with_cache, without_cache);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn repeated_prompt_completes_as_the_first_time() {
    let mut model = ModelSession::load();
    let first = complete_text(&mut model, LONG_PROMPT);

    let repeated = complete_text(&mut model, LONG_PROMPT);

    assert_eq!(repeated, first);
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn long_prompt_after_another_long_prompt_completes() {
    let mut model = ModelSession::load();
    complete_text(
        &mut model,
        &"The quick brown fox jumps over the lazy dog. ".repeat(120),
    );

    let completion = model.complete(Reaction::Request(InferenceRequest::Complete {
        prompt: Prompt("We hold these truths to be self-evident, that all are equal. ".repeat(110)),
        max_tokens: TokenCount(16),
        word_bias: None,
        next_word_bias: NextWordBias::default(),
        min_log_probability: None,
        min_typed_log_probability: None,
        fixes_typos: false,
    }));

    assert!(completion.is_ok(), "unexpected {completion:?}");
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn partial_completions_grow_into_the_completion() {
    let mut model = ModelSession::load();
    model.submit(completion_request(LONG_PROMPT));

    let (parts, completion) = model.next_completion_with_parts();

    let text = completion.expect("the model completes the prompt").text;
    assert!(
        !parts.is_empty() && parts.iter().all(|part| text.starts_with(part.as_str())),
        "parts {parts:?} do not grow into {text:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn newer_completion_replaces_one_that_did_not_finish() {
    let mut model = ModelSession::load();
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

/// Returns how many words `text` has: a word starts at each whitespace character after a
/// character that is not whitespace. The model runner splits only at ASCII whitespace, so the
/// count agrees with it only on text without other whitespace.
fn word_count(text: &str) -> usize {
    let starts = text
        .chars()
        .zip(text.chars().skip(1))
        .filter(|(before, character)| !before.is_whitespace() && character.is_whitespace())
        .count();
    usize::from(!text.is_empty()) + starts
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn completion_gives_a_log_probability_for_each_word() {
    let mut model = ModelSession::load();
    let prompts = [
        SHORT_PROMPT,
        LONG_PROMPT,
        LAST_WORD_CHANGED,
        "Thank you for the quick re",
    ];

    let completions = prompts.map(|prompt| {
        model
            .complete(completion_request(prompt))
            .expect("the model completes the prompt")
    });
    let checks = completions.each_ref().map(|completion| {
        let probabilities = &completion.word_log_probabilities;
        (
            probabilities.len() == word_count(&completion.text),
            probabilities.iter().all(|probability| probability.0 <= 0.0),
        )
    });

    assert_eq!(checks, [(true, true); 4], "unexpected {completions:?}");
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn word_with_a_typo_adds_no_text_below_the_typo_cut() {
    let mut model = ModelSession::load();

    let completion = model
        .complete(request_of("Thank you for the quick rpel", Typos::Hide))
        .expect("the model answers the prompt");

    assert!(
        completion.text.is_empty() && completion.typed_log_probability < TYPO_CUT,
        "unexpected {completion:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn word_with_a_typo_gets_a_fix_and_the_text_after_it() {
    let mut model = ModelSession::load();

    let completion = model
        .complete(request_of("Thank you for the quick rpel", Typos::Fix))
        .expect("the model answers the prompt");

    let fixed = completion
        .correction
        .as_ref()
        .map(|correction| correction.fixed.as_str());
    assert!(
        fixed == Some(" repl") && completion.text.starts_with('y'),
        "unexpected {completion:?} with {fixed:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn word_without_a_typo_finishes_above_the_typo_cut() {
    let mut model = ModelSession::load();

    let completion = model
        .complete(request_of("Thank you for the quick rep", Typos::Hide))
        .expect("the model completes the prompt");

    assert!(
        !completion.text.is_empty() && completion.typed_log_probability > TYPO_CUT,
        "unexpected {completion:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn completion_of_no_tokens_adds_no_text() {
    let mut model = ModelSession::load();

    let completion = model.complete(Reaction::Request(InferenceRequest::Complete {
        prompt: Prompt(SHORT_PROMPT.to_owned()),
        max_tokens: TokenCount(0),
        word_bias: None,
        next_word_bias: NextWordBias::default(),
        min_log_probability: None,
        min_typed_log_probability: None,
        fixes_typos: false,
    }));

    assert!(
        matches!(&completion, Ok(Completion { text, tokens: TokenCount(0), .. }) if text.is_empty()),
        "unexpected {completion:?}"
    );
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn first_key_in_a_prepared_field_suggests_text() {
    let mut model = ModelSession::load();
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
