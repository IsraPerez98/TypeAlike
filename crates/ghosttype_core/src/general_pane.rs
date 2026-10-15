//! The General pane of the settings window: the settings that apply to all of GhostType.

use crate::inference::{Backend, InferenceRequest};
use crate::playground::{Note, PlaygroundPane};
use crate::settings::Settings;

/// Something the user did in the General pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralAction {
    /// Picked this backend to run models.
    SelectBackend(Backend),
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

/// Everything the General pane shows, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralView {
    /// Pane title.
    pub title: &'static str,
    /// One or two sentences under the title about what the pane is for.
    pub summary: &'static str,
    /// The backend menu.
    pub backend: BackendPicker,
    /// How the selected model loads on the backend, or why it did not load; `None` otherwise.
    pub backend_note: Option<Note>,
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
const BACKEND_LABEL: &str = "Inference backend";
const BACKEND_DESCRIPTION: &str =
    "The hardware that runs the model. A GPU is faster, but the model must fit in its memory.";
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
/// assert_eq!(outcome.settings, Some(Settings { backend: Backend::Vulkan }));
/// ```
#[derive(Debug, Default)]
pub struct GeneralPane {
    settings: Settings,
}

impl GeneralPane {
    /// Creates a pane that shows `settings`.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    /// Returns the settings that the pane shows.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// Handles a user action. The Playground pane `playground` loads its model again when the
    /// backend changes.
    pub fn on_action(&mut self, action: GeneralAction, playground: &mut PlaygroundPane) -> GeneralOutcome {
        let GeneralAction::SelectBackend(backend) = action;
        if backend == self.settings.backend || playground.is_busy() {
            return GeneralOutcome::default();
        }
        self.settings.backend = backend;
        GeneralOutcome {
            settings: Some(self.settings),
            request: playground.change_backend(backend),
        }
    }

    /// Returns what the pane must show now, with the load state of the model that `playground`
    /// selected.
    #[must_use]
    pub fn view(&self, playground: &PlaygroundPane) -> GeneralView {
        let choices: Vec<BackendChoice> = BACKEND_CHOICES
            .iter()
            .map(|&(backend, title)| BackendChoice {
                title,
                action: GeneralAction::SelectBackend(backend),
            })
            .collect();
        GeneralView {
            title: PANE_TITLE,
            summary: PANE_SUMMARY,
            backend: BackendPicker {
                label: BACKEND_LABEL,
                description: BACKEND_DESCRIPTION,
                choices,
                selected: menu_position(self.settings.backend),
                is_enabled: !playground.is_busy(),
            },
            backend_note: playground.load_note(),
        }
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
    use crate::hub::ModelFile;
    use crate::inference::{InferenceError, InferenceEvent};
    use crate::playground::{NoteTone, PlaygroundAction};

    fn model_file() -> ModelFile {
        ModelFile {
            repo: "owner/repo".to_owned(),
            name: "model.gguf".to_owned(),
        }
    }

    fn settings(backend: Backend) -> Settings {
        Settings { backend }
    }

    fn choice(title: &'static str, backend: Backend) -> BackendChoice {
        BackendChoice {
            title,
            action: GeneralAction::SelectBackend(backend),
        }
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

        let view = pane.view(&PlaygroundPane::with_backend(Backend::Vulkan));

        assert_eq!(
            view,
            GeneralView {
                title: "General",
                summary: "Change how GhostType runs on this computer.",
                backend: BackendPicker {
                    label: "Inference backend",
                    description: "The hardware that runs the model. A GPU is faster, but the model must fit in its memory.",
                    choices: vec![
                        choice("CPU", Backend::Cpu),
                        choice("Vulkan (GPU)", Backend::Vulkan),
                        choice("CUDA (NVIDIA GPU)", Backend::Cuda),
                    ],
                    selected: 1,
                    is_enabled: true,
                },
                backend_note: None,
            }
        );
    }

    #[test]
    fn menu_shows_each_saved_backend_as_its_own_choice() {
        let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda];

        let shown = backends.map(|backend| {
            let picker = GeneralPane::new(settings(backend))
                .view(&PlaygroundPane::with_backend(backend))
                .backend;
            picker.choices[picker.selected].action
        });

        assert_eq!(shown, backends.map(GeneralAction::SelectBackend));
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
                settings(Backend::Cuda),
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
            (GeneralOutcome::default(), settings(Backend::Cpu))
        );
    }

    #[test]
    fn loading_model_locks_the_menu_and_shows_status() {
        let pane = GeneralPane::new(settings(Backend::Cuda));

        let view = pane.view(&loading_playground(Backend::Cuda));

        assert_eq!(
            (view.backend.is_enabled, view.backend_note),
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
    fn model_too_large_for_the_gpu_tells_what_to_do() {
        let pane = GeneralPane::new(settings(Backend::Vulkan));

        let view = pane.view(&loaded_playground(
            Backend::Vulkan,
            Err(InferenceError::ModelTooLarge),
        ));

        assert_eq!(
            (view.backend.is_enabled, view.backend_note),
            (
                true,
                Some(Note {
                    text: "The model does not fit in the GPU memory. In General, select another backend, or use a smaller model.",
                    tone: NoteTone::Error,
                }),
            )
        );
    }
}
