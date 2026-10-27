//! The parts that several panes share to show their controls and to apply the changes of the user.
//! They are the menus of a value table, the list of apps, and the change of a setting.

use crate::localization::Translations;
use crate::settings_window::view::{ChoiceText, ControlId, Message, Picker, Row, RowIcon, Section};

/// The message of the footer of a list of apps that TypeAlike fills as suggestions show.
pub(super) const SUGGESTED_APPS_FOOTER: &str = "app-list-footer";

/// The id of a menu, and the icon and the messages of the label and the description that the menu
/// shows.
#[derive(Debug, Clone, Copy)]
pub(super) struct MenuText {
    pub(super) id: &'static str,
    pub(super) icon: RowIcon,
    pub(super) label: &'static str,
    pub(super) description: &'static str,
}

/// The enabled menu with `text` that shows the `selected` value of `choices`, each with the
/// message of its title, with no note.
pub(super) fn picker<Value: PartialEq + Copy>(
    text: MenuText,
    choices: &[(Value, &str)],
    selected: Value,
    translations: &Translations,
) -> Picker {
    Picker {
        id: ControlId::new(text.id),
        icon: text.icon,
        label: translations.text(text.label),
        description: translations.text(text.description),
        choices: choice_titles(choices, translations),
        choice_text: ChoiceText::Label,
        selected: choice_index(choices, selected),
        is_enabled: true,
        note: None,
    }
}

/// The title of each of `choices`, from the message of the choice.
pub(super) fn choice_titles<Value>(choices: &[(Value, &str)], translations: &Translations) -> Vec<String> {
    choices
        .iter()
        .map(|&(_, message)| translations.text(message))
        .collect()
}

pub(super) fn choice_index<Value: PartialEq + Copy>(choices: &[(Value, &str)], value: Value) -> usize {
    choices
        .iter()
        .position(|(choice, _)| *choice == value)
        .expect("each value has a menu entry")
}

/// The section of the apps where the user typed, with a message in the place of `rows` when the
/// list has no apps, and the message `footer` below.
pub(super) fn app_list_section(rows: Vec<Row>, footer: &str, translations: &Translations) -> Section {
    list_section(
        translations.text("app-list-heading"),
        translations.text("app-list-empty"),
        rows,
        translations.text(footer),
    )
}

/// A section with `heading` above `rows` and `footer` below them, with the message `empty` in the
/// place of `rows` when the list is empty.
pub(super) fn list_section(heading: String, empty: String, rows: Vec<Row>, footer: String) -> Section {
    let rows = if rows.is_empty() {
        vec![Row::Message(Message {
            text: empty,
            button: None,
        })]
    } else {
        rows
    };
    Section {
        heading: Some(heading),
        back: None,
        rows,
        footer: Some(footer),
    }
}

/// Puts `value` in `slot` when they differ. Returns whether they differed.
pub(super) fn replace_if_changed<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}
