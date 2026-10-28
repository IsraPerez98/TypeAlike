//! The General pane of the settings window, and the sections of its settings that other pages
//! show.

use std::{fmt, iter};

use crate::app::StopReason;
use crate::hub::ModelFile;
use crate::inference::{Backend, InferenceRequest, SentenceEnd};
use crate::localization::{Translations, UiLanguage};
use crate::settings::{
    Autostart, ClipboardAccess, NotesExample, PromptPreparation, Settings, Setup, SuggestionDisplay,
    SuggestionLength, TypoFix, TypoRule, WriterName, WriterNotes,
};
use crate::settings_window::controls::{MenuText, picker, replace_if_changed};
use crate::settings_window::playground::{PlaygroundAction, PlaygroundPane};
use crate::settings_window::view::{
    Button, ChoiceText, ControlId, Message, Note, NoteTone, Page, PageIcon, Picker, Row, RowIcon, Section,
    SettingsInput, Switch, TextArea, TextLine,
};

/// Something the user did with a setting of the General pane, on any page that shows it.
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
    /// Picked this example of the notes about the user.
    PickNotesExample(NotesExample),
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
    /// Finished the setup.
    FinishSetup,
}

/// What a press of the button that resets all settings does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetPress {
    /// The first press: the button asks for a second press.
    Armed,
    /// The second press: the settings go back to the settings of the first start.
    Confirmed,
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
pub(crate) const WRITER_NAME: &str = "writer-name";
const WRITER_NOTES: &str = "writer-notes";
const NOTES_EXAMPLE: &str = "writer-notes-example";
const RESET: &str = "reset-settings";
const LENGTH_CHOICES: [(SuggestionLength, &str); 5] = [
    (SuggestionLength::VeryShort, "general-length-very-short"),
    (SuggestionLength::Short, "general-length-short"),
    (SuggestionLength::Medium, "general-length-medium"),
    (SuggestionLength::Long, "general-length-long"),
    (SuggestionLength::VeryLong, "general-length-very-long"),
];
const BACKEND_CHOICES: [(Backend, &str); 3] = [
    (Backend::Cpu, "models-hardware-processor"),
    (Backend::Vulkan, "models-hardware-graphics-card"),
    (Backend::Cuda, "models-hardware-nvidia"),
];

