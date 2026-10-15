//! The General pane of the settings window: the settings that apply to all of GhostType.

use std::fmt;

use crate::hub::ModelFile;
use crate::inference::{Backend, InferenceRequest, SystemPrompt};
use crate::playground::{ModelPicker, Note, PlaygroundAction, PlaygroundPane};
use crate::settings::{Autostart, DEFAULT_SYSTEM_PROMPT, Settings};

/// Something the user did in the General pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneralAction {
    /// Picked this backend to run models.
    SelectBackend(Backend),
    /// Turned on the switch that starts GhostType at login.
    EnableAutostart,
    /// Turned off the switch that starts GhostType at login.
    DisableAutostart,
    /// Changed the text of the system prompt to this text.
    EditSystemPrompt(SystemPrompt),
    /// Asked for the default system prompt again.
    ResetSystemPrompt,
}

/// One entry of the backend menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendChoice {
    /// Text shown for the entry.
    pub title: &'static str,
    /// What picking the entry does.
    pub action: GeneralAction,
}

/// The menu that picks the backend that runs models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendPicker {
    /// Text before the menu, in sentence case.
    pub label: &'static str,
    /// A sentence below the label about what the setting changes.
    pub description: &'static str,
    /// Menu entries, top to bottom. Never empty.
    pub choices: Vec<BackendChoice>,
    /// Index of the entry the menu shows.
    pub selected: usize,
    /// Whether the user can open the menu.
    pub is_enabled: bool,
}

/// The switch that starts GhostType when the user logs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutostartSwitch {
    /// Text before the switch, in sentence case.
    pub label: &'static str,
    /// A sentence below the label about what the setting changes.
    pub description: &'static str,
    /// Whether the switch shows on.
    pub is_on: bool,
}

/// The menu that picks the model that suggests text: the model menu of the Playground pane, with
/// a description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionModelPicker {
    /// The menu, as the Playground pane shows it. Its entries are Playground actions.
    pub menu: ModelPicker,
    /// A sentence below the label about what the setting changes.
    pub description: &'static str,
    /// Status of the selected model, or a hint when there is no model to select.
    pub note: Option<Note>,
}

/// The field that edits the system prompt, and the button that resets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPromptEditor {
    /// Text before the field, in sentence case.
    pub label: &'static str,
    /// A sentence below the label about what the setting changes.
    pub description: &'static str,
    /// The system prompt in the field. Change the field only when it shows other text, so that
    /// the caret and the undo history of the user stay.
    pub text: String,
    /// The reset button's label, in sentence case.
    pub reset_label: &'static str,
    /// Whether the reset button can reset, because the field does not show the default.
    pub is_reset_enabled: bool,
}

/// Everything the General pane shows, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralView {
    /// Pane title.
    pub title: &'static str,
    /// One or two sentences under the title about what the pane is for.
    pub summary: &'static str,
    /// The autostart switch, or `None` when this platform cannot start GhostType at login.
    pub autostart: Option<AutostartSwitch>,
    /// The suggestion model menu.
    pub suggestion_model: SuggestionModelPicker,
    /// The backend menu, or `None` when the model runner picks the backend itself.
    pub backend: Option<BackendPicker>,
    /// How the selected model loads on the backend, or why it did not load; `None` otherwise.
    pub backend_note: Option<Note>,
    /// The system prompt editor.
    pub system_prompt: SystemPromptEditor,
}

/// What the platform must do after a General action.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeneralOutcome {
    /// The settings to keep for the next run, when they changed.
    pub settings: Option<Settings>,
    /// The request that applies the change to the model runner, if any.
    pub request: Option<InferenceRequest>,
}

const PANE_TITLE: &str = "General";
const PANE_SUMMARY: &str = "Change how GhostType runs on this computer.";
const AUTOSTART_LABEL: &str = "Open at login";
const AUTOSTART_DESCRIPTION: &str = "Start GhostType when you log in to this computer.";
const SUGGESTION_MODEL_DESCRIPTION: &str =
    "The model that suggests text as you type. GhostType loads it again when it starts.";
