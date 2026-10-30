//! The Playground pane of the settings window: a downloaded model continues text the user types.

use std::iter;

use crate::app::StopReason;
use crate::desktop::Surroundings;
use crate::hub::ModelFile;
use crate::inference::{
    Backend, Completion, InferenceError, InferenceEvent, InferenceRequest, NextWordBias, SentenceEnd,
    TokenCount, TypoCheck,
};
use crate::localization::{MessageArgument, Translations};
use crate::prompt::{self, Writer};
use crate::settings::{Settings, WriterName, WriterNotes};
use crate::settings_window::view::{
    ChoiceText, ControlId, Note, NoteTone, Output, Page, PageIcon, Picker, Row, RowIcon, Section,
    SettingsInput, TextField,
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
/// use typealike_core::hub::ModelFile;
/// use typealike_core::inference::{Backend, InferenceRequest};
/// use typealike_core::settings_window::playground::{PlaygroundAction, PlaygroundPane};
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
    Unloaded(StopReason),
}

#[derive(Debug)]
enum CompletionState {
    Running,
    Done(Completion),
    Failed(InferenceError),
}

pub(crate) const MODEL: &str = "playground-model";
const PROMPT: &str = "playground-prompt";
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
            .filter(|model| !matches!(model.state, ModelState::Unloaded(_)))?;
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

    /// Whether no model is in memory or on its way there: no model is selected, or the selected
    /// model is unloaded.
    #[must_use]
    pub fn is_model_unloaded(&self) -> bool {
        self.model
            .as_ref()
            .is_none_or(|model| matches!(model.state, ModelState::Unloaded(_)))
    }

    /// Unloads the selected model to free its memory while `reason` applies. The model stays
    /// selected, so that selecting it again loads it again. Returns the request to send, if any:
    /// none when no model is selected or the model is unloaded already.
    pub fn unload_model(&mut self, reason: StopReason) -> Option<InferenceRequest> {
        let model = self.model.as_mut()?;
        let was_unloaded = matches!(model.state, ModelState::Unloaded(_));
        model.state = ModelState::Unloaded(reason);
        (!was_unloaded).then_some(InferenceRequest::Unload)
    }

    /// Selects `file` without a load, as a model that `reason` keeps unloaded, when no model is
    /// selected.
    pub fn select_unloaded_model(&mut self, file: ModelFile, reason: StopReason) {
        if self.model.is_none() {
            self.model = Some(SelectedModel {
                file,
                state: ModelState::Unloaded(reason),
            });
        }
    }

    /// Handles a user action. Returns the request that it needs, if any.
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
            InferenceEvent::Partial(_) | InferenceEvent::GpuMemory { .. } => {}
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

    /// Returns the page that the pane shows now, with the texts of `translations`, offering the
    /// downloaded `installed` model files. The row of the `recommended` model, if any, shows below
    /// the model menu.
    #[must_use]
    pub fn page(
        &self,
        installed: &[ModelFile],
        recommended: Option<Row>,
        translations: &Translations,
    ) -> Page {
        let picker = self.model_picker(installed, ControlId::new(MODEL), String::new(), translations);
        let model = Section::new(iter::once(Row::Picker(picker)).chain(recommended).collect());
        let prompt = Section::new(vec![Row::TextField(TextField {
            id: ControlId::new(PROMPT),
            icon: RowIcon::Pencil,
            label: translations.text("playground-text"),
            placeholder: translations.text("playground-text-placeholder"),
            submit_label: translations.text("playground-suggest"),
            is_submit_enabled: self.can_complete(installed),
        })]);
        let output = self.completion.as_ref().map(|completion| Section {
            heading: Some(translations.text("playground-output")),
            ..Section::new(vec![completion_row(completion, translations)])
        });
        Page {
            title: translations.text("playground-title"),
            icon: PageIcon::TextCursor,
            summary: translations.text("playground-summary"),
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
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Stop,
            line_after: String::new(),
            first_word_gate: None,
        })
    }

    fn is_completing(&self) -> bool {
        matches!(self.completion, Some(CompletionState::Running))
    }

    /// The menu that picks the model, as control `id`, with `description` below its label and the
    /// texts of `translations`.
    pub(super) fn model_picker(
        &self,
        installed: &[ModelFile],
        id: ControlId,
        description: String,
        translations: &Translations,
    ) -> Picker {
        let menu = self.model_menu(installed);
        let placeholder = translations.text(placeholder_message(installed));
        Picker {
            id,
            icon: RowIcon::Robot,
            label: translations.text("model-menu"),
            description,
            choices: menu
                .entries
                .iter()
                .map(|entry| entry.map_or_else(|| placeholder.clone(), ModelFile::model_name))
                .collect(),
            choice_text: ChoiceText::Name,
            selected: menu.selected,
            is_enabled: !installed.is_empty() && !self.is_busy(),
            note: model_note(installed, self.selected_state(installed), translations),
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

fn placeholder_message(installed: &[ModelFile]) -> &'static str {
    if installed.is_empty() {
        "model-menu-none"
    } else {
        "model-menu-select"
    }
}

fn model_note(
    installed: &[ModelFile],
    selected_state: Option<ModelState>,
    translations: &Translations,
) -> Option<Note> {
    let (message, tone) = match selected_state {
        Some(ModelState::Loading) => ("model-starting", NoteTone::Busy),
        Some(ModelState::Failed(error)) => (error_message(error), NoteTone::Error),
        Some(ModelState::Unloaded(reason)) => (unloaded_message(reason), NoteTone::Plain),
        None if installed.is_empty() => ("model-needed", NoteTone::Plain),
        Some(ModelState::Ready) | None => return None,
    };
    Some(note(message, tone, translations))
}

fn note(message: &str, tone: NoteTone, translations: &Translations) -> Note {
    Note {
        text: translations.text(message),
        tone,
    }
}

fn unloaded_message(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Paused => "model-paused",
        StopReason::EnergySaver => "model-energy-saver",
        StopReason::Game => "model-game",
        StopReason::Idle => "model-idle",
    }
}

