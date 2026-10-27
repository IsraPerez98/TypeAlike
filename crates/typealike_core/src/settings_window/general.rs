//! The General pane of the settings window: the settings that apply to all of TypeAlike.

use std::fmt;

use crate::app::StopReason;
use crate::hub::ModelFile;
use crate::inference::{Backend, InferenceRequest, SentenceEnd};
use crate::localization::Translations;
use crate::settings::{
    Autostart, ClipboardAccess, PromptPreparation, Settings, SuggestionDisplay, SuggestionLength, TypoFix,
    TypoRule, WriterName, WriterNotes,
};
use crate::settings_window::controls::{MenuText, picker, replace_if_changed};
use crate::settings_window::playground::{PlaygroundAction, PlaygroundPane};
use crate::settings_window::view::{
    ControlId, Note, NoteTone, Page, PageIcon, Picker, Row, RowIcon, Section, SettingsInput, Switch,
    TextArea, TextLine,
};

/// Something the user did in the General pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneralAction {
    /// Picked this backend to run models.
    SelectBackend(Backend),
    /// Turned on the switch that starts TypeAlike at login.
    EnableAutostart,
    /// Turned off the switch that starts TypeAlike at login.
    DisableAutostart,
    /// Changed the name of the user to this text.
    EditWriterName(WriterName),
    /// Changed the notes about the user to this text.
    EditWriterNotes(WriterNotes),
    /// Turned on or off the switch that lets suggestions use the text on the clipboard.
    SwitchClipboard(ClipboardAccess),
    /// Turned on or off the switch that gives the text around the caret to the model before the
    /// user types.
    SwitchPreparation(PromptPreparation),
    /// Turned on or off the switch that shows each word of a suggestion as soon as the model
    /// writes it.
    SwitchDisplay(SuggestionDisplay),
    /// Turned on or off the switch that hides suggestions while the word before the caret looks
    /// like it has a typo.
    SwitchTypoRule(TypoRule),
    /// Turned on or off the switch that fixes the word before the caret when it looks like it has
    /// a typo.
    SwitchTypoFix(TypoFix),
    /// Picked this suggestion length.
    SelectSuggestionLength(SuggestionLength),
    /// Turned on or off the switch that stops each suggestion at the end of a sentence.
    SwitchSentenceEnd(SentenceEnd),
    /// Picked this downloaded model to suggest text.
    SelectSuggestionModel(ModelFile),
}

/// What the platform must do after a General action.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GeneralOutcome {
    /// The settings to keep for the next run, when they changed.
    pub settings: Option<Settings>,
    /// The request that applies the change to the model runner, if any.
    pub request: Option<InferenceRequest>,
}

const AUTOSTART: &str = "autostart";
pub(crate) const CLIPBOARD: &str = "clipboard";
pub(crate) const PREPARATION: &str = "prompt-preparation";
const DISPLAY: &str = "suggestion-display";
const TYPO_RULE: &str = "typo-rule";
const TYPO_FIX: &str = "typo-fix";
const LENGTH: &str = "suggestion-length";
const SENTENCE_END: &str = "sentence-end";
pub(crate) const SUGGESTION_MODEL: &str = "suggestion-model";
pub(crate) const BACKEND: &str = "backend";
const WRITER_NAME: &str = "writer-name";
const WRITER_NOTES: &str = "writer-notes";
const LENGTH_CHOICES: [(SuggestionLength, &str); 5] = [
    (SuggestionLength::VeryShort, "general-length-very-short"),
    (SuggestionLength::Short, "general-length-short"),
    (SuggestionLength::Medium, "general-length-medium"),
    (SuggestionLength::Long, "general-length-long"),
    (SuggestionLength::VeryLong, "general-length-very-long"),
];
const BACKEND_CHOICES: [(Backend, &str); 3] = [
    (Backend::Cpu, "general-hardware-processor"),
    (Backend::Vulkan, "general-hardware-graphics-card"),
    (Backend::Cuda, "general-hardware-nvidia"),
];

