//! The Personalization pane of the settings window: whether suggestions use the text that the
//! user wrote before, in which apps, and how much they prefer the words of the user.

use crate::settings::{
    HistoryAccess, HistoryApp, HistoryScope, KeptTexts, PersonalizationSettings, WordChoice,
};
use crate::settings_view::{
    AppPicker, Button, ChoiceText, ControlId, Message, Note, NoteTone, Page, PageIcon, Picker, Row, Section,
    SettingsInput, Switch,
};

/// Something the user did in the Personalization pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersonalizationAction {
    /// Turned the main switch on or off.
    SwitchHistory(HistoryAccess),
    /// Picked which texts GhostType saves.
    SelectKeptTexts(KeptTexts),
    /// Picked how much suggestions prefer the words of the user.
    SelectWordChoice(WordChoice),
    /// Picked which earlier writing the app with this id gives and gets.
    SelectAppScope {
        /// The id of the app.
        app_id: String,
        /// The scope that the menu shows now.
        scope: HistoryScope,
    },
    /// Pressed the button that deletes all the saved text.
    Delete,
}

/// What GhostType must do after an action of the Personalization pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersonalizationChange {
    /// Nothing changed.
    None,
    /// The settings changed: keep them and use them.
    Settings,
    /// The user turned off the app with this id: keep and use the settings, and delete the saved
    /// text of the app.
    AppOff(String),
    /// Delete all the saved text.
    DeleteAll,
}

pub(crate) const HISTORY: &str = "personalization";
const KEPT_TEXTS: &str = "personalization-kept-texts";
const WORD_CHOICE: &str = "personalization-word-choice";
const APP_PREFIX: &str = "personalization-app:";
pub(crate) const DELETE: &str = "personalization-delete";
const PANE_TITLE: &str = "Personalization";
const PANE_SUMMARY: &str = "Suggestions can use text that you wrote before. GhostType saves this text only on \
     this computer, in a locked file. GhostType does not save text from password fields.";
const HISTORY_LABEL: &str = "Use earlier writing";
const HISTORY_DESCRIPTION: &str =
    "When this switch is on, GhostType saves the text that you write. Suggestions use this text as examples.";
const KEPT_TEXTS_LABEL: &str = "Text to save";
const KEPT_TEXTS_DESCRIPTION: &str =
    "GhostType saves your text when you finish it, for example when you send a message.";
const KEPT_TEXTS_CHOICES: [(KeptTexts, &str); 2] = [
    (KeptTexts::AcceptedOnly, "Only text where you used a suggestion"),
    (KeptTexts::All, "All text"),
];
const WORD_CHOICE_LABEL: &str = "Word choice";
const WORD_CHOICE_DESCRIPTION: &str = "Suggestions use more of the words that you often write. A word that you \
     often write after the same three words shows at once. A stronger setting gives more of these words, but a \
     suggestion can match your text less well.";
const WORD_CHOICE_CHOICES: [(WordChoice, &str); 4] = [
    (WordChoice::Off, "Off"),
    (WordChoice::Light, "Light"),
    (WordChoice::Medium, "Medium"),
    (WordChoice::Strong, "Strong"),
];
const WORD_CHOICE_OFF_NOTE: &str = "To use this setting, turn on “Use earlier writing”.";
const APPS_HEADING: &str = "Apps";
const APP_SCOPE_CHOICES: [(HistoryScope, &str); 3] = [
    (HistoryScope::Off, "Off"),
    (HistoryScope::ThisApp, "Only this app"),
    (HistoryScope::AllApps, "All apps"),
];
const APPS_FOOTER: &str = "GhostType saves text only from the apps that are not off. The text of an app with “All \
     apps” also helps suggestions in the other apps with “All apps”. Suggestions use the text of the same app \
     first. When you type in an app for the first time, GhostType adds the app to this list. When you turn off \
     an app, GhostType deletes the text that it saved from the app.";
const NO_APPS: &str = "This list has no apps.";
const DELETE_DESCRIPTION: &str = "This deletes all the text that GhostType saved. You cannot undo this.";
const DELETE_LABEL: &str = "Delete all earlier writing";
const CONFIRM_DELETE_LABEL: &str = "Press again to delete";

