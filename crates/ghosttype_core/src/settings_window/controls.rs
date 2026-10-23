//! The parts that several panes share to show their controls and to apply the changes of the user.
//! They are the menus of a value table, the list of apps, and the change of a setting.

use crate::settings_window::view::{ChoiceText, ControlId, Message, Picker, Row, RowIcon, Section};

const APPS_HEADING: &str = "Apps";
const NO_APPS: &str = "This list has no apps.";
/// The footer of a list of apps that GhostType fills as the user types.
pub(super) const TYPED_APPS_FOOTER: &str =
    "When you type in an app for the first time, GhostType adds the app to this list.";

/// The id of a menu, and the icon, the label and the description that the menu shows.
#[derive(Debug, Clone, Copy)]
pub(super) struct MenuText {
    pub(super) id: &'static str,
    pub(super) icon: RowIcon,
    pub(super) label: &'static str,
    pub(super) description: &'static str,
}

/// The menu with `text` that shows the `selected` value of `choices`, with no note.
pub(super) fn picker<Value: PartialEq + Copy>(
    text: MenuText,
    choices: &[(Value, &str)],
    selected: Value,
    is_enabled: bool,
) -> Picker {
    Picker {
        id: ControlId::new(text.id),
        icon: text.icon,
        label: text.label,
        description: text.description,
        choices: choice_titles(choices),
        choice_text: ChoiceText::Label,
        selected: choice_index(choices, selected),
        is_enabled,
        note: None,
    }
}

pub(super) fn choice_titles<Value>(choices: &[(Value, &str)]) -> Vec<String> {
    choices.iter().map(|&(_, title)| title.to_owned()).collect()
}

pub(super) fn choice_index<Value: PartialEq + Copy>(choices: &[(Value, &str)], value: Value) -> usize {
    choices
        .iter()
        .position(|(choice, _)| *choice == value)
        .expect("each value has a menu entry")
}

/// The section of the apps where the user typed, with a message in the place of `rows` when the
/// list has no apps.
pub(super) fn app_list_section(rows: Vec<Row>, footer: &'static str) -> Section {
    list_section(APPS_HEADING, NO_APPS, rows, footer)
}

/// A section with `heading` above `rows` and `footer` below them, with the message `empty` in the
/// place of `rows` when the list is empty.
pub(super) fn list_section(heading: &str, empty: &str, rows: Vec<Row>, footer: &'static str) -> Section {
    let rows = if rows.is_empty() {
        vec![Row::Message(Message {
            text: empty.to_owned(),
            button: None,
        })]
    } else {
        rows
    };
    Section {
        heading: Some(heading.to_owned()),
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