fn completion_row(completion: &CompletionState, translations: &Translations) -> Row {
    match completion {
        CompletionState::Running => Row::Note(note("playground-writing", NoteTone::Busy, translations)),
        CompletionState::Failed(error) => {
            Row::Note(note(error_message(*error), NoteTone::Error, translations))
        }
        CompletionState::Done(completion) if completion.text.is_empty() => {
            Row::Note(note("playground-nothing-added", NoteTone::Plain, translations))
        }
        CompletionState::Done(completion) => Row::Output(Output {
            text: completion.text.clone(),
            detail: completion_detail(completion, translations),
        }),
    }
}

/// The time that the model took for `completion`, in seconds with two digits after the decimal
/// separator.
fn completion_detail(completion: &Completion, translations: &Translations) -> String {
    let hundredths = completion.duration.as_millis().saturating_add(5) / 10;
    let seconds = MessageArgument::Decimal {
        value: u32::try_from(hundredths).unwrap_or(u32::MAX),
        digits: 2,
    };
    translations.text_with("playground-time", &[("seconds", seconds)])
}

fn error_message(error: InferenceError) -> &'static str {
    match error {
        InferenceError::ModelMissing => "model-error-missing",
        InferenceError::ModelInvalid => "model-error-invalid",
        InferenceError::ModelTooLarge => "model-error-too-large",
        InferenceError::BackendUnavailable => "model-error-hardware",
        InferenceError::NoModel => "model-error-no-model",
        InferenceError::PromptTooLong => "model-error-text-too-long",
        InferenceError::Failed => "model-error-failed",
        InferenceError::Superseded => "model-error-superseded",
    }
}

#[cfg(test)]
mod tests;
