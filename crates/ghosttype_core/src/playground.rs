//! The Playground pane of the settings window: a downloaded model continues text the user types.

use std::iter;

use crate::desktop::Surroundings;
use crate::hub::ModelFile;
use crate::inference::{
    Backend, Completion, InferenceError, InferenceEvent, InferenceRequest, SystemPrompt, TokenCount,
};
use crate::models_pane::model_name;
use crate::prompt;
use crate::settings::Settings;

/// Something the user did in the Playground pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaygroundAction {
    /// Picked this downloaded model.
    SelectModel(ModelFile),
    /// Asked the model to continue this text, as it continues the text that the user types.
    Complete(String),
}

/// One entry of the model menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    /// Text shown for the entry.
    pub title: String,
    /// What picking the entry does; `None` for a placeholder entry.
    pub action: Option<PlaygroundAction>,
}

/// The menu that picks the model to try.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPicker {
    /// Text before the menu, in sentence case.
    pub label: &'static str,
    /// Menu entries, top to bottom. Never empty.
    pub choices: Vec<ModelChoice>,
    /// Index of the entry the menu shows.
    pub selected: usize,
    /// Whether the user can open the menu.
    pub is_enabled: bool,
}

/// How a note reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteTone {
    /// Plain information.
    Plain,
    /// Work is in progress.
    Busy,
    /// Something failed.
    Error,
}

/// A short line of status text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The text.
    pub text: &'static str,
    /// How the text reads.
    pub tone: NoteTone,
}

/// What shows below the prompt after the user asks for a completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaygroundOutput {
    /// Status instead of text: in progress, failed, or nothing added.
    Note(Note),
    /// The text the model added.
    Completion {
        /// The added text.
        text: String,
        /// How many tokens, and how long the model took.
        detail: String,
    },
}

/// Everything the Playground pane shows, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaygroundView {
    /// Pane title.
    pub title: &'static str,
    /// One or two sentences under the title about what the pane is for.
    pub summary: &'static str,
    /// The model menu.
    pub model: ModelPicker,
    /// Status of the selected model, or a hint when there is no model to select.
    pub model_note: Option<Note>,
    /// Text before the prompt field, in sentence case.
    pub prompt_label: &'static str,
    /// The prompt field's placeholder.
    pub prompt_placeholder: &'static str,
    /// The complete button's label, in sentence case.
    pub complete_label: &'static str,
    /// Whether the complete button and the prompt field's return key ask for a completion.
    pub is_complete_enabled: bool,
    /// Title above the output, in sentence case.
    pub output_heading: &'static str,
    /// The last completion or its status; `None` before the first request.
    pub output: Option<PlaygroundOutput>,
}

/// State of the Playground pane: the selected model and the last completion.
///
/// ```
/// use ghosttype_core::hub::ModelFile;
/// use ghosttype_core::inference::InferenceRequest;
/// use ghosttype_core::playground::{PlaygroundAction, PlaygroundPane};
///
/// let file = ModelFile { repo: "owner/repo".to_owned(), name: "model.gguf".to_owned() };
/// let mut pane = PlaygroundPane::new();
///
/// let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));
///
/// assert_eq!(request, Some(InferenceRequest::Load { file, backend: None }));
/// ```
#[derive(Debug, Default)]
pub struct PlaygroundPane {
    model: Option<SelectedModel>,
    completion: Option<CompletionState>,
    backend: Option<Backend>,
    system_prompt: SystemPrompt,
}