/// State of the Personalization pane: the settings, and whether the delete button waits for its
/// second press.
///
/// ```
/// use ghosttype_core::personalization_pane::{
///     PersonalizationAction, PersonalizationChange, PersonalizationPane,
/// };
/// use ghosttype_core::settings::PersonalizationSettings;
///
/// let mut pane = PersonalizationPane::new(PersonalizationSettings::default());
///
/// assert_eq!(pane.on_action(PersonalizationAction::Delete), PersonalizationChange::None);
/// assert_eq!(pane.on_action(PersonalizationAction::Delete), PersonalizationChange::DeleteAll);
/// ```
#[derive(Debug)]
pub struct PersonalizationPane {
    settings: PersonalizationSettings,
    is_delete_pending: bool,
}

impl PersonalizationPane {
    /// Creates a pane that shows `settings`.
    #[must_use]
    pub fn new(settings: PersonalizationSettings) -> Self {
        Self {
            settings,
            is_delete_pending: false,
        }
    }

    /// Returns the settings that the pane shows.
    #[must_use]
    pub fn settings(&self) -> &PersonalizationSettings {
        &self.settings
    }

    /// Adds `app`, where the user typed, to the list when the list does not have it. Returns
    /// whether the settings changed.
    pub fn add_app(&mut self, app: &HistoryApp) -> bool {
        self.settings.add(app)
    }

    /// Makes the delete button wait for its first press again. Returns whether it waited for its
    /// second press.
    pub fn cancel_delete(&mut self) -> bool {
        std::mem::take(&mut self.is_delete_pending)
    }

    /// Handles a user action. The first press of the delete button only asks for a second press,
    /// and each other action cancels it.
    pub fn on_action(&mut self, action: PersonalizationAction) -> PersonalizationChange {
        let was_delete_pending = self.cancel_delete();
        match action {
            PersonalizationAction::Delete if was_delete_pending => PersonalizationChange::DeleteAll,
            PersonalizationAction::Delete => {
                self.is_delete_pending = true;
                PersonalizationChange::None
            }
            PersonalizationAction::SwitchHistory(history) => change_to(&mut self.settings.history, history),
            PersonalizationAction::SelectKeptTexts(kept) => change_to(&mut self.settings.kept_texts, kept),
            PersonalizationAction::SelectWordChoice(choice) => {
                change_to(&mut self.settings.word_choice, choice)
            }
            PersonalizationAction::SelectAppScope { app_id, scope } => self.select_app_scope(app_id, scope),
        }
    }

