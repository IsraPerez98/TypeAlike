//! The Playground pane of the settings window: a downloaded model continues text the user types.

use std::iter;

use crate::desktop::Surroundings;
use crate::hub::ModelFile;
use crate::inference::{
    Backend, Completion, InferenceError, InferenceEvent, InferenceRequest, NextWordBias, TokenCount,
};
use crate::prompt::{self, Writer};
use crate::settings::{Settings, WriterName, WriterNotes};
use crate::settings_window::models::model_name;
use crate::settings_window::view::{
    ChoiceText, ControlId, Note, NoteTone, Output, Page, PageIcon, Picker, Row, Section, SettingsInput,
    TextField,
};

/// Something the user did in the Playground pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaygroundAction {
    /// Picked this downloaded model.
    SelectModel(ModelFile),
    /// Asked the model to continue this text, as it continues the text that the user types.
    Complete(String),
}

/// State of the Playground pane: the selected model and the last completion.
///
/// ```
/// use ghosttype_core::hub::ModelFile;
/// use ghosttype_core::inference::{Backend, InferenceRequest};
/// use ghosttype_core::settings_window::playground::{PlaygroundAction, PlaygroundPane};
///
/// let file = ModelFile { repo: "owner/repo".to_owned(), name: "model.gguf".to_owned() };
/// let mut pane = PlaygroundPane::new();
///
/// let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));
///
/// assert_eq!(request, Some(InferenceRequest::Load { file, backend: Backend::Cpu }));
/// ```
#[derive(Debug, Default)]
pub struct PlaygroundPane {
    model: Option<SelectedModel>,
    completion: Option<CompletionState>,
    backend: Backend,
    writer_name: WriterName,
    writer_notes: WriterNotes,
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
    Unloaded,
}

#[derive(Debug)]
enum CompletionState {
    Running,
    Done(Completion),
    Failed(InferenceError),
}

pub(crate) const MODEL: &str = "playground-model";
pub(crate) const PROMPT: &str = "playground-prompt";
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
const UNLOADED_NOTE: Note = Note {
    text: "Suggestions are paused. GhostType loads the model again when you resume them.",
    tone: NoteTone::Plain,
};
const MAX_COMPLETION_TOKENS: TokenCount = TokenCount(32);

impl PlaygroundPane {
    /// Creates a pane with no model selected that loads each model on the default backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a pane with no model selected that loads each model on `backend`.
    #[must_use]
    pub fn with_backend(backend: Backend) -> Self {
        Self {
            backend,
            ..Self::default()
        }
    }

    /// Loads models on `backend` from now on, and loads the selected model again on it, unless
    /// it is unloaded. Returns the request to send, if any. Call it only while the pane is not
    /// busy, see [`is_busy`](Self::is_busy), because a load that runs ignores the change.
    pub fn change_backend(&mut self, backend: Backend) -> Option<InferenceRequest> {
        self.backend = backend;
        let model = self
            .model
            .as_mut()
            .filter(|model| model.state != ModelState::Unloaded)?;
        model.state = ModelState::Loading;
        self.completion = None;
        Some(InferenceRequest::Load {
            file: model.file.clone(),
            backend: self.backend,
        })
    }

    /// Puts the writer notes and the writer name of `settings` before the text of each later
    /// completion.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.writer_name = settings.writer_name.clone();
        self.writer_notes = settings.writer_notes.clone();
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

    /// Unloads the selected model to free its memory, while it stays selected, so that
    /// selecting it again loads it again. Returns the request to send, if any.
    pub fn unload_model(&mut self) -> Option<InferenceRequest> {
        let model = self.model.as_mut()?;
        model.state = ModelState::Unloaded;
        Some(InferenceRequest::Unload)
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
            InferenceEvent::Partial(_) => {}
            InferenceEvent::Completed(result) => {
                if self.is_completing() {
                    self.completion =
                        Some(result.map_or_else(CompletionState::Failed, CompletionState::Done));
                }
            }
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane. The downloaded
    /// `installed` model files give the entries of the model menu.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput, installed: &[ModelFile]) -> Option<PlaygroundAction> {
        match input {
            SettingsInput::Picked { id, index } if id.as_str() == MODEL => self
                .model_at(installed, *index)
                .map(PlaygroundAction::SelectModel),
            SettingsInput::Submitted { id, text }
                if id.as_str() == PROMPT && self.can_complete(installed) =>
            {
                Some(PlaygroundAction::Complete(text.clone()))
            }
            _ => None,
        }
    }