#[derive(Debug)]
struct SelectedModel {
    file: ModelFile,
    state: ModelState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelState {
    Loading,
    Ready,
    Failed(InferenceError),
}

#[derive(Debug)]
enum CompletionState {
    Running,
    Done(Completion),
    Failed(InferenceError),
}

const PANE_TITLE: &str = "Playground";
const PANE_SUMMARY: &str =
    "Select the model that suggests text as you type. Then type the start of a sentence to test the model.";
const MODEL_LABEL: &str = "Suggestion model";
const SELECT_MODEL_PLACEHOLDER: &str = "Select a model";
const NO_MODELS_PLACEHOLDER: &str = "No downloaded models";
const PROMPT_LABEL: &str = "Text";
const PROMPT_PLACEHOLDER: &str = "Type the start of a sentence";
const COMPLETE_LABEL: &str = "Complete text";
const OUTPUT_HEADING: &str = "Suggestion";
const NO_MODELS_NOTE: Note = Note {
    text: "Download a model in the Models pane.",
    tone: NoteTone::Plain,
};
const LOADING_NOTE: Note = Note {
    text: "Loading the model…",
    tone: NoteTone::Busy,
};
const COMPLETING_NOTE: Note = Note {
    text: "Completing the text…",
    tone: NoteTone::Busy,
};
const NOTHING_ADDED_NOTE: Note = Note {
    text: "The model did not add text.",
    tone: NoteTone::Plain,
};
const MAX_COMPLETION_TOKENS: TokenCount = TokenCount(32);

impl PlaygroundPane {
    /// Creates a pane with no model selected. The runner picks the backend of each model.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a pane with no model selected that loads each model on `backend`.
    #[must_use]
    pub fn with_backend(backend: Backend) -> Self {
        Self {
            backend: Some(backend),
            ..Self::default()
        }
    }

    /// Loads models on `backend` from now on, and loads the selected model again on it. Returns
    /// the request to send, if any. Call it only while the pane is not busy, see
    /// [`is_busy`](Self::is_busy), because a load that runs ignores the change.
    pub fn change_backend(&mut self, backend: Backend) -> Option<InferenceRequest> {
        self.backend = Some(backend);
        let model = self.model.as_mut()?;
        model.state = ModelState::Loading;
        self.completion = None;
        Some(InferenceRequest::Load {
            file: model.file.clone(),
            backend: self.backend,
        })
    }

    /// Puts the system prompt of `settings` before the text of each later completion.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.system_prompt = settings.system_prompt();
    }