const BACKEND_LABEL: &str = "Inference backend";
const BACKEND_DESCRIPTION: &str =
    "The hardware that runs the model. A GPU is faster, but the model must fit in its memory.";
const SYSTEM_PROMPT_LABEL: &str = "System prompt";
const SYSTEM_PROMPT_DESCRIPTION: &str = "The model reads this text before the text you type. Describe the \
     writer, the tone and the language. Keep it short, because a long prompt makes suggestions slower.";
const RESET_SYSTEM_PROMPT_LABEL: &str = "Reset to default";
const BACKEND_CHOICES: [(Backend, &str); 3] = [
    (Backend::Cpu, "CPU"),
    (Backend::Vulkan, "Vulkan (GPU)"),
    (Backend::Cuda, "CUDA (NVIDIA GPU)"),
];

/// State of the General pane: the settings that the user changes in it.
///
/// ```
/// use ghosttype_core::general_pane::{GeneralAction, GeneralPane};
/// use ghosttype_core::inference::Backend;
/// use ghosttype_core::playground::PlaygroundPane;
/// use ghosttype_core::settings::Settings;
///
/// let mut pane = GeneralPane::new(Settings::default());
/// let mut playground = PlaygroundPane::with_backend(Backend::Cpu);
///
/// let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Vulkan), &mut playground);
///
/// assert_eq!(outcome.settings.map(|settings| settings.backend), Some(Backend::Vulkan));
/// ```
pub struct GeneralPane {
    settings: Settings,
    has_backend_menu: bool,
    autostart: Option<Box<dyn Autostart>>,
}

impl fmt::Debug for GeneralPane {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneralPane")
            .field("settings", &self.settings)
            .field("has_backend_menu", &self.has_backend_menu)
            .finish_non_exhaustive()
    }
}