/// State of the General pane: the settings that the user changes in it.
///
/// ```
/// use typealike_core::settings_window::general::{GeneralAction, GeneralPane};
/// use typealike_core::inference::Backend;
/// use typealike_core::settings_window::playground::PlaygroundPane;
/// use typealike_core::settings::Settings;
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

    /// Selects the saved suggestion model in `playground`, so that it loads when TypeAlike starts.
    /// Returns the request to send, if any.
    pub fn load_suggestion_model(&self, playground: &mut PlaygroundPane) -> Option<InferenceRequest> {
        let file = self.settings.suggestion_model.clone()?;
        playground.on_action(PlaygroundAction::SelectModel(file))
    }

    /// Keeps the model of `playground` unloaded while `reason` applies. When `playground` has no
    /// model, it selects the saved suggestion model without a load, so that the panes show it.
    /// Returns the request to send, if any.
    pub fn unload_suggestion_model(
        &self,
        playground: &mut PlaygroundPane,
        reason: StopReason,
    ) -> Option<InferenceRequest> {
        if let Some(file) = self.settings.suggestion_model.clone() {
            playground.select_unloaded_model(file, reason);
        }
        playground.unload_model(reason)
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
            GeneralAction::EditWriterName(name) => {
                self.change_setting(name, |settings| &mut settings.writer_name)
            }
            GeneralAction::EditWriterNotes(notes) => {
                self.change_setting(notes, |settings| &mut settings.writer_notes)
            }
            GeneralAction::SwitchClipboard(access) => {
                self.change_setting(access, |settings| &mut settings.clipboard_access)
            }
            GeneralAction::SwitchPreparation(preparation) => {
                self.change_setting(preparation, |settings| &mut settings.prompt_preparation)
            }
            GeneralAction::SwitchDisplay(display) => {
                self.change_setting(display, |settings| &mut settings.suggestion_display)
            }
            GeneralAction::SwitchTypoRule(rule) => {
                self.change_setting(rule, |settings| &mut settings.typo_rule)
            }
            GeneralAction::SwitchTypoFix(fix) => self.change_setting(fix, |settings| &mut settings.typo_fix),
            GeneralAction::SelectSuggestionLength(length) => {
                self.change_setting(length, |settings| &mut settings.suggestion_length)
            }
            GeneralAction::SwitchSentenceEnd(end) => {
                self.change_setting(end, |settings| &mut settings.sentence_end)
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
            SettingsInput::Toggled { id, is_on } => action_for_switch(id.as_str(), *is_on),
            SettingsInput::Picked { id, index } => {
                self.action_for_pick(id.as_str(), *index, playground, installed)
            }
            SettingsInput::Edited { id, text } => match id.as_str() {
                WRITER_NAME => Some(GeneralAction::EditWriterName(WriterName(text.clone()))),
                WRITER_NOTES => Some(GeneralAction::EditWriterNotes(WriterNotes(text.clone()))),
                _ => None,
            },
            SettingsInput::Pressed(_) | SettingsInput::Submitted { .. } => None,
        }
    }

    /// Returns the page that the pane shows now, with the texts of `translations`, the load state
    /// of the model that `playground` selected and the downloaded `installed` model files. The row
    /// of the `recommended` model, if any, shows below the model menu.
    #[must_use]
    pub fn page(
        &self,
        playground: &PlaygroundPane,
        installed: &[ModelFile],
        recommended: Option<Row>,
        translations: &Translations,
    ) -> Page {
        let model_picker = playground.model_picker(
            installed,
            ControlId::new(SUGGESTION_MODEL),
            translations.text("general-model-description"),
            translations,
        );
        let suggestion_rows = [
            Row::Picker(self.suggestion_length_picker(translations)),
            Row::Switch(self.sentence_end_switch(translations)),
            Row::Picker(model_picker),
        ];
        let backend = self
            .has_backend_menu
            .then(|| Row::Picker(self.backend_picker(playground, translations)));
        let settings = self
            .switch_rows(translations)
            .chain(suggestion_rows)
            .chain(recommended)
            .chain(backend);
        Page {
            title: translations.text("general-title"),
            icon: PageIcon::Gear,
            summary: translations.text("general-summary"),
            search: None,
            sections: vec![
                Section::new(settings.collect()),
                self.writer_section(translations),
            ],
        }
    }

    fn switch_rows(&self, translations: &Translations) -> impl Iterator<Item = Row> {
        let switch = |id, icon, (label, description), is_on| {
            Row::Switch(Switch {
                id: ControlId::new(id),
                icon,
                label: translations.text(label),
                description: translations.text(description),
                is_on,
                note: None,
            })
        };
        let autostart = self.autostart.as_ref().map(|autostart| {
            switch(
                AUTOSTART,
                RowIcon::Power,
                ("general-autostart", "general-autostart-description"),
                autostart.is_enabled(),
            )
        });
        let switches = [
            switch(
                CLIPBOARD,
                RowIcon::Clipboard,
                ("general-clipboard", "general-clipboard-description"),
                self.settings.clipboard_access == ClipboardAccess::Allowed,
            ),
            switch(
                PREPARATION,
                RowIcon::LightningBolt,
                ("general-preparation", "general-preparation-description"),
                self.settings.prompt_preparation == PromptPreparation::OnFocus,
            ),
            switch(
                DISPLAY,
                RowIcon::Stopwatch,
                ("general-display", "general-display-description"),
                self.settings.suggestion_display == SuggestionDisplay::WordByWord,
            ),
            switch(
                TYPO_RULE,
                RowIcon::WarningSign,
                ("general-typo-rule", "general-typo-rule-description"),
                self.settings.typo_rule == TypoRule::HideSuggestions,
            ),
        ];
        let typo_fix = (self.settings.typo_rule == TypoRule::HideSuggestions).then(|| {
            switch(
                TYPO_FIX,
                RowIcon::SpellCheck,
                ("general-typo-fix", "general-typo-fix-description"),
                self.settings.typo_fix == TypoFix::Offer,
            )
        });
        autostart.into_iter().chain(switches).chain(typo_fix)
    }

    fn writer_section(&self, translations: &Translations) -> Section {
        Section::new(vec![
            Row::TextLine(TextLine {
                id: ControlId::new(WRITER_NAME),
                icon: RowIcon::Person,
                label: translations.text("general-name"),
                description: translations.text("general-name-description"),
                text: self.settings.writer_name.0.clone(),
            }),
            Row::TextArea(TextArea {
                id: ControlId::new(WRITER_NOTES),
                icon: RowIcon::Note,
                label: translations.text("general-notes"),
                description: translations.text("general-notes-description"),
                text: self.settings.writer_notes.0.clone(),
                button: None,
            }),
        ])
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

    /// Keeps `value` in the setting that `field` picks, and returns the settings when they changed.
    fn change_setting<T: PartialEq>(
        &mut self,
        value: T,
        field: fn(&mut Settings) -> &mut T,
    ) -> GeneralOutcome {
        if !replace_if_changed(field(&mut self.settings), value) {
            return GeneralOutcome::default();
        }
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

    fn action_for_pick(
        &self,
        id: &str,
        index: usize,
        playground: &PlaygroundPane,
        installed: &[ModelFile],
    ) -> Option<GeneralAction> {
        match id {
            LENGTH => self
                .length_choices()
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

    /// The lengths that the menu offers: up to the longest length that the backend allows.
    fn length_choices(&self) -> &'static [(SuggestionLength, &'static str)] {
        let longest = self.settings.longest_suggestion_length();
        let offered = LENGTH_CHOICES.partition_point(|&(length, _)| length <= longest);
        &LENGTH_CHOICES[..offered]
    }

    fn suggestion_length_picker(&self, translations: &Translations) -> Picker {
        let choices = self.length_choices();
        let text = MenuText {
            id: LENGTH,
            icon: RowIcon::Ruler,
            label: "general-length",
            description: "general-length-description",
        };
        Picker {
            note: (choices.len() < LENGTH_CHOICES.len()).then(|| Note {
                text: translations.text("general-length-processor-note"),
                tone: NoteTone::Plain,
            }),
            ..picker(
                text,
                choices,
                self.settings.suggestion_length_in_use(),
                translations,
            )
        }
    }

    fn sentence_end_switch(&self, translations: &Translations) -> Switch {
        let is_on = self.settings.sentence_end == SentenceEnd::Stop;
        Switch {
            id: ControlId::new(SENTENCE_END),
            icon: RowIcon::StopButton,
            label: translations.text("general-sentence-end"),
            description: translations.text("general-sentence-end-description"),
            is_on,
            note: (!is_on).then(|| Note {
                text: translations.text("general-sentence-end-off-note"),
                tone: NoteTone::Warning,
            }),
        }
    }

    fn backend_picker(&self, playground: &PlaygroundPane, translations: &Translations) -> Picker {
        let text = MenuText {
            id: BACKEND,
            icon: RowIcon::Chip,
            label: "general-hardware",
            description: "general-hardware-description",
        };
        Picker {
            is_enabled: !playground.is_busy(),
            ..picker(text, &BACKEND_CHOICES, self.settings.backend, translations)
        }
    }
}

fn action_for_switch(id: &str, is_on: bool) -> Option<GeneralAction> {
    let (turn_on, turn_off) = match id {
        AUTOSTART => (GeneralAction::EnableAutostart, GeneralAction::DisableAutostart),
        CLIPBOARD => (
            GeneralAction::SwitchClipboard(ClipboardAccess::Allowed),
            GeneralAction::SwitchClipboard(ClipboardAccess::Denied),
        ),
        PREPARATION => (
            GeneralAction::SwitchPreparation(PromptPreparation::OnFocus),
            GeneralAction::SwitchPreparation(PromptPreparation::OnFirstKey),
        ),
        DISPLAY => (
            GeneralAction::SwitchDisplay(SuggestionDisplay::WordByWord),
            GeneralAction::SwitchDisplay(SuggestionDisplay::WhenComplete),
        ),
        TYPO_RULE => (
            GeneralAction::SwitchTypoRule(TypoRule::HideSuggestions),
            GeneralAction::SwitchTypoRule(TypoRule::Suggest),
        ),
        TYPO_FIX => (
            GeneralAction::SwitchTypoFix(TypoFix::Offer),
            GeneralAction::SwitchTypoFix(TypoFix::Off),
        ),
        SENTENCE_END => (
            GeneralAction::SwitchSentenceEnd(SentenceEnd::Stop),
            GeneralAction::SwitchSentenceEnd(SentenceEnd::Continue),
        ),
        _ => return None,
    };
    Some(if is_on { turn_on } else { turn_off })
}

#[cfg(test)]
mod tests;
