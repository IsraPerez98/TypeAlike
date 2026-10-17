//! What the settings window shows, as a tree of pages, sections and rows, and what the user does
//! in it. Each platform renders each kind of row with its own controls, so a new setting needs no
//! platform code.

use std::borrow::Cow;
use std::fmt;

/// Names a control of the settings window. A control keeps its id from one render to the next,
/// and the platform gives the id back with each input of the control.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControlId(Cow<'static, str>);

impl ControlId {
    /// Names a control `name`.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// The name of the control.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One page of the settings window, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The title, in sentence case.
    pub title: &'static str,
    /// One or two sentences under the title about what the page is for.
    pub summary: &'static str,
    /// The sections, top to bottom.
    pub sections: Vec<Section>,
}

/// Rows that belong together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The rows, top to bottom.
    pub rows: Vec<Row>,
}

/// One row of a section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A setting that is on or off.
    Switch(Switch),
    /// A setting that takes one value of a list.
    Picker(Picker),
    /// Text of several lines that the user writes.
    TextArea(TextArea),
}

impl Row {
    /// The id of the control that the row holds, if it holds one.
    #[must_use]
    pub fn id(&self) -> Option<&ControlId> {
        match self {
            Self::Switch(switch) => Some(&switch.id),
            Self::Picker(picker) => Some(&picker.id),
            Self::TextArea(text_area) => Some(&text_area.id),
        }
    }
}

/// A switch that turns a setting on and off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switch {
    /// The switch. Turning it gives [`SettingsInput::Toggled`].
    pub id: ControlId,
    /// Text before the switch, in sentence case.
    pub label: &'static str,
    /// A sentence below the label about what the setting changes.
    pub description: &'static str,
    /// Whether the switch shows on.
    pub is_on: bool,
}

/// A menu that picks the value of a setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    /// The menu. Picking an entry gives [`SettingsInput::Picked`].
    pub id: ControlId,
    /// Text before the menu, in sentence case.
    pub label: &'static str,
    /// A sentence below the label about what the setting changes; empty when there is none.
    pub description: &'static str,
    /// Menu entries, top to bottom. Never empty.
    pub choices: Vec<String>,
    /// What the text of the entries is.
    pub choice_text: ChoiceText,
    /// Index of the entry the menu shows.
    pub selected: usize,
    /// Whether the user can open the menu.
    pub is_enabled: bool,
    /// The status of the setting, or a hint, below the description.
    pub note: Option<Note>,
}

/// What the text of the entries of a menu is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceText {
    /// Labels in sentence case, which each platform shows in its own case.
    Label,
    /// Names, such as the names of models, which each platform shows as they are.
    Name,
}

/// A field that edits text of several lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextArea {
    /// The field. Each change of its text gives [`SettingsInput::Edited`].
    pub id: ControlId,
    /// Text before the field, in sentence case.
    pub label: &'static str,
    /// A sentence below the label about what the text changes.
    pub description: &'static str,
    /// The text in the field. Change the field only when it shows other text, so that the caret
    /// and the undo history of the user stay.
    pub text: String,
    /// A button below the field, such as one that resets the text.
    pub button: Option<Button>,
}

/// A push button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    /// The button. A click gives [`SettingsInput::Pressed`].
    pub id: ControlId,
    /// What the button does, in sentence case.
    pub label: &'static str,
    /// Whether the user can click the button.
    pub is_enabled: bool,
}

/// How a note reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteTone {
    /// Plain information.
    Plain,
    /// Work is in progress.
    Busy,
    /// Something failed.
    Error,
}

/// A short line of status text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The text.
    pub text: &'static str,
    /// How the text reads.
    pub tone: NoteTone,
}

/// Something that the user did with a control of the settings window.
#[derive(Clone, PartialEq, Eq)]
pub enum SettingsInput {
    /// Turned a switch on or off.
    Toggled {
        /// The switch.
        id: ControlId,
        /// Whether the switch is on now.
        is_on: bool,
    },
    /// Picked an entry of a menu.
    Picked {
        /// The menu.
        id: ControlId,
        /// The index of the entry in the last render.
        index: usize,
    },
    /// Changed the text of a field.
    Edited {
        /// The field.
        id: ControlId,
        /// The text in the field now.
        text: String,
    },
    /// Clicked a button.
    Pressed(ControlId),
}

impl fmt::Debug for SettingsInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toggled { id, is_on } => formatter
                .debug_struct("Toggled")
                .field("id", id)
                .field("is_on", is_on)
                .finish(),
            Self::Picked { id, index } => formatter
                .debug_struct("Picked")
                .field("id", id)
                .field("index", index)
                .finish(),
            Self::Edited { id, text } => formatter
                .debug_struct("Edited")
                .field("id", id)
                .field("chars", &text.chars().count())
                .finish(),
            Self::Pressed(id) => formatter.debug_tuple("Pressed").field(id).finish(),
        }
    }
}

#[cfg(test)]
impl Page {
    /// The row whose control is `id`.
    pub(crate) fn row(&self, id: &str) -> Option<&Row> {
        self.sections
            .iter()
            .flat_map(|section| &section.rows)
            .find(|row| row.id().is_some_and(|row_id| row_id.as_str() == id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_of_an_edit_hides_the_text() {
        let input = SettingsInput::Edited {
            id: ControlId::new("system-prompt"),
            text: "Formal".to_owned(),
        };

        let debug = format!("{input:?}");

        assert_eq!(debug, r#"Edited { id: ControlId("system-prompt"), chars: 6 }"#);
    }
}
