//! The General pane of the settings window: the settings that apply to all of GhostType.

use std::fmt;

use crate::hub::ModelFile;
use crate::inference::{Backend, InferenceRequest};
use crate::playground::{PlaygroundAction, PlaygroundPane};
use crate::settings::{
    Autostart, ClipboardAccess, PromptPreparation, Settings, SuggestionDisplay, SuggestionLength, WriterNotes,
};
use crate::settings_view::{
    ChoiceText, ControlId, Page, PageIcon, Picker, Row, Section, SettingsInput, Switch, TextArea,
};

/// Something the user did in the General pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneralAction {
    /// Picked this backend to run models.
    SelectBackend(Backend),
    /// Turned on the switch that starts GhostType at login.
    EnableAutostart,
    /// Turned off the switch that starts GhostType at login.
    DisableAutostart,
    /// Changed the notes about the user to this text.
    EditWriterNotes(WriterNotes),
    /// Turned on the switch that lets suggestions use the text on the clipboard.
    AllowClipboard,
    /// Turned off the switch that lets suggestions use the text on the clipboard.
    DenyClipboard,
    /// Turned on the switch that gives the text around the caret to the model before the user
    /// types.
    PrepareOnFocus,
    /// Turned off the switch that gives the text around the caret to the model before the user
    /// types.
    PrepareOnFirstKey,
    /// Turned on the switch that shows each word of a suggestion as soon as the model writes it.
    ShowWordByWord,
    /// Turned off the switch that shows each word of a suggestion as soon as the model writes it.
    ShowWhenComplete,
    /// Picked this suggestion length.
    SelectSuggestionLength(SuggestionLength),
    /// Picked this downloaded model to suggest text.
    SelectSuggestionModel(ModelFile),
}

/// What the platform must do after a General action.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeneralOutcome {
    /// The settings to keep for the next run, when they changed.
    pub settings: Option<Settings>,
    /// The request that applies the change to the model runner, if any.
    pub request: Option<InferenceRequest>,
}

pub(crate) const AUTOSTART: &str = "autostart";
pub(crate) const CLIPBOARD: &str = "clipboard";
pub(crate) const PREPARATION: &str = "prompt-preparation";
pub(crate) const DISPLAY: &str = "suggestion-display";
pub(crate) const LENGTH: &str = "suggestion-length";
pub(crate) const SUGGESTION_MODEL: &str = "suggestion-model";
pub(crate) const BACKEND: &str = "backend";
pub(crate) const WRITER_NOTES: &str = "writer-notes";
const PANE_TITLE: &str = "General";
const PANE_SUMMARY: &str = "Change how GhostType runs on this computer.";
const AUTOSTART_LABEL: &str = "Open at login";
const AUTOSTART_DESCRIPTION: &str = "Start GhostType when you log in to this computer.";
const SUGGESTION_MODEL_DESCRIPTION: &str =
    "The model that suggests text as you type. GhostType loads it again when it starts.";
const CLIPBOARD_LABEL: &str = "Use copied text";
const CLIPBOARD_DESCRIPTION: &str = "Suggestions can use the text that you copy. GhostType reads the \
     clipboard only when you start to type, and does not save the text.";
const CLIPBOARD_ON_FOCUS_DESCRIPTION: &str = "Suggestions can use the text that you copy. GhostType reads \
     the clipboard when you go to a text field, and does not save the text.";
const PREPARATION_LABEL: &str = "Prepare before you type";
const PREPARATION_DESCRIPTION: &str = "When you go to a text field, the model reads the text around the \
     caret before you type. The first suggestion then shows sooner, but this uses more power.";
const DISPLAY_LABEL: &str = "Show words early";
const DISPLAY_DESCRIPTION: &str = "Each word of a suggestion shows as soon as the model writes it. The first \
     word then shows sooner, but the suggestion grows while you read it.";
const LENGTH_LABEL: &str = "Suggestion length";
const LENGTH_DESCRIPTION: &str = "A short suggestion has 2 or 3 words, a medium suggestion about 5, and a \
     long suggestion about 10. A shorter suggestion shows sooner.";
const BACKEND_LABEL: &str = "Inference backend";
const BACKEND_DESCRIPTION: &str =
    "The hardware that runs the model. A GPU is faster, but the model must fit in its memory.";
const WRITER_NOTES_LABEL: &str = "Notes about you";
const WRITER_NOTES_DESCRIPTION: &str = "Suggestions use these notes. Write facts about you and how you \
     write, for example your job, your languages and the names that you often use. A fact such as “I keep \
     my emails short” works better than an instruction such as “Keep emails short”.";