    fn select_app_scope(&mut self, app_id: String, scope: HistoryScope) -> PersonalizationChange {
        let Some(app) = self.settings.apps.iter_mut().find(|app| app.app_id == app_id) else {
            return PersonalizationChange::None;
        };
        if app.scope == scope {
            return PersonalizationChange::None;
        }
        app.scope = scope;
        if scope == HistoryScope::Off {
            PersonalizationChange::AppOff(app_id)
        } else {
            PersonalizationChange::Settings
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<PersonalizationAction> {
        match input {
            SettingsInput::Pressed(id) if id.as_str() == DELETE => Some(PersonalizationAction::Delete),
            SettingsInput::Picked { id, index } if id.as_str() == KEPT_TEXTS => KEPT_TEXTS_CHOICES
                .get(*index)
                .map(|&(kept, _)| PersonalizationAction::SelectKeptTexts(kept)),
            SettingsInput::Picked { id, index } if id.as_str() == WORD_CHOICE => WORD_CHOICE_CHOICES
                .get(*index)
                .map(|&(choice, _)| PersonalizationAction::SelectWordChoice(choice)),
            SettingsInput::Toggled { id, is_on } if id.as_str() == HISTORY => {
                Some(PersonalizationAction::SwitchHistory(if *is_on {
                    HistoryAccess::On
                } else {
                    HistoryAccess::Off
                }))
            }
            SettingsInput::Picked { id, index } => {
                let app_id = id.as_str().strip_prefix(APP_PREFIX)?;
                let &(scope, _) = APP_SCOPE_CHOICES.get(*index)?;
                Some(PersonalizationAction::SelectAppScope {
                    app_id: app_id.to_owned(),
                    scope,
                })
            }
            _ => None,
        }
    }

    /// Returns the page that the pane shows now.
    #[must_use]
    pub fn page(&self) -> Page {
        let is_on = self.settings.history == HistoryAccess::On;
        let main_section = Section::new(vec![
            Row::Switch(Switch {
                id: ControlId::new(HISTORY),
                label: HISTORY_LABEL,
                description: HISTORY_DESCRIPTION,
                is_on,
            }),
            Row::Picker(picker(
                (KEPT_TEXTS, KEPT_TEXTS_LABEL, KEPT_TEXTS_DESCRIPTION),
                &KEPT_TEXTS_CHOICES,
                self.settings.kept_texts,
                is_on,
            )),
            Row::Picker(Picker {
                note: (!is_on).then_some(Note {
                    text: WORD_CHOICE_OFF_NOTE,
                    tone: NoteTone::Plain,
                }),
                ..picker(
                    (WORD_CHOICE, WORD_CHOICE_LABEL, WORD_CHOICE_DESCRIPTION),
                    &WORD_CHOICE_CHOICES,
                    self.settings.word_choice,
                    is_on,
                )
            }),
        ]);
        let app_list = Section {
            heading: Some(APPS_HEADING.to_owned()),
            back: None,
            rows: self.app_rows(is_on),
            footer: Some(APPS_FOOTER),
        };
        let delete = Section::new(vec![Row::Message(Message {
            text: DELETE_DESCRIPTION.to_owned(),
            button: Some(Button {
                id: ControlId::new(DELETE),
                label: if self.is_delete_pending {
                    CONFIRM_DELETE_LABEL
                } else {
                    DELETE_LABEL
                },
                is_enabled: true,
            }),
        })]);
        Page {
            title: PANE_TITLE,
            icon: PageIcon::PersonText,
            summary: PANE_SUMMARY,
            search: None,
            sections: vec![main_section, app_list, delete],
        }
    }

    fn app_rows(&self, is_enabled: bool) -> Vec<Row> {
        if self.settings.apps.is_empty() {
            return vec![Row::Message(Message {
                text: NO_APPS.to_owned(),
                button: None,
            })];
        }
        self.settings
            .apps
            .iter()
            .map(|app| {
                Row::AppPicker(AppPicker {
                    id: ControlId::from(format!("{APP_PREFIX}{}", app.app_id)),
                    app_id: app.app_id.clone(),
                    app_path: app.app_path.clone(),
                    app_name: app.app_name.clone(),
                    choices: APP_SCOPE_CHOICES
                        .iter()
                        .map(|&(_, title)| title.to_owned())
                        .collect(),
                    selected: APP_SCOPE_CHOICES
                        .iter()
                        .position(|&(scope, _)| scope == app.scope)
                        .expect("each scope has a menu entry"),
                    is_enabled,
                })
            })
            .collect()
    }
}

/// Sets `setting` to `value`, and tells whether the settings changed.
fn change_to<Value: PartialEq>(setting: &mut Value, value: Value) -> PersonalizationChange {
    if *setting == value {
        return PersonalizationChange::None;
    }
    *setting = value;
    PersonalizationChange::Settings
}

/// The menu with the `id`, `label` and `description` of `text` that shows the `selected` value of
/// `choices`.
fn picker<Value: PartialEq + Copy>(
    (id, label, description): (&'static str, &'static str, &'static str),
    choices: &[(Value, &str)],
    selected: Value,
    is_enabled: bool,
) -> Picker {
    Picker {
        id: ControlId::new(id),
        label,
        description,
        choices: choices.iter().map(|&(_, title)| title.to_owned()).collect(),
        choice_text: ChoiceText::Label,
        selected: choices
            .iter()
            .position(|&(value, _)| value == selected)
            .expect("each value has a menu entry"),
        is_enabled,
        note: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(app_id: &str, app_name: &str, scope: HistoryScope) -> HistoryApp {
        HistoryApp {
            app_id: app_id.to_owned(),
            app_name: app_name.to_owned(),
            app_path: None,
            scope,
        }
    }

    fn pane_with(history: HistoryAccess, apps: Vec<HistoryApp>) -> PersonalizationPane {
        PersonalizationPane::new(PersonalizationSettings {
            history,
            apps,
            ..PersonalizationSettings::default()
        })
    }

    fn toggled(id: &str, is_on: bool) -> SettingsInput {
        SettingsInput::Toggled {
            id: ControlId::from(id.to_owned()),
            is_on,
        }
    }

    fn app_picked(app_id: &str, index: usize) -> SettingsInput {
        SettingsInput::Picked {
            id: ControlId::from(format!("personalization-app:{app_id}")),
            index,
        }
    }

    fn picked(id: &'static str, index: usize) -> SettingsInput {
        SettingsInput::Picked {
            id: ControlId::new(id),
            index,
        }
    }

    fn picker_of<'page>(page: &'page Page, id: &str) -> &'page Picker {
        match page.row(id) {
            Some(Row::Picker(picker)) => picker,
            row => panic!("no picker {id}: {row:?}"),
        }
    }

    fn delete_button(page: &Page) -> Option<&Button> {
        page.sections
            .iter()
            .flat_map(|section| &section.rows)
            .find_map(|row| match row {
                Row::Message(Message { button, .. }) => button.as_ref(),
                _ => None,
            })
    }

    #[test]
    fn page_shows_the_main_switch_off_by_default() {
        let pane = PersonalizationPane::new(PersonalizationSettings::default());

        let shown = pane.page();

        assert_eq!(
            shown.row(HISTORY),
            Some(&Row::Switch(Switch {
                id: ControlId::new(HISTORY),
                label: "Use earlier writing",
                description: "When this switch is on, GhostType saves the text that you write. Suggestions use \
                     this text as examples.",
                is_on: false,
            }))
        );
    }

    #[test]
    fn main_switch_off_dims_the_menus_and_tells_how_to_use_the_word_choice() {
        let pane = pane_with(HistoryAccess::Off, Vec::new());

        let shown = pane.page();

        assert_eq!(
            (
                picker_of(&shown, KEPT_TEXTS).is_enabled,
                picker_of(&shown, WORD_CHOICE).is_enabled,
                picker_of(&shown, WORD_CHOICE).note.as_ref().map(|note| note.text),
            ),
            (
                false,
                false,
                Some("To use this setting, turn on “Use earlier writing”.")
            )
        );
    }

    #[test]
    fn main_switch_on_enables_the_menus() {
        let pane = pane_with(HistoryAccess::On, Vec::new());

        let shown = pane.page();

        assert_eq!(
            (
                picker_of(&shown, KEPT_TEXTS).is_enabled,
                picker_of(&shown, WORD_CHOICE).is_enabled,
                picker_of(&shown, WORD_CHOICE).note.as_ref(),
            ),
            (true, true, None)
        );
    }

    #[test]
    fn menus_show_their_entries_and_the_defaults() {
        let pane = pane_with(HistoryAccess::On, Vec::new());

        let shown = pane.page();

        assert_eq!(
            [KEPT_TEXTS, WORD_CHOICE].map(|id| {
                let picker = picker_of(&shown, id);
                (picker.choices.clone(), picker.selected)
            }),
            [
                (
                    vec![
                        "Only text where you used a suggestion".to_owned(),
                        "All text".to_owned()
                    ],
                    0
                ),
                (
                    ["Off", "Light", "Medium", "Strong"].map(str::to_owned).to_vec(),
                    2
                ),
            ]
        );
    }

    #[test]
    fn app_menus_show_the_scope_of_each_app_and_dim_when_the_main_switch_is_off() {
        let pane = pane_with(
            HistoryAccess::Off,
            vec![app("com.apple.mail", "Mail", HistoryScope::ThisApp)],
        );

        let shown = pane.page();

        assert_eq!(
            shown.sections[1].rows,
            [Row::AppPicker(AppPicker {
                id: ControlId::from("personalization-app:com.apple.mail".to_owned()),
                app_id: "com.apple.mail".to_owned(),
                app_path: None,
                app_name: "Mail".to_owned(),
                choices: ["Off", "Only this app", "All apps"].map(str::to_owned).to_vec(),
                selected: 1,
                is_enabled: false,
            })]
        );
    }

    #[test]
    fn app_list_tells_what_each_scope_does() {
        let pane = pane_with(HistoryAccess::On, Vec::new());

        let shown = pane.page();

        assert_eq!(
            shown.sections[1].footer,
            Some(
                "GhostType saves text only from the apps that are not off. The text of an app with “All \
                 apps” also helps suggestions in the other apps with “All apps”. Suggestions use the text \
                 of the same app first. When you type in an app for the first time, GhostType adds the app \
                 to this list. When you turn off an app, GhostType deletes the text that it saved from the \
                 app."
            )
        );
    }

    #[test]
    fn empty_list_says_that_it_has_no_apps() {
        let pane = pane_with(HistoryAccess::On, Vec::new());

        let shown = pane.page();

        assert_eq!(
            shown.sections[1].rows,
            [Row::Message(Message {
                text: "This list has no apps.".to_owned(),
                button: None,
            })]
        );
    }

    #[test]
    fn controls_give_their_actions() {
        let pane = pane_with(HistoryAccess::On, Vec::new());
        let inputs = [
            toggled(HISTORY, true),
            toggled(HISTORY, false),
            picked(KEPT_TEXTS, 1),
            picked(WORD_CHOICE, 3),
            picked(WORD_CHOICE, 4),
            app_picked("com.apple.mail", 1),
            app_picked("com.apple.mail", 3),
            SettingsInput::Pressed(ControlId::new(DELETE)),
            toggled("screen-text-app:com.apple.mail", false),
        ];

        let actions = inputs.map(|input| pane.action_for(&input));

        assert_eq!(
            actions,
            [
                Some(PersonalizationAction::SwitchHistory(HistoryAccess::On)),
                Some(PersonalizationAction::SwitchHistory(HistoryAccess::Off)),
                Some(PersonalizationAction::SelectKeptTexts(KeptTexts::All)),
                Some(PersonalizationAction::SelectWordChoice(WordChoice::Strong)),
                None,
                Some(PersonalizationAction::SelectAppScope {
                    app_id: "com.apple.mail".to_owned(),
                    scope: HistoryScope::ThisApp,
                }),
                None,
                Some(PersonalizationAction::Delete),
                None,
            ]
        );
    }

    #[test]
    fn actions_change_the_settings() {
        let mut pane = pane_with(HistoryAccess::Off, Vec::new());

        let changes = [
            PersonalizationAction::SwitchHistory(HistoryAccess::On),
            PersonalizationAction::SelectKeptTexts(KeptTexts::All),
            PersonalizationAction::SelectWordChoice(WordChoice::Light),
            PersonalizationAction::SelectWordChoice(WordChoice::Light),
        ]
        .map(|action| pane.on_action(action));

        assert_eq!(
            changes,
            [
                PersonalizationChange::Settings,
                PersonalizationChange::Settings,
                PersonalizationChange::Settings,
                PersonalizationChange::None,
            ]
        );
        assert_eq!(
            pane.settings(),
            &PersonalizationSettings {
                history: HistoryAccess::On,
                kept_texts: KeptTexts::All,
                apps: Vec::new(),
                word_choice: WordChoice::Light,
            }
        );
    }

    #[test]
    fn app_off_asks_to_delete_its_text_and_other_scopes_only_change_the_settings() {
        let mut pane = pane_with(
            HistoryAccess::On,
            vec![app("com.apple.mail", "Mail", HistoryScope::AllApps)],
        );
        let select = |scope| PersonalizationAction::SelectAppScope {
            app_id: "com.apple.mail".to_owned(),
            scope,
        };

        let changes = [
            HistoryScope::ThisApp,
            HistoryScope::Off,
            HistoryScope::Off,
            HistoryScope::AllApps,
        ]
        .map(|scope| pane.on_action(select(scope)));

        assert_eq!(
            changes,
            [
                PersonalizationChange::Settings,
                PersonalizationChange::AppOff("com.apple.mail".to_owned()),
                PersonalizationChange::None,
                PersonalizationChange::Settings,
            ]
        );
    }

    #[test]
    fn unknown_app_changes_nothing() {
        let mut pane = pane_with(HistoryAccess::On, Vec::new());

        let change = pane.on_action(PersonalizationAction::SelectAppScope {
            app_id: "unknown.exe".to_owned(),
            scope: HistoryScope::Off,
        });

        assert_eq!(change, PersonalizationChange::None);
    }

    #[test]
    fn first_press_of_the_delete_button_asks_for_a_second_press() {
        let mut pane = pane_with(HistoryAccess::On, Vec::new());

        let change = pane.on_action(PersonalizationAction::Delete);

        assert_eq!(change, PersonalizationChange::None);
        assert_eq!(
            delete_button(&pane.page()),
            Some(&Button {
                id: ControlId::new(DELETE),
                label: "Press again to delete",
                is_enabled: true,
            })
        );
    }

    #[test]
    fn delete_button_tells_what_it_deletes() {
        let pane = pane_with(HistoryAccess::Off, Vec::new());

        let shown = pane.page();

        assert_eq!(
            shown.sections[2].rows,
            [Row::Message(Message {
                text: "This deletes all the text that GhostType saved. You cannot undo this.".to_owned(),
                button: Some(Button {
                    id: ControlId::new(DELETE),
                    label: "Delete all earlier writing",
                    is_enabled: true,
                }),
            })]
        );
    }

    #[test]
    fn other_action_or_cancel_makes_the_delete_button_wait_for_its_first_press() {
        let mut first = pane_with(HistoryAccess::On, Vec::new());
        let mut second = pane_with(HistoryAccess::On, Vec::new());
        first.on_action(PersonalizationAction::Delete);
        second.on_action(PersonalizationAction::Delete);

        first.on_action(PersonalizationAction::SelectWordChoice(WordChoice::Light));
        let was_pending = second.cancel_delete();

        assert!(was_pending);
        assert_eq!(
            [first, second].map(|mut pane| pane.on_action(PersonalizationAction::Delete)),
            [PersonalizationChange::None, PersonalizationChange::None]
        );
    }
}