    /// Returns the page that the pane shows now, offering the downloaded `installed` model files.
    #[must_use]
    pub fn page(&self, installed: &[ModelFile]) -> Page {
        let model = Section::new(vec![Row::Picker(self.model_picker(
            installed,
            ControlId::new(MODEL),
            "",
        ))]);
        let prompt = Section::new(vec![Row::TextField(TextField {
            id: ControlId::new(PROMPT),
            label: PROMPT_LABEL,
            placeholder: PROMPT_PLACEHOLDER,
            submit_label: COMPLETE_LABEL,
            is_submit_enabled: self.can_complete(installed),
        })]);
        let output = self.completion.as_ref().map(|completion| Section {
            heading: Some(OUTPUT_HEADING.to_owned()),
            ..Section::new(vec![completion_row(completion)])
        });
        Page {
            title: PANE_TITLE,
            icon: PageIcon::TextCursor,
            summary: PANE_SUMMARY,
            search: None,
            sections: [model, prompt].into_iter().chain(output).collect(),
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
        let preface = prompt::preface(
            &Writer {
                notes: &self.writer_notes.0,
                name: &self.writer_name.0,
                ..Writer::default()
            },
            &Surroundings::default(),
            None,
            None,
        );
        Some(InferenceRequest::Complete {
            prompt: prompt::prompt(&preface, text),
            max_tokens: MAX_COMPLETION_TOKENS,
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            min_typed_log_probability: None,
            fixes_typos: false,
        })
    }

    fn is_completing(&self) -> bool {
        matches!(self.completion, Some(CompletionState::Running))
    }

    /// The menu that picks the model, as control `id`, with `description` below its label.
    pub(super) fn model_picker(
        &self,
        installed: &[ModelFile],
        id: ControlId,
        description: &'static str,
    ) -> Picker {
        let menu = self.model_menu(installed);
        let placeholder = placeholder_title(installed);
        Picker {
            id,
            label: MODEL_LABEL,
            description,
            choices: menu
                .entries
                .iter()
                .map(|entry| entry.map_or_else(|| placeholder.to_owned(), model_name))
                .collect(),
            choice_text: ChoiceText::Name,
            selected: menu.selected,
            is_enabled: !installed.is_empty() && !self.is_busy(),
            note: model_note(installed, self.selected_state(installed)),
        }
    }

    /// The model of the entry at `index` of [`Self::model_picker`], unless the entry is the
    /// placeholder.
    pub(super) fn model_at(&self, installed: &[ModelFile], index: usize) -> Option<ModelFile> {
        self.model_menu(installed)
            .entries
            .get(index)
            .copied()
            .flatten()
            .cloned()
    }

    /// Whether the selected model is ready among the `installed` files, and no completion runs.
    fn can_complete(&self, installed: &[ModelFile]) -> bool {
        self.selected_state(installed) == Some(ModelState::Ready) && !self.is_completing()
    }

    fn model_menu<'files>(&self, installed: &'files [ModelFile]) -> ModelMenu<'files> {
        let models = installed.iter().map(Some);
        match self.selected_index(installed) {
            Some(index) => ModelMenu {
                entries: models.collect(),
                selected: index,
            },
            None => ModelMenu {
                entries: iter::once(None).chain(models).collect(),
                selected: 0,
            },
        }
    }

    /// The index of the selected model among the `installed` files.
    fn selected_index(&self, installed: &[ModelFile]) -> Option<usize> {
        let model = self.model.as_ref()?;
        installed.iter().position(|file| *file == model.file)
    }

    /// The state of the selected model, while it is among the `installed` files.
    fn selected_state(&self, installed: &[ModelFile]) -> Option<ModelState> {
        self.selected_index(installed)?;
        self.model.as_ref().map(|model| model.state)
    }
}

/// The entries of the model menu, top to bottom: `None` is the placeholder that shows while no
/// downloaded model is selected.
struct ModelMenu<'files> {
    entries: Vec<Option<&'files ModelFile>>,
    selected: usize,
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
        Some(ModelState::Unloaded) => Some(UNLOADED_NOTE),
        None => installed.is_empty().then_some(NO_MODELS_NOTE),
    }
}

fn completion_row(completion: &CompletionState) -> Row {
    match completion {
        CompletionState::Running => Row::Note(COMPLETING_NOTE),
        CompletionState::Failed(error) => Row::Note(error_note(*error)),
        CompletionState::Done(completion) if completion.text.is_empty() => Row::Note(NOTHING_ADDED_NOTE),
        CompletionState::Done(completion) => Row::Output(Output {
            text: completion.text.clone(),
            detail: completion_detail(completion),
        }),
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
        InferenceError::Superseded => "A newer request stopped this completion. Try again.",
    };
    Note {
        text,
        tone: NoteTone::Error,
    }
}

#[cfg(test)]
mod tests;