    /// Whether a model loads or a completion runs now.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        let is_loading = self
            .model
            .as_ref()
            .is_some_and(|model| model.state == ModelState::Loading);
        is_loading || self.is_completing()
    }

    /// Returns the selected model once it loaded without an error.
    #[must_use]
    pub fn loaded_model(&self) -> Option<&ModelFile> {
        self.model
            .as_ref()
            .filter(|model| model.state == ModelState::Ready)
            .map(|model| &model.file)
    }

    /// Tells that the selected model loads, or why it did not load; `None` otherwise.
    #[must_use]
    pub fn load_note(&self) -> Option<Note> {
        match self.model.as_ref()?.state {
            ModelState::Loading => Some(LOADING_NOTE),
            ModelState::Failed(error) => Some(error_note(error)),
            ModelState::Ready => None,
        }
    }

    /// Handles a user action; returns the request it needs, if any.
    pub fn on_action(&mut self, action: PlaygroundAction) -> Option<InferenceRequest> {
        match action {
            PlaygroundAction::SelectModel(file) => self.select_model(file),
            PlaygroundAction::Complete(text) => self.complete(&text),
        }
    }

    /// Handles the outcome of an earlier request.
    pub fn on_event(&mut self, event: InferenceEvent) {
        match event {
            InferenceEvent::Loaded { file, result } => {
                let loading_model = self
                    .model
                    .as_mut()
                    .filter(|model| model.file == file && model.state == ModelState::Loading);
                if let Some(model) = loading_model {
                    model.state = result.map_or_else(ModelState::Failed, |()| ModelState::Ready);
                }
            }
            InferenceEvent::Completed(result) => {
                if self.is_completing() {
                    self.completion =
                        Some(result.map_or_else(CompletionState::Failed, CompletionState::Done));
                }
            }
        }
    }

    /// Returns what the pane must show now, offering the downloaded `installed` model files.
    #[must_use]
    pub fn view(&self, installed: &[ModelFile]) -> PlaygroundView {
        let selected = self.model.as_ref().and_then(|model| {
            let index = installed.iter().position(|file| *file == model.file)?;
            Some((index, model.state))
        });
        let selected_state = selected.map(|(_, state)| state);
        PlaygroundView {
            title: PANE_TITLE,
            summary: PANE_SUMMARY,
            model: self.model_picker(installed, selected.map(|(index, _)| index)),
            model_note: model_note(installed, selected_state),
            prompt_label: PROMPT_LABEL,
            prompt_placeholder: PROMPT_PLACEHOLDER,
            complete_label: COMPLETE_LABEL,
            is_complete_enabled: selected_state == Some(ModelState::Ready) && !self.is_completing(),
            output_heading: OUTPUT_HEADING,
            output: self.completion.as_ref().map(completion_output),
        }
    }

    fn select_model(&mut self, file: ModelFile) -> Option<InferenceRequest> {
        let is_loaded = self
            .model
            .as_ref()
            .is_some_and(|model| model.file == file && model.state == ModelState::Ready);
        if is_loaded || self.is_busy() {
            return None;
        }
        self.model = Some(SelectedModel {
            file: file.clone(),
            state: ModelState::Loading,
        });
        self.completion = None;
        Some(InferenceRequest::Load {
            file,
            backend: self.backend,
        })
    }

    fn complete(&mut self, text: &str) -> Option<InferenceRequest> {
        let is_ready = self
            .model
            .as_ref()
            .is_some_and(|model| model.state == ModelState::Ready);
        if !is_ready || self.is_completing() || text.trim().is_empty() {
            return None;
        }
        self.completion = Some(CompletionState::Running);
        let preface = prompt::preface(&self.system_prompt, &Surroundings::default(), None);
        Some(InferenceRequest::Complete {
            prompt: prompt::prompt(&preface, text),
            max_tokens: MAX_COMPLETION_TOKENS,
        })
    }

    fn is_completing(&self) -> bool {
        matches!(self.completion, Some(CompletionState::Running))
    }

    fn model_picker(&self, installed: &[ModelFile], selected: Option<usize>) -> ModelPicker {
        let models = installed.iter().map(|file| ModelChoice {
            title: model_name(file),
            action: Some(PlaygroundAction::SelectModel(file.clone())),
        });
        let (choices, selected) = if let Some(index) = selected {
            (models.collect(), index)
        } else {
            let placeholder = ModelChoice {
                title: placeholder_title(installed).to_owned(),
                action: None,
            };
            (iter::once(placeholder).chain(models).collect(), 0)
        };
        ModelPicker {
            label: MODEL_LABEL,
            choices,
            selected,
            is_enabled: !installed.is_empty() && !self.is_busy(),
        }
    }
}

fn placeholder_title(installed: &[ModelFile]) -> &'static str {
    if installed.is_empty() {
        NO_MODELS_PLACEHOLDER
    } else {
        SELECT_MODEL_PLACEHOLDER
    }
}

fn model_note(installed: &[ModelFile], selected_state: Option<ModelState>) -> Option<Note> {
    match selected_state {
        Some(ModelState::Loading) => Some(LOADING_NOTE),
        Some(ModelState::Failed(error)) => Some(error_note(error)),
        Some(ModelState::Ready) => None,
        None => installed.is_empty().then_some(NO_MODELS_NOTE),
    }
}

fn completion_output(completion: &CompletionState) -> PlaygroundOutput {
    match completion {
        CompletionState::Running => PlaygroundOutput::Note(COMPLETING_NOTE),
        CompletionState::Failed(error) => PlaygroundOutput::Note(error_note(*error)),
        CompletionState::Done(completion) if completion.text.is_empty() => {
            PlaygroundOutput::Note(NOTHING_ADDED_NOTE)
        }
        CompletionState::Done(completion) => PlaygroundOutput::Completion {
            text: completion.text.clone(),
            detail: completion_detail(completion),
        },
    }
}

fn completion_detail(completion: &Completion) -> String {
    let unit = if completion.tokens == TokenCount(1) {
        "token"
    } else {
        "tokens"
    };
    format!(
        "{} {unit} in {:.2} seconds",
        completion.tokens.0,
        completion.duration.as_secs_f64()
    )
}