const LENGTH_CHOICES: [(SuggestionLength, &str); 3] = [
    (SuggestionLength::Short, "Short"),
    (SuggestionLength::Medium, "Medium"),
    (SuggestionLength::Long, "Long"),
];
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
    /// Creates a pane that shows `settings` and the backend menu, with no autostart switch. A saved
    /// backend that the menu does not offer becomes the default backend.
    #[must_use]
    pub fn new(mut settings: Settings) -> Self {
        if !BACKEND_CHOICES
            .iter()
            .any(|&(backend, _)| backend == settings.backend)
        {
            settings.backend = Backend::default();
        }
        Self {
            settings,
            has_backend_menu: true,
            autostart: None,
        }
    }

    /// Removes the backend menu and runs each model on `backend`, for a platform with one backend.
    #[must_use]
    pub fn with_platform_backend(self, backend: Backend) -> Self {
        Self {
            settings: Settings {
                backend,
                ..self.settings
            },
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

    /// Handles a user action. The Playground pane `playground` loads the model that the user
    /// picks, and loads its model again when the backend changes.
    pub fn on_action(&mut self, action: GeneralAction, playground: &mut PlaygroundPane) -> GeneralOutcome {
        match action {
            GeneralAction::SelectBackend(backend) => self.select_backend(backend, playground),
            GeneralAction::EnableAutostart => self.change_autostart(|autostart| autostart.enable()),
            GeneralAction::DisableAutostart => self.change_autostart(|autostart| autostart.disable()),
            GeneralAction::EditWriterNotes(notes) => {
                self.change_setting(notes, |settings| &mut settings.writer_notes)
            }
            GeneralAction::AllowClipboard => self.change_clipboard_access(ClipboardAccess::Allowed),
            GeneralAction::DenyClipboard => self.change_clipboard_access(ClipboardAccess::Denied),
            GeneralAction::PrepareOnFocus => self.change_prompt_preparation(PromptPreparation::OnFocus),
            GeneralAction::PrepareOnFirstKey => self.change_prompt_preparation(PromptPreparation::OnFirstKey),
            GeneralAction::ShowWordByWord => self.change_suggestion_display(SuggestionDisplay::WordByWord),
            GeneralAction::ShowWhenComplete => {
                self.change_suggestion_display(SuggestionDisplay::WhenComplete)
            }
            GeneralAction::SelectSuggestionLength(length) => {
                self.change_setting(length, |settings| &mut settings.suggestion_length)
            }
            GeneralAction::SelectSuggestionModel(file) => GeneralOutcome {
                settings: None,
                request: playground.on_action(PlaygroundAction::SelectModel(file)),
            },
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane. `playground`
    /// and the downloaded `installed` model files give the entries of the suggestion model menu.
    #[must_use]
    pub fn action_for(
        &self,
        input: &SettingsInput,
        playground: &PlaygroundPane,
        installed: &[ModelFile],
    ) -> Option<GeneralAction> {
        match input {
            SettingsInput::Toggled { id, is_on } => switch_action(id.as_str(), *is_on),
            SettingsInput::Picked { id, index } => {
                self.pick_action(id.as_str(), *index, playground, installed)
            }
            SettingsInput::Edited { id, text } => (id.as_str() == WRITER_NOTES)
                .then(|| GeneralAction::EditWriterNotes(WriterNotes(text.clone()))),
            SettingsInput::Pressed(_) | SettingsInput::Submitted { .. } => None,
        }
    }

    /// Returns the page that the pane shows now, with the load state of the model that
    /// `playground` selected and the downloaded `installed` model files.
    #[must_use]
    pub fn page(&self, playground: &PlaygroundPane, installed: &[ModelFile]) -> Page {
        let autostart = self.autostart.as_ref().map(|autostart| {
            Row::Switch(Switch {
                id: ControlId::new(AUTOSTART),
                label: AUTOSTART_LABEL,
                description: AUTOSTART_DESCRIPTION,
                is_on: autostart.is_enabled(),
            })
        });
        let settings = [
            Row::Switch(Switch {
                id: ControlId::new(CLIPBOARD),
                label: CLIPBOARD_LABEL,
                description: self.clipboard_description(),
                is_on: self.settings.clipboard_access == ClipboardAccess::Allowed,
            }),
            Row::Switch(Switch {
                id: ControlId::new(PREPARATION),
                label: PREPARATION_LABEL,
                description: PREPARATION_DESCRIPTION,
                is_on: self.settings.prompt_preparation == PromptPreparation::OnFocus,
            }),
            Row::Switch(Switch {
                id: ControlId::new(DISPLAY),
                label: DISPLAY_LABEL,
                description: DISPLAY_DESCRIPTION,
                is_on: self.settings.suggestion_display == SuggestionDisplay::WordByWord,
            }),
            Row::Picker(self.suggestion_length_picker()),
            Row::Picker(playground.model_picker(
                installed,
                ControlId::new(SUGGESTION_MODEL),
                SUGGESTION_MODEL_DESCRIPTION,
            )),
        ];
        let backend = self
            .has_backend_menu
            .then(|| Row::Picker(self.backend_picker(playground)));
        Page {
            title: PANE_TITLE,
            icon: PageIcon::Gear,
            summary: PANE_SUMMARY,
            search: None,
            sections: vec![
                Section::new(autostart.into_iter().chain(settings).chain(backend).collect()),
                Section::new(vec![Row::TextArea(self.writer_notes_area())]),
            ],
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

    fn change_clipboard_access(&mut self, access: ClipboardAccess) -> GeneralOutcome {
        self.change_setting(access, |settings| &mut settings.clipboard_access)
    }

    fn change_prompt_preparation(&mut self, preparation: PromptPreparation) -> GeneralOutcome {
        self.change_setting(preparation, |settings| &mut settings.prompt_preparation)
    }

    fn change_suggestion_display(&mut self, display: SuggestionDisplay) -> GeneralOutcome {
        self.change_setting(display, |settings| &mut settings.suggestion_display)
    }

    /// Keeps `value` in the setting that `field` picks, and returns the settings when they changed.
    fn change_setting<T: PartialEq>(
        &mut self,
        value: T,
        field: fn(&mut Settings) -> &mut T,
    ) -> GeneralOutcome {
        let setting = field(&mut self.settings);
        if *setting == value {
            return GeneralOutcome::default();
        }
        *setting = value;
        GeneralOutcome {
            settings: Some(self.settings.clone()),
            request: None,
        }
    }

    fn change_autostart(&mut self, change: fn(&mut dyn Autostart)) -> GeneralOutcome {
        if let Some(autostart) = &mut self.autostart {
            change(autostart.as_mut());
        }
        GeneralOutcome::default()
    }

    fn clipboard_description(&self) -> &'static str {
        match self.settings.prompt_preparation {
            PromptPreparation::OnFirstKey => CLIPBOARD_DESCRIPTION,
            PromptPreparation::OnFocus => CLIPBOARD_ON_FOCUS_DESCRIPTION,
        }
    }

    fn pick_action(
        &self,
        id: &str,
        index: usize,
        playground: &PlaygroundPane,
        installed: &[ModelFile],
    ) -> Option<GeneralAction> {
        match id {
            LENGTH => LENGTH_CHOICES
                .get(index)
                .map(|&(length, _)| GeneralAction::SelectSuggestionLength(length)),
            BACKEND if self.has_backend_menu => BACKEND_CHOICES
                .get(index)
                .map(|&(backend, _)| GeneralAction::SelectBackend(backend)),
            SUGGESTION_MODEL => playground
                .model_at(installed, index)
                .map(GeneralAction::SelectSuggestionModel),
            _ => None,
        }
    }

    fn suggestion_length_picker(&self) -> Picker {
        Picker {
            id: ControlId::new(LENGTH),
            label: LENGTH_LABEL,
            description: LENGTH_DESCRIPTION,
            choices: choice_titles(&LENGTH_CHOICES),
            choice_text: ChoiceText::Label,
            selected: LENGTH_CHOICES
                .iter()
                .position(|&(length, _)| length == self.settings.suggestion_length)
                .expect("every suggestion length has a menu entry"),
            is_enabled: true,
            note: None,
        }
    }

    fn backend_picker(&self, playground: &PlaygroundPane) -> Picker {
        Picker {
            id: ControlId::new(BACKEND),
            label: BACKEND_LABEL,
            description: BACKEND_DESCRIPTION,
            choices: choice_titles(&BACKEND_CHOICES),
            choice_text: ChoiceText::Label,
            selected: BACKEND_CHOICES
                .iter()
                .position(|&(backend, _)| backend == self.settings.backend)
                .expect("every backend has a menu entry"),
            is_enabled: !playground.is_busy(),
            note: None,
        }
    }

    fn writer_notes_area(&self) -> TextArea {
        TextArea {
            id: ControlId::new(WRITER_NOTES),
            label: WRITER_NOTES_LABEL,
            description: WRITER_NOTES_DESCRIPTION,
            text: self.settings.writer_notes.0.clone(),
            button: None,
        }
    }
}

/// The action of turning the switch `id` on or off.
fn switch_action(id: &str, is_on: bool) -> Option<GeneralAction> {
    let (turn_on, turn_off) = match id {
        AUTOSTART => (GeneralAction::EnableAutostart, GeneralAction::DisableAutostart),
        CLIPBOARD => (GeneralAction::AllowClipboard, GeneralAction::DenyClipboard),
        PREPARATION => (GeneralAction::PrepareOnFocus, GeneralAction::PrepareOnFirstKey),
        DISPLAY => (GeneralAction::ShowWordByWord, GeneralAction::ShowWhenComplete),
        _ => return None,
    };
    Some(if is_on { turn_on } else { turn_off })
}

fn choice_titles<Value>(choices: &[(Value, &str)]) -> Vec<String> {
    choices.iter().map(|(_, title)| (*title).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{InferenceError, InferenceEvent};
    use crate::settings_view::{Note, NoteTone};

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

    fn notes_settings(notes: &str) -> Settings {
        Settings {
            writer_notes: WriterNotes(notes.to_owned()),
            ..Settings::default()
        }
    }

    fn settings_outcome(settings: Settings) -> GeneralOutcome {
        GeneralOutcome {
            settings: Some(settings),
            request: None,
        }
    }

    fn clipboard_settings(clipboard_access: ClipboardAccess) -> Settings {
        Settings {
            clipboard_access,
            ..Settings::default()
        }
    }

    fn preparation_settings(prompt_preparation: PromptPreparation) -> Settings {
        Settings {
            prompt_preparation,
            ..Settings::default()
        }
    }

    fn display_settings(suggestion_display: SuggestionDisplay) -> Settings {
        Settings {
            suggestion_display,
            ..Settings::default()
        }
    }

    fn edit_writer_notes(pane: &mut GeneralPane, text: &str) -> GeneralOutcome {
        pane.on_action(
            GeneralAction::EditWriterNotes(WriterNotes(text.to_owned())),
            &mut PlaygroundPane::new(),
        )
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

    fn default_page(pane: &GeneralPane) -> Page {
        pane.page(&PlaygroundPane::new(), &[])
    }

    fn row(page: &Page, id: &str) -> Row {
        page.row(id).cloned().expect("the page shows the control")
    }

    fn switch(page: &Page, id: &str) -> Switch {
        match row(page, id) {
            Row::Switch(switch) => switch,
            other => panic!("the control is not a switch: {other:?}"),
        }
    }

    fn picker(page: &Page, id: &str) -> Picker {
        match row(page, id) {
            Row::Picker(picker) => picker,
            other => panic!("the control is not a menu: {other:?}"),
        }
    }

    fn text_area(page: &Page, id: &str) -> TextArea {
        match row(page, id) {
            Row::TextArea(text_area) => text_area,
            other => panic!("the control is not a text area: {other:?}"),
        }
    }

    fn toggled(id: &'static str, is_on: bool) -> SettingsInput {
        SettingsInput::Toggled {
            id: ControlId::new(id),
            is_on,
        }
    }

    fn picked(id: &'static str, index: usize) -> SettingsInput {
        SettingsInput::Picked {
            id: ControlId::new(id),
            index,
        }
    }

    fn action_for(pane: &GeneralPane, input: &SettingsInput) -> Option<GeneralAction> {
        pane.action_for(input, &PlaygroundPane::new(), &[])
    }

    #[test]
    fn page_shows_the_settings_and_then_the_writer_notes() {
        let pane = GeneralPane::new(settings(Backend::Vulkan));

        let shown = pane.page(&PlaygroundPane::with_backend(Backend::Vulkan), &[]);

        assert_eq!(
            shown,
            Page {
                title: "General",
                icon: PageIcon::Gear,
                summary: "Change how GhostType runs on this computer.",
                search: None,
                sections: vec![
                    Section::new(vec![
                        Row::Switch(Switch {
                            id: ControlId::new(CLIPBOARD),
                            label: "Use copied text",
                            description: "Suggestions can use the text that you copy. GhostType reads the clipboard only when you start to type, and does not save the text.",
                            is_on: false,
                        }),
                        Row::Switch(Switch {
                            id: ControlId::new(PREPARATION),
                            label: "Prepare before you type",
                            description: "When you go to a text field, the model reads the text around the caret before you type. The first suggestion then shows sooner, but this uses more power.",
                            is_on: false,
                        }),
                        Row::Switch(Switch {
                            id: ControlId::new(DISPLAY),
                            label: "Show words early",
                            description: "Each word of a suggestion shows as soon as the model writes it. The first word then shows sooner, but the suggestion grows while you read it.",
                            is_on: false,
                        }),
                        Row::Picker(Picker {
                            id: ControlId::new(LENGTH),
                            label: "Suggestion length",
                            description: "A short suggestion has 2 or 3 words, a medium suggestion about 5, and a long suggestion about 10. A shorter suggestion shows sooner.",
                            choices: vec!["Short".to_owned(), "Medium".to_owned(), "Long".to_owned()],
                            choice_text: ChoiceText::Label,
                            selected: 1,
                            is_enabled: true,
                            note: None,
                        }),
                        Row::Picker(Picker {
                            id: ControlId::new(SUGGESTION_MODEL),
                            label: "Suggestion model",
                            description: "The model that suggests text as you type. GhostType loads it again when it starts.",
                            choices: vec!["No downloaded models".to_owned()],
                            choice_text: ChoiceText::Name,
                            selected: 0,
                            is_enabled: false,
                            note: Some(Note {
                                text: "Download a model in the Models pane.",
                                tone: NoteTone::Plain,
                            }),
                        }),
                        Row::Picker(Picker {
                            id: ControlId::new(BACKEND),
                            label: "Inference backend",
                            description: "The hardware that runs the model. A GPU is faster, but the model must fit in its memory.",
                            choices: vec![
                                "CPU".to_owned(),
                                "Vulkan (GPU)".to_owned(),
                                "CUDA (NVIDIA GPU)".to_owned(),
                            ],
                            choice_text: ChoiceText::Label,
                            selected: 1,
                            is_enabled: true,
                            note: None,
                        }),
                    ]),
                    Section::new(vec![Row::TextArea(TextArea {
                        id: ControlId::new(WRITER_NOTES),
                        label: "Notes about you",
                        description: "Suggestions use these notes. Write facts about you and how you write, for example your job, your languages and the names that you often use. A fact such as “I keep my emails short” works better than an instruction such as “Keep emails short”.",
                        text: String::new(),
                        button: None,
                    })]),
                ],
            }
        );
    }

    #[test]
    fn menu_shows_each_saved_backend_as_its_own_choice() {
        let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda];

        let shown = backends.map(|backend| {
            let pane = GeneralPane::new(settings(backend));
            let selected = picker(&default_page(&pane), BACKEND).selected;
            action_for(&pane, &picked(BACKEND, selected))
        });

        assert_eq!(
            shown,
            backends.map(|backend| Some(GeneralAction::SelectBackend(backend)))
        );
    }

    #[test]
    fn pane_with_a_platform_backend_hides_the_menu() {
        let pane = GeneralPane::new(settings(Backend::Cpu)).with_platform_backend(Backend::Metal);

        let shown = pane.page(&loading_playground(Backend::Metal), &[]);

        assert_eq!(shown.row(BACKEND), None);
    }

    #[test]
    fn platform_backend_replaces_the_saved_backend() {
        let pane = GeneralPane::new(settings(Backend::Cpu)).with_platform_backend(Backend::Metal);

        assert_eq!(pane.settings().backend, Backend::Metal);
    }

    #[test]
    fn saved_backend_that_the_menu_does_not_offer_becomes_the_default() {
        let pane = GeneralPane::new(settings(Backend::Metal));

        assert_eq!(pane.settings().backend, Backend::default());
    }

    #[test]
    fn turning_a_switch_turns_its_setting_on_or_off() {
        let pane = GeneralPane::new(Settings::default());
        let inputs = [
            toggled(AUTOSTART, true),
            toggled(AUTOSTART, false),
            toggled(CLIPBOARD, true),
            toggled(CLIPBOARD, false),
            toggled(PREPARATION, true),
            toggled(PREPARATION, false),
            toggled(DISPLAY, true),
            toggled(DISPLAY, false),
        ];

        let actions = inputs.map(|input| action_for(&pane, &input));

        assert_eq!(
            actions,
            [
                GeneralAction::EnableAutostart,
                GeneralAction::DisableAutostart,
                GeneralAction::AllowClipboard,
                GeneralAction::DenyClipboard,
                GeneralAction::PrepareOnFocus,
                GeneralAction::PrepareOnFirstKey,
                GeneralAction::ShowWordByWord,
                GeneralAction::ShowWhenComplete,
            ]
            .map(Some)
        );
    }

    #[test]
    fn picking_a_suggestion_length_selects_it() {
        let pane = GeneralPane::new(Settings::default());

        let action = action_for(&pane, &picked(LENGTH, 0));

        assert_eq!(
            action,
            Some(GeneralAction::SelectSuggestionLength(SuggestionLength::Short))
        );
    }

    #[test]
    fn picking_past_the_last_entry_does_nothing() {
        let pane = GeneralPane::new(Settings::default());

        let action = action_for(&pane, &picked(LENGTH, LENGTH_CHOICES.len()));

        assert_eq!(action, None);
    }

    #[test]
    fn picking_a_backend_without_backend_menu_does_nothing() {
        let pane = GeneralPane::new(settings(Backend::Cpu)).with_platform_backend(Backend::Metal);

        let action = action_for(&pane, &picked(BACKEND, 1));

        assert_eq!(action, None);
    }

    #[test]
    fn picking_a_suggestion_model_selects_it() {
        let pane = GeneralPane::new(Settings::default());
        let installed = [named_file("other.gguf"), model_file()];

        let action = pane.action_for(&picked(SUGGESTION_MODEL, 2), &PlaygroundPane::new(), &installed);

        assert_eq!(action, Some(GeneralAction::SelectSuggestionModel(model_file())));
    }

    #[test]
    fn picking_the_placeholder_model_does_nothing() {
        let pane = GeneralPane::new(Settings::default());

        let action = pane.action_for(
            &picked(SUGGESTION_MODEL, 0),
            &PlaygroundPane::new(),
            &[model_file()],
        );

        assert_eq!(action, None);
    }

    #[test]
    fn editing_the_notes_field_edits_the_writer_notes() {
        let pane = GeneralPane::new(Settings::default());
        let input = SettingsInput::Edited {
            id: ControlId::new(WRITER_NOTES),
            text: "I am a nurse.".to_owned(),
        };

        let action = action_for(&pane, &input);

        assert_eq!(
            action,
            Some(GeneralAction::EditWriterNotes(WriterNotes(
                "I am a nurse.".to_owned()
            )))
        );
    }

    #[test]
    fn input_for_a_control_of_another_pane_is_not_for_this_pane() {
        let pane = GeneralPane::new(Settings::default());

        let action = action_for(&pane, &SettingsInput::Pressed(ControlId::new("download")));

        assert_eq!(action, None);
    }

    #[test]
    fn selecting_a_suggestion_model_loads_it_in_the_playground() {
        let mut pane = GeneralPane::new(Settings::default());
        let mut playground = PlaygroundPane::new();

        let outcome = pane.on_action(
            GeneralAction::SelectSuggestionModel(model_file()),
            &mut playground,
        );

        assert_eq!(
            outcome,
            GeneralOutcome {
                settings: None,
                request: Some(InferenceRequest::Load {
                    file: model_file(),
                    backend: Backend::Cpu,
                }),
            }
        );
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
                backend: Backend::Vulkan,
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
    fn loading_model_locks_the_backend_menu() {
        let pane = GeneralPane::new(settings(Backend::Cuda));

        let shown = pane.page(&loading_playground(Backend::Cuda), &[]);

        assert!(!picker(&shown, BACKEND).is_enabled);
    }

    #[test]
    fn model_too_large_for_the_gpu_tells_what_to_do() {
        let pane = GeneralPane::new(settings(Backend::Vulkan));

        let shown = pane.page(
            &loaded_playground(Backend::Vulkan, Err(InferenceError::ModelTooLarge)),
            &[model_file()],
        );

        assert_eq!(
            (
                picker(&shown, BACKEND).is_enabled,
                picker(&shown, SUGGESTION_MODEL).note
            ),
            (
                true,
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

        let shown = default_page(&pane);

        assert_eq!(
            switch(&shown, AUTOSTART),
            Switch {
                id: ControlId::new(AUTOSTART),
                label: "Open at login",
                description: "Start GhostType when you log in to this computer.",
                is_on: true,
            }
        );
    }

    #[test]
    fn autostart_switch_comes_first() {
        let pane = autostart_pane(false);

        let shown = default_page(&pane);

        assert_eq!(shown.sections[0].rows[0].id(), Some(&ControlId::new(AUTOSTART)));
    }

    #[test]
    fn enabling_autostart_turns_the_switch_on() {
        let mut pane = autostart_pane(false);

        let outcome = pane.on_action(GeneralAction::EnableAutostart, &mut PlaygroundPane::new());

        let is_on = switch(&default_page(&pane), AUTOSTART).is_on;
        assert_eq!((outcome, is_on), (GeneralOutcome::default(), true));
    }

    #[test]
    fn disabling_autostart_turns_the_switch_off() {
        let mut pane = autostart_pane(true);

        pane.on_action(GeneralAction::DisableAutostart, &mut PlaygroundPane::new());

        assert!(!switch(&default_page(&pane), AUTOSTART).is_on);
    }

    #[test]
    fn switch_shows_the_kept_state_when_the_os_refuses_a_change() {
        let mut pane = GeneralPane::new(Settings::default())
            .with_autostart(Box::new(RefusingAutostart { is_enabled: false }));

        pane.on_action(GeneralAction::EnableAutostart, &mut PlaygroundPane::new());

        assert!(!switch(&default_page(&pane), AUTOSTART).is_on);
    }

    #[test]
    fn suggestion_model_menu_shows_the_model_that_the_playground_selected() {
        let other = named_file("other.gguf");
        let pane = GeneralPane::new(suggestion_settings(None));

        let shown = pane.page(&loaded_playground(Backend::Cpu, Ok(())), &[other, model_file()]);

        let menu = picker(&shown, SUGGESTION_MODEL);
        assert_eq!(
            (menu.choices, menu.selected, menu.is_enabled, menu.note),
            (vec!["other".to_owned(), "model".to_owned()], 1, true, None)
        );
    }

    #[test]
    fn suggestion_model_menu_locks_and_shows_status_while_the_model_loads() {
        let pane = GeneralPane::new(suggestion_settings(None));

        let shown = pane.page(&loading_playground(Backend::Cpu), &[model_file()]);

        let menu = picker(&shown, SUGGESTION_MODEL);
        assert_eq!(
            (menu.is_enabled, menu.note),
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
            (request, playground.is_busy()),
            (
                Some(InferenceRequest::Load {
                    file: model_file(),
                    backend: Backend::Cpu,
                }),
                true,
            )
        );
    }

    #[test]
    fn unloaded_model_stays_the_suggestion_model() {
        let mut pane = GeneralPane::new(suggestion_settings(Some(model_file())));
        let mut playground = loaded_playground(Backend::Cpu, Ok(()));
        playground.unload_model();

        let settings = pane.keep_suggestion_model(&playground);

        assert_eq!(
            (settings, pane.settings()),
            (None, &suggestion_settings(Some(model_file())))
        );
    }

    #[test]
    fn unloaded_suggestion_model_loads_again() {
        let pane = GeneralPane::new(suggestion_settings(Some(model_file())));
        let mut playground = loaded_playground(Backend::Cpu, Ok(()));
        playground.unload_model();

        let request = pane.load_suggestion_model(&mut playground);

        assert_eq!(
            request,
            Some(InferenceRequest::Load {
                file: model_file(),
                backend: Backend::Cpu,
            })
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
    fn notes_field_shows_the_writer_notes() {
        let pane = GeneralPane::new(notes_settings("I am a nurse."));

        let field = text_area(&default_page(&pane), WRITER_NOTES);

        assert_eq!(field.text, "I am a nurse.");
    }

    #[test]
    fn editing_the_writer_notes_keeps_them() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = edit_writer_notes(&mut pane, "I am a nurse.");

        assert_eq!(
            (outcome, pane.settings()),
            (
                settings_outcome(notes_settings("I am a nurse.")),
                &notes_settings("I am a nurse."),
            )
        );
    }

    #[test]
    fn emptied_writer_notes_are_the_default() {
        let mut pane = GeneralPane::new(notes_settings("I am a nurse."));

        let outcome = edit_writer_notes(&mut pane, "");

        assert_eq!(outcome, settings_outcome(Settings::default()));
    }

    #[test]
    fn editing_the_writer_notes_to_the_same_text_does_nothing() {
        let mut pane = GeneralPane::new(notes_settings("I am a nurse."));

        let outcome = edit_writer_notes(&mut pane, "I am a nurse.");

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn allowing_the_clipboard_keeps_it_and_turns_the_switch_on() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = pane.on_action(GeneralAction::AllowClipboard, &mut PlaygroundPane::new());

        let is_on = switch(&default_page(&pane), CLIPBOARD).is_on;
        assert_eq!(
            (outcome, is_on),
            (
                settings_outcome(clipboard_settings(ClipboardAccess::Allowed)),
                true
            )
        );
    }

    #[test]
    fn denying_the_clipboard_keeps_it_and_turns_the_switch_off() {
        let mut pane = GeneralPane::new(clipboard_settings(ClipboardAccess::Allowed));

        let outcome = pane.on_action(GeneralAction::DenyClipboard, &mut PlaygroundPane::new());

        let is_on = switch(&default_page(&pane), CLIPBOARD).is_on;
        assert_eq!((outcome, is_on), (settings_outcome(Settings::default()), false));
    }

    #[test]
    fn choosing_the_current_clipboard_access_does_nothing() {
        let mut pane = GeneralPane::new(clipboard_settings(ClipboardAccess::Allowed));

        let outcome = pane.on_action(GeneralAction::AllowClipboard, &mut PlaygroundPane::new());

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn preparing_on_focus_keeps_it_and_turns_the_switch_on() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = pane.on_action(GeneralAction::PrepareOnFocus, &mut PlaygroundPane::new());

        let is_on = switch(&default_page(&pane), PREPARATION).is_on;
        assert_eq!(
            (outcome, is_on),
            (
                settings_outcome(preparation_settings(PromptPreparation::OnFocus)),
                true
            )
        );
    }

    #[test]
    fn preparing_on_the_first_key_keeps_it_and_turns_the_switch_off() {
        let mut pane = GeneralPane::new(preparation_settings(PromptPreparation::OnFocus));

        let outcome = pane.on_action(GeneralAction::PrepareOnFirstKey, &mut PlaygroundPane::new());

        let is_on = switch(&default_page(&pane), PREPARATION).is_on;
        assert_eq!((outcome, is_on), (settings_outcome(Settings::default()), false));
    }

    #[test]
    fn choosing_the_current_preparation_does_nothing() {
        let mut pane = GeneralPane::new(preparation_settings(PromptPreparation::OnFocus));

        let outcome = pane.on_action(GeneralAction::PrepareOnFocus, &mut PlaygroundPane::new());

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn showing_word_by_word_keeps_it_and_turns_the_switch_on() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = pane.on_action(GeneralAction::ShowWordByWord, &mut PlaygroundPane::new());

        let is_on = switch(&default_page(&pane), DISPLAY).is_on;
        assert_eq!(
            (outcome, is_on),
            (
                settings_outcome(display_settings(SuggestionDisplay::WordByWord)),
                true
            )
        );
    }

    #[test]
    fn showing_when_complete_keeps_it_and_turns_the_switch_off() {
        let mut pane = GeneralPane::new(display_settings(SuggestionDisplay::WordByWord));

        let outcome = pane.on_action(GeneralAction::ShowWhenComplete, &mut PlaygroundPane::new());

        let is_on = switch(&default_page(&pane), DISPLAY).is_on;
        assert_eq!((outcome, is_on), (settings_outcome(Settings::default()), false));
    }

    #[test]
    fn choosing_the_current_suggestion_display_does_nothing() {
        let mut pane = GeneralPane::new(display_settings(SuggestionDisplay::WordByWord));

        let outcome = pane.on_action(GeneralAction::ShowWordByWord, &mut PlaygroundPane::new());

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn selecting_a_suggestion_length_keeps_it_and_selects_its_entry() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = pane.on_action(
            GeneralAction::SelectSuggestionLength(SuggestionLength::Short),
            &mut PlaygroundPane::new(),
        );

        let selected = picker(&default_page(&pane), LENGTH).selected;
        let short_settings = Settings {
            suggestion_length: SuggestionLength::Short,
            ..Settings::default()
        };
        assert_eq!((outcome, selected), (settings_outcome(short_settings), 0));
    }

    #[test]
    fn selecting_the_current_suggestion_length_does_nothing() {
        let mut pane = GeneralPane::new(Settings::default());

        let outcome = pane.on_action(
            GeneralAction::SelectSuggestionLength(SuggestionLength::Medium),
            &mut PlaygroundPane::new(),
        );

        assert_eq!(outcome, GeneralOutcome::default());
    }

    #[test]
    fn clipboard_description_tells_that_preparing_on_focus_reads_the_clipboard_before_a_key() {
        let pane = GeneralPane::new(preparation_settings(PromptPreparation::OnFocus));

        let clipboard = switch(&default_page(&pane), CLIPBOARD);

        assert_eq!(
            clipboard.description,
            "Suggestions can use the text that you copy. GhostType reads the clipboard when you go to a text field, and does not save the text."
        );
    }
}
