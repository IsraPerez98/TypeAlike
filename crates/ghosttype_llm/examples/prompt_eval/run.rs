//! The model of the eval, and the run of one case through a new app.

use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::Instant;

use anyhow::bail;
use ghosttype_core::app::{App, SettledWork};
use ghosttype_core::desktop::ScreenRead;
use ghosttype_core::engine::Reaction;
use ghosttype_core::history::{HistoryEntry, word_bias_of_counts};
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Backend, Completion, CpuCores, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest,
    LogProbability, ModelRunner, WordBias,
};
use ghosttype_core::key::Key;
use ghosttype_core::settings::{Settings, WordChoice};
use ghosttype_llm::LlamaCpp;

use crate::arguments::{Arguments, TailCut, WordBiasUse};
use crate::case::{Case, CaseField, CaseScreen};
use crate::likelihood::Continuation;
use crate::score::{
    CaseKind, Outcome, WordCounts, cumulative_log_probabilities, fixed_word, intended_word, is_match,
    next_word, words,
};

pub struct Model {
    runner: LlamaCpp,
    sink: InferenceEventSink,
    events: mpsc::Receiver<InferenceEvent>,
}

impl Model {
    pub fn load(path: &Path, backend: Backend, cpu_cores: Option<CpuCores>) -> anyhow::Result<Self> {
        let path = path.to_owned();
        let runner = LlamaCpp::new(move |_| Some(path.clone()), cpu_cores);
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });
        let mut model = Self { runner, sink, events };
        let file = ModelFile {
            repo: "local/model".to_owned(),
            name: "model.gguf".to_owned(),
        };
        model
            .runner
            .submit(InferenceRequest::Load { file, backend }, &model.sink);
        match model.events.recv()? {
            InferenceEvent::Loaded { result: Ok(()), .. } => Ok(model),
            event => bail!("the model did not load: {event:?}"),
        }
    }

    fn complete(&mut self, request: InferenceRequest) -> anyhow::Result<Result<Completion, InferenceError>> {
        self.runner.submit(request, &self.sink);
        loop {
            match self.events.recv()? {
                InferenceEvent::Partial(_) => {}
                InferenceEvent::Completed(result) => return Ok(result),
                event @ InferenceEvent::Loaded { .. } => bail!("expected a completion, got {event:?}"),
            }
        }
    }
}

/// What one case gave, and the prompt with the next word for its log probability.
pub struct CaseRun {
    pub outcome: Outcome,
    pub continuation: Option<Continuation>,
}

/// Returns `request` with the word bias of the `words` of `case`, or else with the bias that the
/// app built from the history of the case, or without a word bias when `bias_use` is off. The bias
/// gets `version`, because each case starts a new app, and the runner prepares each version one
/// time.
fn with_case_bias(
    mut request: InferenceRequest,
    case: &Case,
    choice: WordChoice,
    bias_use: WordBiasUse,
    version: u64,
) -> InferenceRequest {
    if let InferenceRequest::Complete { word_bias, .. } = &mut request {
        *word_bias = if bias_use == WordBiasUse::Off {
            None
        } else if case.words.is_empty() {
            word_bias.take().map(|bias| WordBias { version, ..bias })
        } else {
            word_bias_of_counts(case.words.clone(), choice, version)
        };
    }
    request
}

/// Returns `request` without its cut of the tail when `cut` is off.
fn with_tail_cut(mut request: InferenceRequest, cut: TailCut) -> InferenceRequest {
    if let (
        InferenceRequest::Complete {
            min_log_probability, ..
        },
        TailCut::Off,
    ) = (&mut request, cut)
    {
        *min_log_probability = None;
    }
    request
}

/// What the eval does with a prompt that the app prepares before the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Preparation {
    /// The model decodes the prompt, as when the user waits before the first key.
    Decode,
    /// The model does not decode the prompt, as when the user types at once.
    Skip,
}

/// What the app does after a request, with the prompt of the request and how likely the model
/// found each word of its completion.
struct Answer {
    reaction: Reaction,
    prompt: Option<String>,
    word_log_probabilities: Vec<LogProbability>,
    typed_log_probability: Option<LogProbability>,
    fixed_word: Option<String>,
}

/// What one key in the field of a case gave.
struct KeyRun {
    answer: Answer,
    millis: u64,
}

/// What the input settle gave.
struct Settled {
    has_read_screen: bool,
    /// What the app does to show the suggestion for a new line.
    suggestion: Option<Reaction>,
}

/// A case before its last key, in a new app each time.
struct Typing<'case> {
    case: &'case Case,
    history: &'case [HistoryEntry],
    settings: &'case Settings,
    /// The field before the last key.
    field: CaseField,
    last_key: Key,
    /// The field after the last key.
    typed_field: CaseField,
    version: u64,
    word_bias: WordBiasUse,
    tail_cut: TailCut,
}