/// State of the General pane: the settings that no other pane keeps. The General page shows some of
/// them, and the sections of the pane show the others on the Copied and screen text,
/// Personalization, Energy saving and Models pages.
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
    language: UiLanguage,
    is_reset_pending: bool,
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
            language: UiLanguage::English,
            is_reset_pending: false,
        }
    }

    /// Gives the notes of each example that the user picks in `language`.
    #[must_use]
    pub fn with_language(self, language: UiLanguage) -> Self {
        Self { language, ..self }
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

    /// Whether the pane has the switch that starts TypeAlike at login.
    #[must_use]
    pub fn has_autostart(&self) -> bool {
        self.autostart.is_some()
    }

    /// Handles a press of the button that resets all settings: the first press asks for a second
    /// press, and the second press confirms the reset.
    pub fn press_reset(&mut self) -> ResetPress {
        if self.cancel_reset() {
            ResetPress::Confirmed
        } else {
            self.is_reset_pending = true;
            ResetPress::Armed
        }
    }

    /// Makes the reset button wait for its first press again. Returns whether it waited for its
    /// second press.
    pub fn cancel_reset(&mut self) -> bool {
        std::mem::take(&mut self.is_reset_pending)
    }

    /// Gives the pane the settings of the `first_start`. The suggestion model stays, because a
    /// reset keeps the downloaded models. A platform with one backend keeps it. Else the backend of
    /// the first start runs the models, and `playground` loads its model again on that backend.
    pub fn reset(&mut self, first_start: Settings, playground: &mut PlaygroundPane) -> GeneralOutcome {
        let backend = first_start.backend;
        self.settings = Settings {
            suggestion_model: self.settings.suggestion_model.take(),
            backend: self.settings.backend,
            ..first_start
        };
        let outcome = if self.has_backend_menu {
            self.select_backend(backend, playground)
        } else {
            GeneralOutcome::default()
        };
        GeneralOutcome {
            settings: Some(self.settings.clone()),
            ..outcome
        }
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
            GeneralAction::PickNotesExample(example) => {
                let notes = example.notes(&Translations::new(self.language));
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
            GeneralAction::FinishSetup => {
                self.change_setting(Setup::Finished, |settings| &mut settings.setup)
            }
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

    /// Returns the page that the pane shows now, with the texts of `translations`. While no model
    /// is downloaded, the `recommended` rows show first, below the reason for them: the row of the
    /// recommended model, after its warning when it has one.
    #[must_use]
    pub fn page(&self, recommended: Vec<Row>, translations: &Translations) -> Page {
        let first_run = (!recommended.is_empty()).then(|| {
            let reason = Row::Note(Note {
                text: translations.text("model-needed"),
                tone: NoteTone::Plain,
            });
            Section::new(iter::once(reason).chain(recommended).collect())
        });
        let autostart = self.autostart.as_ref().map(|autostart| {
            Section::new(vec![switch_row(
                AUTOSTART,
                RowIcon::Power,
                ("general-autostart", "general-autostart-description"),
                autostart.is_enabled(),
                translations,
            )])
        });
        let sections = [
            first_run,
            autostart,
            Some(self.suggestion_section(translations)),
            Some(self.reset_section(translations)),
        ];
        Page {
            title: translations.text("general-title"),
            icon: PageIcon::Gear,
            summary: translations.text("general-summary"),
            search: None,
            sections: sections.into_iter().flatten().collect(),
        }
    }

    /// Returns the section of the switch for the copied text, for the Copied and screen text page.
    #[must_use]
    pub fn copied_text_section(&self, translations: &Translations) -> Section {
        Section::new(vec![switch_row(
            CLIPBOARD,
            RowIcon::Clipboard,
            ("screen-text-clipboard", "screen-text-clipboard-description"),
            self.settings.clipboard_access == ClipboardAccess::Allowed,
            translations,
        )])
    }

    /// Returns the section of the name and the notes of the writer, with the menu of the examples
    /// of the notes, for the Personalization page and the setup.
    #[must_use]
    pub fn writer_section(&self, translations: &Translations) -> Section {
        Section::new(vec![
            Row::TextLine(TextLine {
                id: ControlId::new(WRITER_NAME),
                icon: RowIcon::Person,
                label: translations.text("personalization-name"),
                description: translations.text("personalization-name-description"),
                text: self.settings.writer_name.0.clone(),
            }),
            Row::Picker(self.notes_example_picker(translations)),
            Row::TextArea(TextArea {
                id: ControlId::new(WRITER_NOTES),
                icon: RowIcon::Note,
                label: translations.text("personalization-notes"),
                description: translations.text("personalization-notes-description"),
                text: self.settings.writer_notes.0.clone(),
                button: None,
            }),
        ])
    }

    /// Returns the section of the switch that prepares suggestions early, for the Energy saving
    /// page.
    #[must_use]
    pub fn preparation_section(&self, translations: &Translations) -> Section {
        Section::new(vec![switch_row(
            PREPARATION,
            RowIcon::LightningBolt,
            (
                "energy-saving-preparation",
                "energy-saving-preparation-description",
            ),
            self.settings.prompt_preparation == PromptPreparation::OnFocus,
            translations,
        )])
    }

    /// Returns the section of the suggestion model menu and the backend menu, for the Models page.
    /// `playground` gives the load state of the selected model, and the downloaded `installed`
    /// model files give the entries of the model menu.
    #[must_use]
    pub fn model_section(
        &self,
        playground: &PlaygroundPane,
        installed: &[ModelFile],
        translations: &Translations,
    ) -> Section {
        let model_picker = playground.model_picker(
            installed,
            ControlId::new(SUGGESTION_MODEL),
            translations.text("models-model-description"),
            translations,
        );
        let backend = self
            .has_backend_menu
            .then(|| Row::Picker(self.backend_picker(playground, translations)));
        Section::new(iter::once(Row::Picker(model_picker)).chain(backend).collect())
    }

    /// The menu of the examples of the notes. Its first entry stands for notes that the user
    /// wrote, and picks nothing.
    fn notes_example_picker(&self, translations: &Translations) -> Picker {
        let examples = NotesExample::ALL
            .iter()
            .map(|example| translations.text(example.name_message()));
        let selected = NotesExample::of(&self.settings.writer_notes, translations)
            .and_then(|example| NotesExample::ALL.iter().position(|&each| each == example))
            .map_or(0, |index| index + 1);
        Picker {
            id: ControlId::new(NOTES_EXAMPLE),
            icon: RowIcon::Pencil,
            label: translations.text("personalization-notes-example"),
            description: translations.text("personalization-notes-example-description"),
            choices: iter::once(translations.text("notes-example-own"))
                .chain(examples)
                .collect(),
            choice_text: ChoiceText::Label,
            selected,
            is_enabled: true,
            note: None,
        }
    }

    fn reset_section(&self, translations: &Translations) -> Section {
        let label = if self.is_reset_pending {
            "general-reset-confirm"
        } else {
            "general-reset"
        };
        Section::new(vec![Row::Message(Message {
            text: translations.text("general-reset-description"),
            button: Some(Button {
                id: ControlId::new(RESET),
                label: translations.text(label),
                is_enabled: true,
            }),
        })])
    }

    /// Returns the section of the backend menu, for the setup, when the pane has the menu.
    /// `playground` gives whether the menu is enabled.
    #[must_use]
    pub fn backend_section(
        &self,
        playground: &PlaygroundPane,
        translations: &Translations,
    ) -> Option<Section> {
        self.has_backend_menu
            .then(|| Section::new(vec![Row::Picker(self.backend_picker(playground, translations))]))
    }

    fn suggestion_section(&self, translations: &Translations) -> Section {
        let rows = [
            Row::Picker(self.suggestion_length_picker(translations)),
            Row::Switch(self.sentence_end_switch(translations)),
            switch_row(
                DISPLAY,
                RowIcon::Stopwatch,
                ("general-display", "general-display-description"),
                self.settings.suggestion_display == SuggestionDisplay::WordByWord,
                translations,
            ),
            switch_row(
                TYPO_RULE,
                RowIcon::WarningSign,
                ("general-typo-rule", "general-typo-rule-description"),
                self.settings.typo_rule == TypoRule::HideSuggestions,
                translations,
            ),
        ];
        let typo_fix = (self.settings.typo_rule == TypoRule::HideSuggestions).then(|| {
            switch_row(
                TYPO_FIX,
                RowIcon::SpellCheck,
                ("general-typo-fix", "general-typo-fix-description"),
                self.settings.typo_fix == TypoFix::Offer,
                translations,
            )
        });
        Section::new(rows.into_iter().chain(typo_fix).collect())
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
            NOTES_EXAMPLE => index
                .checked_sub(1)
                .and_then(|example| NotesExample::ALL.get(example))
                .map(|&example| GeneralAction::PickNotesExample(example)),
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
            label: "models-hardware",
            description: "models-hardware-description",
        };
        Picker {
            is_enabled: !playground.is_busy(),
            ..picker(text, &BACKEND_CHOICES, self.settings.backend, translations)
        }
    }
}

/// Whether `input` is a press of the button that resets all settings.
#[must_use]
pub fn presses_reset(input: &SettingsInput) -> bool {
    matches!(input, SettingsInput::Pressed(id) if id.as_str() == RESET)
}

/// A switch row with the label and the description of the `texts` messages.
fn switch_row(
    id: &'static str,
    icon: RowIcon,
    (label, description): (&str, &str),
    is_on: bool,
    translations: &Translations,
) -> Row {
    Row::Switch(Switch {
        id: ControlId::new(id),
        icon,
        label: translations.text(label),
        description: translations.text(description),
        is_on,
        note: None,
    })
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