fn error_note(error: InferenceError) -> Note {
    let text = match error {
        InferenceError::ModelMissing => "GhostType cannot find the model file. Download the model again.",
        InferenceError::ModelInvalid => {
            "GhostType cannot load this model. The file is damaged, or GhostType cannot use this type of model."
        }
        InferenceError::ModelTooLarge => {
            "The model does not fit in the GPU memory. In General, select another backend, or use a smaller model."
        }
        InferenceError::BackendUnavailable => {
            "GhostType cannot use this backend on this computer. In General, select another backend."
        }
        InferenceError::NoModel => "Select a model first.",
        InferenceError::PromptTooLong => "The text is too long for this model. Use shorter text.",
        InferenceError::Failed => "GhostType cannot complete the text. Try again.",
    };
    Note {
        text,
        tone: NoteTone::Error,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::inference::Prompt;

    const LOADING: Note = Note {
        text: "Loading the model…",
        tone: NoteTone::Busy,
    };

    fn model_file(name: &str) -> ModelFile {
        ModelFile {
            repo: "owner/repo".to_owned(),
            name: name.to_owned(),
        }
    }

    fn prompt(text: &str) -> Prompt {
        Prompt(text.to_owned())
    }

    fn complete(text: &str) -> PlaygroundAction {
        PlaygroundAction::Complete(text.to_owned())
    }

    fn choice(file: &ModelFile, title: &str) -> ModelChoice {
        ModelChoice {
            title: title.to_owned(),
            action: Some(PlaygroundAction::SelectModel(file.clone())),
        }
    }

    fn placeholder(title: &str) -> ModelChoice {
        ModelChoice {
            title: title.to_owned(),
            action: None,
        }
    }

    fn ready_pane(file: &ModelFile) -> PlaygroundPane {
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));
        pane.on_event(InferenceEvent::Loaded {
            file: file.clone(),
            result: Ok(()),
        });
        pane
    }

    fn completing_pane(file: &ModelFile) -> PlaygroundPane {
        let mut pane = ready_pane(file);
        pane.on_action(complete("Dear"));
        pane
    }

    fn completion(text: &str, tokens: u32, duration: Duration) -> Completion {
        Completion {
            text: text.to_owned(),
            tokens: TokenCount(tokens),
            duration,
        }
    }

    fn completed_view(text: &str, tokens: u32, duration: Duration) -> Option<PlaygroundOutput> {
        let file = model_file("a.gguf");
        let mut pane = completing_pane(&file);
        pane.on_event(InferenceEvent::Completed(Ok(completion(text, tokens, duration))));
        pane.view(&[file]).output
    }

    #[test]
    fn pane_without_downloads_asks_for_one() {
        let pane = PlaygroundPane::new();

        let view = pane.view(&[]);

        assert_eq!(
            view,
            PlaygroundView {
                title: "Playground",
                summary: "Select the model that suggests text as you type. Then type the start of a sentence to test the model.",
                model: ModelPicker {
                    label: "Suggestion model",
                    choices: vec![placeholder("No downloaded models")],
                    selected: 0,
                    is_enabled: false,
                },
                model_note: Some(Note {
                    text: "Download a model in the Models pane.",
                    tone: NoteTone::Plain,
                }),
                prompt_label: "Text",
                prompt_placeholder: "Type the start of a sentence",
                complete_label: "Complete text",
                is_complete_enabled: false,
                output_heading: "Suggestion",
                output: None,
            }
        );
    }

    #[test]
    fn downloaded_models_follow_a_placeholder_until_one_is_selected() {
        let first = model_file("first.gguf");
        let second = model_file("second.gguf");
        let pane = PlaygroundPane::new();

        let view = pane.view(&[first.clone(), second.clone()]);

        assert_eq!(
            (view.model, view.model_note, view.is_complete_enabled),
            (
                ModelPicker {
                    label: "Suggestion model",
                    choices: vec![
                        placeholder("Select a model"),
                        choice(&first, "first"),
                        choice(&second, "second"),
                    ],
                    selected: 0,
                    is_enabled: true,
                },
                None,
                false,
            )
        );
    }

    #[test]
    fn selecting_a_model_loads_it() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();

        let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        assert_eq!(request, Some(InferenceRequest::Load { file, backend: None }));
    }

    #[test]
    fn pane_with_a_backend_loads_models_on_it() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::with_backend(Backend::Vulkan);

        let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file,
                backend: Some(Backend::Vulkan),
            })
        );
    }

    #[test]
    fn changing_the_backend_loads_the_selected_model_again() {
        let file = model_file("a.gguf");
        let mut pane = ready_pane(&file);

        let request = pane.change_backend(Backend::Cuda);

        assert_eq!(
            (request, pane.view(std::slice::from_ref(&file)).model_note),
            (
                Some(InferenceRequest::Load {
                    file,
                    backend: Some(Backend::Cuda),
                }),
                Some(LOADING),
            )
        );
    }

    #[test]
    fn changing_the_backend_loads_a_failed_model_again() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::with_backend(Backend::Vulkan);
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));
        pane.on_event(InferenceEvent::Loaded {
            file: file.clone(),
            result: Err(InferenceError::ModelTooLarge),
        });

        let request = pane.change_backend(Backend::Cpu);

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file,
                backend: Some(Backend::Cpu),
            })
        );
    }

    #[test]
    fn changing_the_backend_without_a_model_loads_nothing() {
        let mut pane = PlaygroundPane::new();

        let request = pane.change_backend(Backend::Vulkan);

        assert_eq!(request, None);
    }

    #[test]
    fn models_selected_after_a_backend_change_load_on_it() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();
        pane.change_backend(Backend::Vulkan);

        let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file,
                backend: Some(Backend::Vulkan),
            })
        );
    }

    #[test]
    fn changing_the_backend_clears_the_output() {
        let file = model_file("a.gguf");
        let mut pane = completing_pane(&file);
        pane.on_event(InferenceEvent::Completed(Ok(completion(
            " Sir",
            1,
            Duration::ZERO,
        ))));

        pane.change_backend(Backend::Vulkan);

        assert_eq!(pane.view(&[file]).output, None);
    }

    #[test]
    fn pane_is_busy_while_a_model_loads() {
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(model_file("a.gguf")));

        assert!(pane.is_busy());
    }

    #[test]
    fn pane_is_busy_while_text_completes() {
        let pane = completing_pane(&model_file("a.gguf"));

        assert!(pane.is_busy());
    }

    #[test]
    fn pane_with_a_loaded_model_is_not_busy() {
        let pane = ready_pane(&model_file("a.gguf"));

        assert!(!pane.is_busy());
    }

    #[test]
    fn load_note_tells_how_the_selected_model_loads() {
        let file = model_file("a.gguf");
        let loading = {
            let mut pane = PlaygroundPane::new();
            pane.on_action(PlaygroundAction::SelectModel(file.clone()));
            pane
        };
        let failed = {
            let mut pane = PlaygroundPane::new();
            pane.on_action(PlaygroundAction::SelectModel(file.clone()));
            pane.on_event(InferenceEvent::Loaded {
                file: file.clone(),
                result: Err(InferenceError::BackendUnavailable),
            });
            pane
        };
        let panes = [PlaygroundPane::new(), loading, failed, ready_pane(&file)];

        let notes = panes.map(|pane| pane.load_note());

        assert_eq!(
            notes,
            [
                None,
                Some(LOADING),
                Some(Note {
                    text: "GhostType cannot use this backend on this computer. In General, select another backend.",
                    tone: NoteTone::Error,
                }),
                None,
            ]
        );
    }

    #[test]
    fn loading_model_shows_status_and_locks_the_menu() {
        let first = model_file("first.gguf");
        let second = model_file("second.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(second.clone()));

        let view = pane.view(&[first.clone(), second.clone()]);

        assert_eq!(
            (view.model, view.model_note, view.is_complete_enabled),
            (
                ModelPicker {
                    label: "Suggestion model",
                    choices: vec![choice(&first, "first"), choice(&second, "second")],
                    selected: 1,
                    is_enabled: false,
                },
                Some(LOADING),
                false,
            )
        );
    }

    #[test]
    fn loaded_model_can_complete_text() {
        let file = model_file("a.gguf");
        let pane = ready_pane(&file);

        let view = pane.view(&[file]);

        assert_eq!(
            (view.model.is_enabled, view.model_note, view.is_complete_enabled),
            (true, None, true)
        );
    }

    #[test]
    fn failed_load_tells_why() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));
        pane.on_event(InferenceEvent::Loaded {
            file: file.clone(),
            result: Err(InferenceError::ModelInvalid),
        });

        let view = pane.view(&[file]);

        assert_eq!(
            (view.model.is_enabled, view.model_note, view.is_complete_enabled),
            (
                true,
                Some(Note {
                    text: "GhostType cannot load this model. The file is damaged, or GhostType cannot use this type of model.",
                    tone: NoteTone::Error,
                }),
                false,
            )
        );
    }

    #[test]
    fn failed_model_can_be_selected_again() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));
        pane.on_event(InferenceEvent::Loaded {
            file: file.clone(),
            result: Err(InferenceError::ModelMissing),
        });

        let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        assert_eq!(request, Some(InferenceRequest::Load { file, backend: None }));
    }

    #[test]
    fn load_result_of_another_file_is_ignored() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        pane.on_event(InferenceEvent::Loaded {
            file: model_file("b.gguf"),
            result: Ok(()),
        });

        assert_eq!(pane.view(&[file]).model_note, Some(LOADING));
    }

    #[test]
    fn selecting_the_loaded_model_again_does_nothing() {
        let file = model_file("a.gguf");
        let mut pane = ready_pane(&file);

        let request = pane.on_action(PlaygroundAction::SelectModel(file));

        assert_eq!(request, None);
    }

    #[test]
    fn selecting_while_a_model_loads_does_nothing() {
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(model_file("a.gguf")));

        let request = pane.on_action(PlaygroundAction::SelectModel(model_file("b.gguf")));

        assert_eq!(request, None);
    }

    #[test]
    fn selecting_another_loaded_model_loads_it() {
        let other = model_file("b.gguf");
        let mut pane = ready_pane(&model_file("a.gguf"));

        let request = pane.on_action(PlaygroundAction::SelectModel(other.clone()));

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file: other,
                backend: None,
            })
        );
    }

    #[test]
    fn completing_sends_the_prompt_with_a_token_limit() {
        let mut pane = ready_pane(&model_file("a.gguf"));

        let request = pane.on_action(complete("Dear "));

        assert_eq!(
            request,
            Some(InferenceRequest::Complete {
                prompt: prompt("Dear "),
                max_tokens: TokenCount(32),
            })
        );
    }

    #[test]
    fn completing_puts_the_system_prompt_before_the_text() {
        let mut pane = ready_pane(&model_file("a.gguf"));
        pane.use_settings(&Settings {
            custom_system_prompt: Some(SystemPrompt("Formal.".to_owned())),
            ..Settings::default()
        });

        let request = pane.on_action(complete("Dear "));

        assert_eq!(
            request,
            Some(InferenceRequest::Complete {
                prompt: prompt("Formal.\n\nDear "),
                max_tokens: TokenCount(32),
            })
        );
    }

    #[test]
    fn blank_prompt_is_not_sent() {
        let mut pane = ready_pane(&model_file("a.gguf"));

        let request = pane.on_action(complete(" \n"));

        assert_eq!(request, None);
    }

    #[test]
    fn completing_before_a_model_is_ready_does_nothing() {
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(model_file("a.gguf")));

        let request = pane.on_action(complete("Dear"));

        assert_eq!(request, None);
    }

    #[test]
    fn running_completion_shows_status_and_locks_the_controls() {
        let file = model_file("a.gguf");
        let pane = completing_pane(&file);

        let view = pane.view(&[file]);

        assert_eq!(
            (view.model.is_enabled, view.is_complete_enabled, view.output),
            (
                false,
                false,
                Some(PlaygroundOutput::Note(Note {
                    text: "Completing the text…",
                    tone: NoteTone::Busy,
                })),
            )
        );
    }

    #[test]
    fn completing_while_a_completion_runs_does_nothing() {
        let mut pane = completing_pane(&model_file("a.gguf"));

        let request = pane.on_action(complete("Dear"));

        assert_eq!(request, None);
    }

    #[test]
    fn finished_completion_shows_text_and_timing() {
        let output = completed_view(" Sir,", 12, Duration::from_millis(420));

        assert_eq!(
            output,
            Some(PlaygroundOutput::Completion {
                text: " Sir,".to_owned(),
                detail: "12 tokens in 0.42 seconds".to_owned(),
            })
        );
    }

    #[test]
    fn one_token_completion_counts_in_singular() {
        let output = completed_view(" Sir", 1, Duration::from_millis(50));

        assert_eq!(
            output,
            Some(PlaygroundOutput::Completion {
                text: " Sir".to_owned(),
                detail: "1 token in 0.05 seconds".to_owned(),
            })
        );
    }

    #[test]
    fn empty_completion_says_that_no_text_was_added() {
        let output = completed_view("", 0, Duration::from_millis(30));

        assert_eq!(
            output,
            Some(PlaygroundOutput::Note(Note {
                text: "The model did not add text.",
                tone: NoteTone::Plain,
            }))
        );
    }

    #[test]
    fn failed_completion_tells_why_and_allows_another_try() {
        let file = model_file("a.gguf");
        let mut pane = completing_pane(&file);
        pane.on_event(InferenceEvent::Completed(Err(InferenceError::PromptTooLong)));

        let view = pane.view(&[file]);

        assert_eq!(
            (view.is_complete_enabled, view.output),
            (
                true,
                Some(PlaygroundOutput::Note(Note {
                    text: "The text is too long for this model. Use shorter text.",
                    tone: NoteTone::Error,
                })),
            )
        );
    }

    #[test]
    fn completion_without_a_request_is_ignored() {
        let file = model_file("a.gguf");
        let mut pane = ready_pane(&file);

        pane.on_event(InferenceEvent::Completed(Ok(completion(
            " Sir",
            1,
            Duration::ZERO,
        ))));

        assert_eq!(pane.view(&[file]).output, None);
    }

    #[test]
    fn selecting_another_model_clears_the_output() {
        let file = model_file("a.gguf");
        let mut pane = completing_pane(&file);
        pane.on_event(InferenceEvent::Completed(Ok(completion(
            " Sir",
            1,
            Duration::ZERO,
        ))));

        pane.on_action(PlaygroundAction::SelectModel(model_file("b.gguf")));

        assert_eq!(pane.view(&[file]).output, None);
    }

    #[test]
    fn deleted_model_is_no_longer_selected() {
        let deleted = model_file("a.gguf");
        let remaining = model_file("b.gguf");
        let pane = ready_pane(&deleted);

        let view = pane.view(std::slice::from_ref(&remaining));

        assert_eq!(
            (view.model.choices, view.model.selected, view.is_complete_enabled),
            (
                vec![placeholder("Select a model"), choice(&remaining, "b")],
                0,
                false,
            )
        );
    }

    fn load_failure_text(error: InferenceError) -> Option<&'static str> {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));
        pane.on_event(InferenceEvent::Loaded {
            file: file.clone(),
            result: Err(error),
        });
        pane.view(&[file]).model_note.map(|note| note.text)
    }

    #[test]
    fn every_error_has_its_own_message() {
        let errors = [
            InferenceError::ModelMissing,
            InferenceError::ModelInvalid,
            InferenceError::ModelTooLarge,
            InferenceError::BackendUnavailable,
            InferenceError::NoModel,
            InferenceError::PromptTooLong,
            InferenceError::Failed,
        ];

        let messages = errors.map(load_failure_text);

        assert_eq!(
            messages,
            [
                Some("GhostType cannot find the model file. Download the model again."),
                Some(
                    "GhostType cannot load this model. The file is damaged, or GhostType cannot use this type of model."
                ),
                Some(
                    "The model does not fit in the GPU memory. In General, select another backend, or use a smaller model."
                ),
                Some(
                    "GhostType cannot use this backend on this computer. In General, select another backend."
                ),
                Some("Select a model first."),
                Some("The text is too long for this model. Use shorter text."),
                Some("GhostType cannot complete the text. Try again."),
            ]
        );
    }
}