impl GeneralPane {
    /// Creates a pane that shows `settings` and the backend menu, with no autostart switch.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            has_backend_menu: true,
            autostart: None,
        }
    }

    /// Removes the backend menu, for a model runner that picks the backend itself.
    #[must_use]
    pub fn without_backend_menu(self) -> Self {
        Self {
            has_backend_menu: false,
            ..self
        }
    }

    /// Adds the autostart switch, which turns `autostart` on and off.
    #[must_use]
    pub fn with_autostart(self, autostart: Box<dyn Autostart>) -> Self {
        Self {
            autostart: Some(autostart),
            ..self
        }
    }

    /// Returns the settings that the pane shows.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Returns the request that gives the saved system prompt to the model runner, for when
    /// GhostType starts.
    #[must_use]
    pub fn system_prompt_request(&self) -> InferenceRequest {
        InferenceRequest::SetSystemPrompt(self.settings.system_prompt())
    }

    /// Selects the saved suggestion model in `playground`, so that it loads when GhostType starts.
    /// Returns the request to send, if any.
    pub fn load_suggestion_model(&self, playground: &mut PlaygroundPane) -> Option<InferenceRequest> {
        let file = self.settings.suggestion_model.clone()?;
        playground.on_action(PlaygroundAction::SelectModel(file))
    }

    /// Makes the model that `playground` loaded the suggestion model for the next runs. Returns
    /// the settings to keep, when they changed.
    pub fn keep_suggestion_model(&mut self, playground: &PlaygroundPane) -> Option<Settings> {
        let loaded = playground.loaded_model()?;
        if self.settings.suggestion_model.as_ref() == Some(loaded) {
            return None;
        }
        self.settings.suggestion_model = Some(loaded.clone());
        Some(self.settings.clone())
    }

    /// Handles a user action. The Playground pane `playground` loads its model again when the
    /// backend changes.
    pub fn on_action(&mut self, action: GeneralAction, playground: &mut PlaygroundPane) -> GeneralOutcome {
        match action {
            GeneralAction::SelectBackend(backend) => self.select_backend(backend, playground),
            GeneralAction::EnableAutostart => self.change_autostart(|autostart| autostart.enable()),
            GeneralAction::DisableAutostart => self.change_autostart(|autostart| autostart.disable()),
            GeneralAction::EditSystemPrompt(system_prompt) => self.change_system_prompt(Some(system_prompt)),
            GeneralAction::ResetSystemPrompt => self.change_system_prompt(None),
        }
    }

    /// Returns what the pane must show now, with the load state of the model that `playground`
    /// selected and the downloaded `installed` model files.
    #[must_use]
    pub fn view(&self, playground: &PlaygroundPane, installed: &[ModelFile]) -> GeneralView {
        GeneralView {
            title: PANE_TITLE,
            summary: PANE_SUMMARY,
            autostart: self.autostart.as_ref().map(|autostart| AutostartSwitch {
                label: AUTOSTART_LABEL,
                description: AUTOSTART_DESCRIPTION,
                is_on: autostart.is_enabled(),
            }),
            suggestion_model: suggestion_model_picker(playground, installed),
            backend: self.has_backend_menu.then(|| self.backend_picker(playground)),
            backend_note: playground.load_note().filter(|_| self.has_backend_menu),
            system_prompt: SystemPromptEditor {
                label: SYSTEM_PROMPT_LABEL,
                description: SYSTEM_PROMPT_DESCRIPTION,
                text: self.settings.system_prompt().0,
                reset_label: RESET_SYSTEM_PROMPT_LABEL,
                is_reset_enabled: self.settings.custom_system_prompt.is_some(),
            },
        }
    }

    fn select_backend(&mut self, backend: Backend, playground: &mut PlaygroundPane) -> GeneralOutcome {
        if backend == self.settings.backend || playground.is_busy() {
            return GeneralOutcome::default();
        }
        self.settings.backend = backend;
        GeneralOutcome {
            settings: Some(self.settings.clone()),
            request: playground.change_backend(backend),
        }
    }

    /// Keeps `custom` as the system prompt, or the default for `None` or a copy of the default.
    fn change_system_prompt(&mut self, custom: Option<SystemPrompt>) -> GeneralOutcome {
        let custom = custom.filter(|system_prompt| system_prompt.0 != DEFAULT_SYSTEM_PROMPT);
        if custom == self.settings.custom_system_prompt {
            return GeneralOutcome::default();
        }
        self.settings.custom_system_prompt = custom;
        GeneralOutcome {
            settings: Some(self.settings.clone()),
            request: Some(self.system_prompt_request()),
        }
    }

    fn change_autostart(&mut self, change: fn(&mut dyn Autostart)) -> GeneralOutcome {
        if let Some(autostart) = &mut self.autostart {
            change(autostart.as_mut());
        }
        GeneralOutcome::default()
    }

    fn backend_picker(&self, playground: &PlaygroundPane) -> BackendPicker {
        let choices: Vec<BackendChoice> = BACKEND_CHOICES
            .iter()
            .map(|&(backend, title)| BackendChoice {
                title,
                action: GeneralAction::SelectBackend(backend),
            })
            .collect();
        BackendPicker {
            label: BACKEND_LABEL,
            description: BACKEND_DESCRIPTION,
            choices,
            selected: menu_position(self.settings.backend),
            is_enabled: !playground.is_busy(),
        }
    }
}

/// The Playground model menu of `playground` over the `installed` files, with its description.
fn suggestion_model_picker(playground: &PlaygroundPane, installed: &[ModelFile]) -> SuggestionModelPicker {
    let playground_view = playground.view(installed);
    SuggestionModelPicker {
        menu: playground_view.model,
        description: SUGGESTION_MODEL_DESCRIPTION,
        note: playground_view.model_note,
    }
}