impl Typing<'_> {
    /// Opens the field in a new app, lets the input settle with the `preparation`, presses the last
    /// key and waits for the ghost text. After Enter, the input settles again before the ghost
    /// text. Returns the app, for the keys after it.
    fn press_last_key(&self, model: &mut Model, preparation: Preparation) -> anyhow::Result<(App, KeyRun)> {
        let mut app = App::new();
        app.use_settings(self.settings);
        app.use_history(self.history.to_vec());
        let opened = self.settle(model, &mut app, &self.field, preparation)?;
        if self.case.screen.is_some() && !opened.has_read_screen {
            bail!("case {} reads no screen text", self.case.id);
        }
        let start = Instant::now();
        let reaction = app.on_key(self.last_key, &self.field);
        let mut answer = self.complete(model, &mut app, reaction)?;
        if self.last_key == Key::Enter {
            let settled = self.settle(model, &mut app, &self.typed_field, Preparation::Skip)?;
            if let Some(suggestion) = settled.suggestion {
                answer = self.complete(model, &mut app, suggestion)?;
            }
        }
        let millis = u64::try_from(start.elapsed().as_millis())?;
        Ok((app, KeyRun { answer, millis }))
    }

    /// Returns what the app does after the completion of the request of `reaction`. Returns
    /// `reaction` itself when it has no request.
    fn complete(&self, model: &mut Model, app: &mut App, reaction: Reaction) -> anyhow::Result<Answer> {
        let Reaction::Request(request) = reaction else {
            return Ok(Answer {
                reaction,
                prompt: None,
                word_log_probabilities: Vec::new(),
                typed_log_probability: None,
                fixed_word: None,
            });
        };
        let prompt = match &request {
            InferenceRequest::Complete { prompt, .. } => Some(prompt.0.clone()),
            _ => None,
        };
        let choice = self.settings.personalization.word_choice;
        let request = with_case_bias(request, self.case, choice, self.word_bias, self.version);
        let completion = model.complete(with_tail_cut(request, self.tail_cut))?;
        let word_log_probabilities = completion
            .as_ref()
            .map(|completion| completion.word_log_probabilities.clone())
            .unwrap_or_default();
        let typed_log_probability = completion
            .as_ref()
            .ok()
            .map(|completion| completion.typed_log_probability);
        let reaction = app.on_completion(completion);
        let fixed_word = match &reaction {
            Reaction::ShowFix(fix) => Some(fixed_word(&fix.word).to_owned()),
            _ => None,
        };
        Ok(Answer {
            reaction,
            prompt,
            word_log_probabilities,
            typed_log_probability,
            fixed_word,
        })
    }

    /// Lets the input settle in `app` with the focus in `field`: gives the screen of the case to
    /// each read of the screen, and does the `preparation` with each prompt that the app prepares.
    fn settle(
        &self,
        model: &mut Model,
        app: &mut App,
        field: &CaseField,
        preparation: Preparation,
    ) -> anyhow::Result<Settled> {
        let mut settled = Settled {
            has_read_screen: false,
            suggestion: None,
        };
        let mut work = app.on_input_settled(field);
        while let Some(next) = work {
            work = match next {
                SettledWork::ReadScreen { read, .. } => {
                    settled.has_read_screen = true;
                    let text = self.case.screen.as_ref().map(CaseScreen::to_screen_text);
                    app.on_screen_read(ScreenRead { id: read, text }, field)
                }
                SettledWork::Prepare(request) => {
                    let result = match preparation {
                        Preparation::Decode => model.complete(request)?,
                        Preparation::Skip => Err(InferenceError::Superseded),
                    };
                    app.on_completion(result);
                    None
                }
                SettledWork::Suggest(reaction) => {
                    settled.suggestion = Some(reaction);
                    None
                }
            };
        }
        Ok(settled)
    }
}

pub fn run_case(
    model: &mut Model,
    case: &Case,
    history: &[HistoryEntry],
    settings: &Settings,
    version: u64,
    arguments: &Arguments,
) -> anyhow::Result<CaseRun> {
    let (before_key, last_key) = case.last_key()?;
    let typing = Typing {
        case,
        history,
        settings,
        field: case.field(before_key),
        last_key,
        typed_field: case.field(case.before.clone()),
        version,
        word_bias: arguments.word_bias,
        tail_cut: arguments.tail_cut,
    };
    let (mut app, cold) = typing.press_last_key(model, Preparation::Skip)?;
    let accepted = match app.on_key(Key::Tab, &typing.typed_field) {
        Reaction::Accept { text, .. } | Reaction::Replace { text, .. } => text,
        _ => String::new(),
    };
    let (_, warm) = typing.press_last_key(model, Preparation::Decode)?;
    let suggestion = match cold.answer.reaction {
        Reaction::Show(text) => Some(text),
        _ => None,
    };
    let kind = case.kind();
    let shown = suggestion.as_deref().unwrap_or_default();
    let log_probabilities = cold
        .answer
        .word_log_probabilities
        .iter()
        .map(|log_probability| f64::from(log_probability.0))
        .collect::<Vec<_>>();

    Ok(CaseRun {
        outcome: Outcome {
            id: case.id.clone(),
            kind,
            is_match: is_match(kind, suggestion.as_deref(), &accepted, &case.next),
            cumulative_log_probabilities: cumulative_log_probabilities(
                &log_probabilities,
                words(shown).len(),
            ),
            words: WordCounts::of(shown, &case.next),
            typed_log_probability: cold
                .answer
                .typed_log_probability
                .map(|log_probability| f64::from(log_probability.0)),
            is_fix_right: cold.answer.fixed_word.as_ref().map(|word| {
                *word == intended_word(case.fixed.as_deref().unwrap_or(&case.before), &case.next)
            }),
            fixed_word: cold.answer.fixed_word,
            suggestion,
            millis: cold.millis,
            warm_millis: Some(warm.millis),
            prompt_tokens: None,
            log_probability: None,
        },
        continuation: cold
            .answer
            .prompt
            .filter(|_| kind != CaseKind::NoNewLine)
            .map(|prompt| Continuation {
                prompt,
                text: next_word(&case.next).to_owned(),
            }),
    })
}
