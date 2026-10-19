//! The Playground pane of the settings window: a downloaded model continues text the user types.

use std::iter;

use crate::desktop::Surroundings;
use crate::hub::ModelFile;
use crate::inference::{Backend, Completion, InferenceError, InferenceEvent, InferenceRequest, TokenCount};
use crate::models_pane::model_name;
use crate::prompt::{self, Writer};
use crate::settings::{Settings, WriterNotes};
use crate::settings_view::{
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
/// use ghosttype_core::playground::{PlaygroundAction, PlaygroundPane};
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

    /// Puts the writer notes of `settings` before the text of each later completion.
    pub fn use_settings(&mut self, settings: &Settings) {
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
        })
    }

    fn is_completing(&self) -> bool {
        matches!(self.completion, Some(CompletionState::Running))
    }

    /// The menu that picks the model, as control `id`, with `description` below its label.
    pub(crate) fn model_picker(
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
    pub(crate) fn model_at(&self, installed: &[ModelFile], index: usize) -> Option<ModelFile> {
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
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::inference::Prompt;

    const LOADING: Note = Note {
        text: "Loading the model…",
        tone: NoteTone::Busy,
    };

    const UNLOADED: Note = Note {
        text: "Suggestions are paused. GhostType loads the model again when you resume them.",
        tone: NoteTone::Plain,
    };

    fn model_file(name: &str) -> ModelFile {
        ModelFile {
            repo: "owner/repo".to_owned(),
            name: name.to_owned(),
        }
    }

    fn prompt(text: &str) -> Prompt {
        prompt::framed_prompt(&[], text)
    }

    fn complete(text: &str) -> PlaygroundAction {
        PlaygroundAction::Complete(text.to_owned())
    }

    fn titles(titles: &[&str]) -> Vec<String> {
        titles.iter().map(|title| (*title).to_owned()).collect()
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

    fn model_menu(page: &Page) -> Picker {
        match page.row(MODEL) {
            Some(Row::Picker(picker)) => picker.clone(),
            other => panic!("the page shows no model menu: {other:?}"),
        }
    }

    fn can_complete(page: &Page) -> bool {
        match page.row(PROMPT) {
            Some(Row::TextField(text_field)) => text_field.is_submit_enabled,
            other => panic!("the page shows no prompt field: {other:?}"),
        }
    }

    /// The section below the prompt field, which shows the last completion.
    fn output(page: &Page) -> Option<Section> {
        page.sections.get(2).cloned()
    }

    fn output_section(row: Row) -> Section {
        Section {
            heading: Some("Suggestion".to_owned()),
            ..Section::new(vec![row])
        }
    }

    fn completed_output(text: &str, tokens: u32, duration: Duration) -> Option<Section> {
        let file = model_file("a.gguf");
        let mut pane = completing_pane(&file);
        pane.on_event(InferenceEvent::Completed(Ok(completion(text, tokens, duration))));
        output(&pane.page(&[file]))
    }

    fn submitted(text: &str) -> SettingsInput {
        SettingsInput::Submitted {
            id: ControlId::new(PROMPT),
            text: text.to_owned(),
        }
    }

    fn picked(index: usize) -> SettingsInput {
        SettingsInput::Picked {
            id: ControlId::new(MODEL),
            index,
        }
    }

    #[test]
    fn pane_without_downloads_asks_for_one() {
        let pane = PlaygroundPane::new();

        let shown = pane.page(&[]);

        assert_eq!(
            shown,
            Page {
                title: "Playground",
                icon: PageIcon::TextCursor,
                summary: "Select the model that suggests text as you type. Then type the start of a sentence to test the model.",
                search: None,
                sections: vec![
                    Section::new(vec![Row::Picker(Picker {
                        id: ControlId::new(MODEL),
                        label: "Suggestion model",
                        description: "",
                        choices: titles(&["No downloaded models"]),
                        choice_text: ChoiceText::Name,
                        selected: 0,
                        is_enabled: false,
                        note: Some(Note {
                            text: "Download a model in the Models pane.",
                            tone: NoteTone::Plain,
                        }),
                    })]),
                    Section::new(vec![Row::TextField(TextField {
                        id: ControlId::new(PROMPT),
                        label: "Text",
                        placeholder: "Type the start of a sentence",
                        submit_label: "Complete text",
                        is_submit_enabled: false,
                    })]),
                ],
            }
        );
    }

    #[test]
    fn downloaded_models_follow_a_placeholder_until_one_is_selected() {
        let pane = PlaygroundPane::new();

        let shown = pane.page(&[model_file("first.gguf"), model_file("second.gguf")]);

        let menu = model_menu(&shown);
        assert_eq!(
            (
                menu.choices,
                menu.selected,
                menu.is_enabled,
                menu.note,
                can_complete(&shown)
            ),
            (
                titles(&["Select a model", "first", "second"]),
                0,
                true,
                None,
                false
            )
        );
    }

    #[test]
    fn picking_a_model_selects_it() {
        let file = model_file("a.gguf");
        let pane = PlaygroundPane::new();

        let action = pane.action_for(&picked(1), std::slice::from_ref(&file));

        assert_eq!(action, Some(PlaygroundAction::SelectModel(file)));
    }

    #[test]
    fn picking_the_placeholder_model_does_nothing() {
        let pane = PlaygroundPane::new();

        let action = pane.action_for(&picked(0), &[model_file("a.gguf")]);

        assert_eq!(action, None);
    }

    #[test]
    fn submitting_the_prompt_completes_its_text() {
        let file = model_file("a.gguf");
        let pane = ready_pane(&file);

        let action = pane.action_for(&submitted("Dear"), &[file]);

        assert_eq!(action, Some(complete("Dear")));
    }

    #[test]
    fn submitting_the_prompt_before_the_model_is_ready_does_nothing() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        let action = pane.action_for(&submitted("Dear"), &[file]);

        assert_eq!(action, None);
    }

    #[test]
    fn input_for_a_control_of_another_pane_is_not_for_this_pane() {
        let pane = PlaygroundPane::new();
        let input = SettingsInput::Picked {
            id: ControlId::new("suggestion-length"),
            index: 0,
        };

        let action = pane.action_for(&input, &[model_file("a.gguf")]);

        assert_eq!(action, None);
    }

    #[test]
    fn selecting_a_model_loads_it() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();

        let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file,
                backend: Backend::Cpu
            })
        );
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
                backend: Backend::Vulkan,
            })
        );
    }

    #[test]
    fn changing_the_backend_loads_the_selected_model_again() {
        let file = model_file("a.gguf");
        let mut pane = ready_pane(&file);

        let request = pane.change_backend(Backend::Cuda);

        assert_eq!(
            (request, model_menu(&pane.page(std::slice::from_ref(&file))).note),
            (
                Some(InferenceRequest::Load {
                    file,
                    backend: Backend::Cuda,
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
                backend: Backend::Cpu,
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
                backend: Backend::Vulkan,
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

        assert_eq!(output(&pane.page(&[file])), None);
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
    fn loading_model_shows_status_and_locks_the_menu() {
        let second = model_file("second.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(second.clone()));

        let shown = pane.page(&[model_file("first.gguf"), second]);

        let menu = model_menu(&shown);
        assert_eq!(
            (
                menu.choices,
                menu.selected,
                menu.is_enabled,
                menu.note,
                can_complete(&shown)
            ),
            (titles(&["first", "second"]), 1, false, Some(LOADING), false)
        );
    }

    #[test]
    fn loaded_model_can_complete_text() {
        let file = model_file("a.gguf");
        let pane = ready_pane(&file);

        let shown = pane.page(&[file]);

        let menu = model_menu(&shown);
        assert_eq!(
            (menu.is_enabled, menu.note, can_complete(&shown)),
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

        let shown = pane.page(&[file]);

        let menu = model_menu(&shown);
        assert_eq!(
            (menu.is_enabled, menu.note, can_complete(&shown)),
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

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file,
                backend: Backend::Cpu
            })
        );
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

        assert_eq!(model_menu(&pane.page(&[file])).note, Some(LOADING));
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
                backend: Backend::Cpu,
            })
        );
    }

    #[test]
    fn unloading_the_loaded_model_asks_the_runner_to_unload_it() {
        let mut pane = ready_pane(&model_file("a.gguf"));

        let request = pane.unload_model();

        assert_eq!(request, Some(InferenceRequest::Unload));
    }

    #[test]
    fn unloading_without_a_model_does_nothing() {
        let mut pane = PlaygroundPane::new();

        let request = pane.unload_model();

        assert_eq!(request, None);
    }

    #[test]
    fn unloaded_model_stays_selected_and_tells_that_suggestions_are_paused() {
        let file = model_file("a.gguf");
        let mut pane = ready_pane(&file);

        pane.unload_model();

        let shown = pane.page(std::slice::from_ref(&file));
        let menu = model_menu(&shown);
        assert_eq!(
            (
                pane.loaded_model(),
                menu.selected,
                menu.note,
                can_complete(&shown)
            ),
            (None, 0, Some(UNLOADED), false)
        );
    }

    #[test]
    fn selecting_the_unloaded_model_loads_it_again() {
        let file = model_file("a.gguf");
        let mut pane = ready_pane(&file);
        pane.unload_model();

        let request = pane.on_action(PlaygroundAction::SelectModel(file.clone()));

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file,
                backend: Backend::Cpu
            })
        );
    }

    #[test]
    fn load_that_ends_after_an_unload_is_ignored() {
        let file = model_file("a.gguf");
        let mut pane = PlaygroundPane::new();
        pane.on_action(PlaygroundAction::SelectModel(file.clone()));
        pane.unload_model();

        pane.on_event(InferenceEvent::Loaded { file, result: Ok(()) });

        assert_eq!(pane.loaded_model(), None);
    }

    #[test]
    fn changing_the_backend_does_not_load_an_unloaded_model() {
        let mut pane = ready_pane(&model_file("a.gguf"));
        pane.unload_model();

        let request = pane.change_backend(Backend::Vulkan);

        assert_eq!(request, None);
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
                word_bias: None,
            })
        );
    }

    #[test]
    fn completing_puts_the_writer_notes_before_the_text() {
        let mut pane = ready_pane(&model_file("a.gguf"));
        pane.use_settings(&Settings {
            writer_notes: WriterNotes("I am a nurse.".to_owned()),
            ..Settings::default()
        });

        let request = pane.on_action(complete("Dear "));

        assert_eq!(
            request,
            Some(InferenceRequest::Complete {
                prompt: prompt::framed_prompt(&["Notes about me: “I am a nurse.”"], "Dear "),
                max_tokens: TokenCount(32),
                word_bias: None,
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

        let shown = pane.page(&[file]);

        assert_eq!(
            (
                model_menu(&shown).is_enabled,
                can_complete(&shown),
                output(&shown)
            ),
            (
                false,
                false,
                Some(output_section(Row::Note(Note {
                    text: "Completing the text…",
                    tone: NoteTone::Busy,
                }))),
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
        let shown = completed_output(" Sir,", 12, Duration::from_millis(420));

        assert_eq!(
            shown,
            Some(output_section(Row::Output(Output {
                text: " Sir,".to_owned(),
                detail: "12 tokens in 0.42 seconds".to_owned(),
            })))
        );
    }

    #[test]
    fn one_token_completion_counts_in_singular() {
        let shown = completed_output(" Sir", 1, Duration::from_millis(50));

        assert_eq!(
            shown,
            Some(output_section(Row::Output(Output {
                text: " Sir".to_owned(),
                detail: "1 token in 0.05 seconds".to_owned(),
            })))
        );
    }

    #[test]
    fn empty_completion_says_that_no_text_was_added() {
        let shown = completed_output("", 0, Duration::from_millis(30));

        assert_eq!(
            shown,
            Some(output_section(Row::Note(Note {
                text: "The model did not add text.",
                tone: NoteTone::Plain,
            })))
        );
    }

    #[test]
    fn failed_completion_tells_why_and_allows_another_try() {
        let file = model_file("a.gguf");
        let mut pane = completing_pane(&file);
        pane.on_event(InferenceEvent::Completed(Err(InferenceError::PromptTooLong)));

        let shown = pane.page(&[file]);

        assert_eq!(
            (can_complete(&shown), output(&shown)),
            (
                true,
                Some(output_section(Row::Note(Note {
                    text: "The text is too long for this model. Use shorter text.",
                    tone: NoteTone::Error,
                }))),
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

        assert_eq!(output(&pane.page(&[file])), None);
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

        assert_eq!(output(&pane.page(&[file])), None);
    }

    #[test]
    fn deleted_model_is_no_longer_selected() {
        let pane = ready_pane(&model_file("a.gguf"));

        let shown = pane.page(&[model_file("b.gguf")]);

        let menu = model_menu(&shown);
        assert_eq!(
            (menu.choices, menu.selected, can_complete(&shown)),
            (titles(&["Select a model", "b"]), 0, false)
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
        model_menu(&pane.page(&[file])).note.map(|note| note.text)
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