/// The index of `backend` in [`BACKEND_CHOICES`].
fn menu_position(backend: Backend) -> usize {
    match backend {
        Backend::Cpu => 0,
        Backend::Vulkan => 1,
        Backend::Cuda => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{InferenceError, InferenceEvent};
    use crate::playground::{ModelChoice, NoteTone};

    #[derive(Debug)]
    struct FakeAutostart {
        is_enabled: bool,
    }

    impl Autostart for FakeAutostart {
        fn is_enabled(&self) -> bool {
            self.is_enabled
        }

        fn enable(&mut self) {
            self.is_enabled = true;
        }

        fn disable(&mut self) {
            self.is_enabled = false;
        }
    }

    #[derive(Debug)]
    struct RefusingAutostart {
        is_enabled: bool,
    }

    impl Autostart for RefusingAutostart {
        fn is_enabled(&self) -> bool {
            self.is_enabled
        }

        fn enable(&mut self) {}

        fn disable(&mut self) {}
    }

    fn model_file() -> ModelFile {
        named_file("model.gguf")
    }

    fn named_file(name: &str) -> ModelFile {
        ModelFile {
            repo: "owner/repo".to_owned(),
            name: name.to_owned(),
        }
    }

    fn settings(backend: Backend) -> Settings {
        Settings {
            backend,
            ..Settings::default()
        }
    }

    fn suggestion_settings(suggestion_model: Option<ModelFile>) -> Settings {
        Settings {
            suggestion_model,
            ..Settings::default()
        }
    }

    fn custom_settings(system_prompt: &str) -> Settings {
        Settings {
            custom_system_prompt: Some(SystemPrompt(system_prompt.to_owned())),
            ..Settings::default()
        }
    }

    fn system_prompt_outcome(settings: Settings) -> GeneralOutcome {
        GeneralOutcome {
            request: Some(InferenceRequest::SetSystemPrompt(settings.system_prompt())),
            settings: Some(settings),
        }
    }

    fn edit_system_prompt(pane: &mut GeneralPane, text: &str) -> GeneralOutcome {
        pane.on_action(
            GeneralAction::EditSystemPrompt(SystemPrompt(text.to_owned())),
            &mut PlaygroundPane::new(),
        )
    }

    fn choice(title: &'static str, backend: Backend) -> BackendChoice {
        BackendChoice {
            title,
            action: GeneralAction::SelectBackend(backend),
        }
    }

    fn model_choice(title: &str, file: &ModelFile) -> ModelChoice {
        ModelChoice {
            title: title.to_owned(),
            action: Some(PlaygroundAction::SelectModel(file.clone())),
        }
    }

    fn autostart_pane(is_enabled: bool) -> GeneralPane {
        GeneralPane::new(Settings::default()).with_autostart(Box::new(FakeAutostart { is_enabled }))
    }

    fn loading_playground(backend: Backend) -> PlaygroundPane {
        let mut playground = PlaygroundPane::with_backend(backend);
        playground.on_action(PlaygroundAction::SelectModel(model_file()));
        playground
    }

    fn loaded_playground(backend: Backend, result: Result<(), InferenceError>) -> PlaygroundPane {
        let mut playground = loading_playground(backend);
        playground.on_event(InferenceEvent::Loaded {
            file: model_file(),
            result,
        });
        playground
    }

    #[test]
    fn pane_shows_the_saved_backend_among_the_choices() {
        let pane = GeneralPane::new(settings(Backend::Vulkan));

        let view = pane.view(&PlaygroundPane::with_backend(Backend::Vulkan), &[]);

        assert_eq!(
            view,
            GeneralView {
                title: "General",
                summary: "Change how GhostType runs on this computer.",
                autostart: None,
                suggestion_model: SuggestionModelPicker {
                    menu: ModelPicker {
                        label: "Suggestion model",
                        choices: vec![ModelChoice {
                            title: "No downloaded models".to_owned(),
                            action: None,
                        }],
                        selected: 0,
                        is_enabled: false,
                    },
                    description: "The model that suggests text as you type. GhostType loads it again when it starts.",
                    note: Some(Note {
                        text: "Download a model in the Models pane.",
                        tone: NoteTone::Plain,
                    }),
                },
                backend: Some(BackendPicker {
                    label: "Inference backend",
                    description: "The hardware that runs the model. A GPU is faster, but the model must fit in its memory.",
                    choices: vec![
                        choice("CPU", Backend::Cpu),
                        choice("Vulkan (GPU)", Backend::Vulkan),
                        choice("CUDA (NVIDIA GPU)", Backend::Cuda),
                    ],
                    selected: 1,
                    is_enabled: true,
                }),
                backend_note: None,
                system_prompt: SystemPromptEditor {
                    label: "System prompt",
                    description: "The model reads this text before the text you type. Describe the writer, the tone and the language. Keep it short, because a long prompt makes suggestions slower.",
                    text: DEFAULT_SYSTEM_PROMPT.to_owned(),
                    reset_label: "Reset to default",
                    is_reset_enabled: false,
                },
            }
        );
    }

    #[test]
    fn menu_shows_each_saved_backend_as_its_own_choice() {
        let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda];

        let shown = backends.map(|backend| {
            let picker = GeneralPane::new(settings(backend))
                .view(&PlaygroundPane::with_backend(backend), &[])
                .backend
                .expect("the pane shows the backend menu");
            picker.choices[picker.selected].action.clone()
        });

        assert_eq!(shown, backends.map(GeneralAction::SelectBackend));
    }

    #[test]
    fn pane_without_backend_menu_hides_the_menu_and_its_note() {
        let pane = GeneralPane::new(settings(Backend::Cpu)).without_backend_menu();

        let view = pane.view(&loading_playground(Backend::Cpu), &[]);

        assert_eq!((view.backend, view.backend_note), (None, None));
    }

    #[test]
    fn selecting_a_backend_keeps_it() {
        let mut pane = GeneralPane::new(settings(Backend::Cpu));
        let mut playground = PlaygroundPane::with_backend(Backend::Cpu);

        let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Cuda), &mut playground);

        assert_eq!(
            (outcome, pane.settings()),
            (
                GeneralOutcome {
                    settings: Some(settings(Backend::Cuda)),
                    request: None,
                },
                &settings(Backend::Cuda),
            )
        );
    }

    #[test]
    fn selecting_a_backend_loads_the_selected_model_on_it() {
        let mut pane = GeneralPane::new(settings(Backend::Cpu));
        let mut playground = loaded_playground(Backend::Cpu, Ok(()));

        let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Vulkan), &mut playground);

        assert_eq!(
            outcome.request,
            Some(InferenceRequest::Load {
                file: model_file(),
                backend: Some(Backend::Vulkan),
            })
        );
    }

    #[test]
    fn selecting_the_current_backend_does_nothing() {
        let mut pane = GeneralPane::new(settings(Backend::Vulkan));
        let mut playground = loaded_playground(Backend::Vulkan, Ok(()));

        let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Vulkan), &mut playground);

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn selecting_a_backend_while_the_model_loads_does_nothing() {
        let mut pane = GeneralPane::new(settings(Backend::Cpu));
        let mut playground = loading_playground(Backend::Cpu);

        let outcome = pane.on_action(GeneralAction::SelectBackend(Backend::Vulkan), &mut playground);

        assert_eq!(
            (outcome, pane.settings()),
            (GeneralOutcome::default(), &settings(Backend::Cpu))
        );
    }

    #[test]
    fn loading_model_locks_the_menu_and_shows_status() {
        let pane = GeneralPane::new(settings(Backend::Cuda));

        let view = pane.view(&loading_playground(Backend::Cuda), &[]);

        assert_eq!(
            (view.backend.map(|backend| backend.is_enabled), view.backend_note),
            (
                Some(false),
                Some(Note {
                    text: "Loading the model…",
                    tone: NoteTone::Busy,
                }),
            )
        );
    }

    #[test]
    fn model_too_large_for_the_gpu_tells_what_to_do() {
        let pane = GeneralPane::new(settings(Backend::Vulkan));

        let view = pane.view(
            &loaded_playground(Backend::Vulkan, Err(InferenceError::ModelTooLarge)),
            &[],
        );

        assert_eq!(
            (view.backend.map(|backend| backend.is_enabled), view.backend_note),
            (
                Some(true),
                Some(Note {
                    text: "The model does not fit in the GPU memory. In General, select another backend, or use a smaller model.",
                    tone: NoteTone::Error,
                }),
            )
        );
    }

    #[test]
    fn autostart_switch_shows_the_state_that_the_os_keeps() {
        let pane = autostart_pane(true);

        let view = pane.view(&PlaygroundPane::new(), &[]);

        assert_eq!(
            view.autostart,
            Some(AutostartSwitch {
                label: "Open at login",
                description: "Start GhostType when you log in to this computer.",
                is_on: true,
            })
        );
    }

    #[test]
    fn enabling_autostart_turns_the_switch_on() {
        let mut pane = autostart_pane(false);
        let mut playground = PlaygroundPane::new();

        let outcome = pane.on_action(GeneralAction::EnableAutostart, &mut playground);

        let is_on = pane.view(&playground, &[]).autostart.map(|switch| switch.is_on);
        assert_eq!((outcome, is_on), (GeneralOutcome::default(), Some(true)));
    }

    #[test]
    fn disabling_autostart_turns_the_switch_off() {
        let mut pane = autostart_pane(true);
        let mut playground = PlaygroundPane::new();

        pane.on_action(GeneralAction::DisableAutostart, &mut playground);

        let is_on = pane.view(&playground, &[]).autostart.map(|switch| switch.is_on);
        assert_eq!(is_on, Some(false));
    }

    #[test]
    fn switch_shows_the_kept_state_when_the_os_refuses_a_change() {
        let mut pane = GeneralPane::new(Settings::default())
            .with_autostart(Box::new(RefusingAutostart { is_enabled: false }));
        let mut playground = PlaygroundPane::new();

        pane.on_action(GeneralAction::EnableAutostart, &mut playground);

        let is_on = pane.view(&playground, &[]).autostart.map(|switch| switch.is_on);
        assert_eq!(is_on, Some(false));
    }

    #[test]
    fn suggestion_model_menu_shows_the_model_that_the_playground_selected() {
        let other = named_file("other.gguf");
        let pane = GeneralPane::new(suggestion_settings(None));

        let picker = pane
            .view(
                &loaded_playground(Backend::Cpu, Ok(())),
                &[other.clone(), model_file()],
            )
            .suggestion_model;

        assert_eq!(
            (
                picker.menu.choices,
                picker.menu.selected,
                picker.menu.is_enabled,
                picker.note
            ),
            (
                vec![
                    model_choice("other", &other),
                    model_choice("model", &model_file())
                ],
                1,
                true,
                None,
            )
        );
    }

    #[test]
    fn suggestion_model_menu_locks_and_shows_status_while_the_model_loads() {
        let pane = GeneralPane::new(suggestion_settings(None));

        let picker = pane
            .view(&loading_playground(Backend::Cpu), &[model_file()])
            .suggestion_model;

        assert_eq!(
            (picker.menu.is_enabled, picker.note),
            (
                false,
                Some(Note {
                    text: "Loading the model…",
                    tone: NoteTone::Busy,
                }),
            )
        );
    }

    #[test]
    fn loaded_model_becomes_the_suggestion_model() {
        let mut pane = GeneralPane::new(suggestion_settings(None));

        let settings = pane.keep_suggestion_model(&loaded_playground(Backend::Cpu, Ok(())));

        assert_eq!(
            (settings, pane.settings()),
            (
                Some(suggestion_settings(Some(model_file()))),
                &suggestion_settings(Some(model_file())),
            )
        );
    }

    #[test]
    fn loading_the_saved_suggestion_model_again_keeps_nothing() {
        let mut pane = GeneralPane::new(suggestion_settings(Some(model_file())));

        let settings = pane.keep_suggestion_model(&loaded_playground(Backend::Cpu, Ok(())));

        assert_eq!(settings, None);
    }

    #[test]
    fn model_that_did_not_load_is_not_kept() {
        let playgrounds = [
            loading_playground(Backend::Cpu),
            loaded_playground(Backend::Cpu, Err(InferenceError::ModelInvalid)),
        ];

        let kept = playgrounds
            .map(|playground| GeneralPane::new(suggestion_settings(None)).keep_suggestion_model(&playground));

        assert_eq!(kept, [None, None]);
    }

    #[test]
    fn suggestion_model_loads_in_the_playground() {
        let pane = GeneralPane::new(suggestion_settings(Some(model_file())));
        let mut playground = PlaygroundPane::new();

        let request = pane.load_suggestion_model(&mut playground);

        assert_eq!(
            (request, playground.load_note()),
            (
                Some(InferenceRequest::Load {
                    file: model_file(),
                    backend: None,
                }),
                Some(Note {
                    text: "Loading the model…",
                    tone: NoteTone::Busy,
                }),
            )
        );
    }

    #[test]
    fn no_suggestion_model_loads_nothing() {
        let pane = GeneralPane::new(suggestion_settings(None));
        let mut playground = PlaygroundPane::new();

        let request = pane.load_suggestion_model(&mut playground);

        assert_eq!((request, playground.is_busy()), (None, false));
    }

    #[test]
    fn custom_system_prompt_shows_with_reset_enabled() {
        let pane = GeneralPane::new(custom_settings("Formal Spanish."));

        let editor = pane.view(&PlaygroundPane::new(), &[]).system_prompt;

        assert_eq!(
            (editor.text.as_str(), editor.is_reset_enabled),
            ("Formal Spanish.", true)
        );
    }

    #[test]
    fn editing_the_system_prompt_keeps_it_and_gives_it_to_the_runner() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = edit_system_prompt(&mut pane, "Formal Spanish.");

        assert_eq!(
            (outcome, pane.settings()),
            (
                system_prompt_outcome(custom_settings("Formal Spanish.")),
                &custom_settings("Formal Spanish."),
            )
        );
    }

    #[test]
    fn emptied_system_prompt_is_kept_as_empty() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = edit_system_prompt(&mut pane, "");

        assert_eq!(outcome, system_prompt_outcome(custom_settings("")));
    }

    #[test]
    fn editing_the_system_prompt_to_the_same_text_does_nothing() {
        let mut pane = GeneralPane::new(custom_settings("Formal Spanish."));

        let outcome = edit_system_prompt(&mut pane, "Formal Spanish.");

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn system_prompt_edited_back_to_the_default_is_the_default() {
        let mut pane = GeneralPane::new(custom_settings("Formal Spanish."));

        let outcome = edit_system_prompt(&mut pane, DEFAULT_SYSTEM_PROMPT);

        let is_reset_enabled = pane
            .view(&PlaygroundPane::new(), &[])
            .system_prompt
            .is_reset_enabled;
        assert_eq!(
            (outcome, is_reset_enabled),
            (system_prompt_outcome(Settings::default()), false)
        );
    }

    #[test]
    fn resetting_the_system_prompt_gives_the_default_to_the_runner() {
        let mut pane = GeneralPane::new(custom_settings("Formal Spanish."));

        let outcome = pane.on_action(GeneralAction::ResetSystemPrompt, &mut PlaygroundPane::new());

        assert_eq!(
            (outcome, pane.settings()),
            (system_prompt_outcome(Settings::default()), &Settings::default())
        );
    }

    #[test]
    fn resetting_the_default_system_prompt_does_nothing() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = pane.on_action(GeneralAction::ResetSystemPrompt, &mut PlaygroundPane::new());

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn saved_system_prompt_goes_to_the_runner_at_start() {
        let pane = GeneralPane::new(custom_settings("Formal Spanish."));

        let request = pane.system_prompt_request();

        assert_eq!(
            request,
            InferenceRequest::SetSystemPrompt(SystemPrompt("Formal Spanish.".to_owned()))
        );
    }
}
