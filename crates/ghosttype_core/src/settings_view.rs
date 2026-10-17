//! What the settings window shows, as a tree of pages, sections and rows, and what the user does
//! in it. Each platform renders each kind of row with its own controls, so a new setting needs no
//! platform code.

use std::borrow::Cow;
use std::fmt;

/// Names a control of the settings window. A control keeps its id from one render to the next,
/// and the platform gives the id back with each input of the control. Two buttons can share an id
/// when they do the same thing, such as the buttons of a model that shows in two lists.
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

impl From<String> for ControlId {
    fn from(name: String) -> Self {
        Self(Cow::Owned(name))
    }
}

/// Everything that the settings window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsView {
    /// The pages, in the order of the navigation. The first page shows when the window opens.
    pub pages: Vec<Page>,
}

/// One page of the settings window, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The title, in sentence case.
    pub title: &'static str,
    /// The picture that stands for the page in the navigation.
    pub icon: PageIcon,
    /// One or two sentences under the title about what the page is for.
    pub summary: &'static str,
    /// A search field above the sections.
    pub search: Option<SearchField>,
    /// The sections, top to bottom.
    pub sections: Vec<Section>,
}

/// The picture that stands for a page in the navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageIcon {
    /// A gear, for settings.
    Gear,
    /// A cube, for things that the user downloads.
    Cube,
    /// A text cursor, for text that the user types.
    TextCursor,
}

/// A field that searches for the text that the user submits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchField {
    /// The field. The return key in the field, and the clear button of the field, give
    /// [`SettingsInput::Submitted`].
    pub id: ControlId,
    /// The hint that the field shows while it is empty.
    pub placeholder: &'static str,
}

/// Rows that belong together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Text above the rows, in sentence case. It can hold text that the user typed.
    pub heading: Option<String>,
    /// A button before the heading that goes back to the section that this one came from.
    pub back: Option<Button>,
    /// The rows, top to bottom.
    pub rows: Vec<Row>,
    /// A line of help below the rows.
    pub footer: Option<&'static str>,
}

impl Section {
    /// A section of `rows` with no heading.
    pub(crate) fn new(rows: Vec<Row>) -> Self {
        Self {
            heading: None,
            back: None,
            rows,
            footer: None,
        }
    }
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
    /// A line of text that the user submits.
    TextField(TextField),
    /// A line of status.
    Note(Note),
    /// Text that a model wrote.
    Output(Output),
    /// One thing of a list, such as a model or a file.
    Item(Item),
    /// The rows of the section are on their way. The text tells what happens.
    Loading(&'static str),
    /// The section has no rows to show, and this tells why.
    Message(Message),
}

impl Row {
    /// The id of the control that the row holds, if it holds one.
    #[must_use]
    pub fn id(&self) -> Option<&ControlId> {
        match self {
            Self::Switch(switch) => Some(&switch.id),
            Self::Picker(picker) => Some(&picker.id),
            Self::TextArea(text_area) => Some(&text_area.id),
            Self::TextField(text_field) => Some(&text_field.id),
            Self::Note(_) | Self::Output(_) | Self::Item(_) | Self::Loading(_) | Self::Message(_) => None,
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

/// A field that edits one line of text, and a button that submits the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextField {
    /// The field. The return key in the field and a click of its button give
    /// [`SettingsInput::Submitted`].
    pub id: ControlId,
    /// Text above the field, in sentence case.
    pub label: &'static str,
    /// The hint that the field shows while it is empty.
    pub placeholder: &'static str,
    /// What the button does, in sentence case.
    pub submit_label: &'static str,
    /// Whether the button and the return key submit the text.
    pub is_submit_enabled: bool,
}

/// Text that a model wrote, and facts about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The text.
    pub text: String,
    /// Facts about the text, such as how long the model took.
    pub detail: String,
}

/// One thing of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Main text.
    pub title: String,
    /// Secondary text below the title; empty when there is none.
    pub subtitle: String,
    /// Short facts at the trailing edge, such as the file size; empty when there are none.
    pub detail: String,
    /// What the user can do with the thing.
    pub accessory: Accessory,
}

/// The control at the trailing edge of an item, which tells what the user can do with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accessory {
    /// The thing can be downloaded.
    Download(Button),
    /// The item leads to a list of its own, such as the files of a repository.
    Open(Button),
    /// The thing downloads.
    Downloading {
        /// How much of the thing arrived.
        progress: Permille,
        /// Stops the download.
        cancel: Button,
    },
    /// The download stops.
    Canceling,
    /// The thing is downloaded.
    Installed {
        /// Deletes the thing.
        delete: Button,
    },
    /// The download failed. The subtitle of the item tells why.
    Failed {
        /// Starts the download again.
        retry: Button,
    },
    /// The item cannot be used. The subtitle of the item tells why.
    Unavailable,
}

/// How far a download got, in thousandths of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Permille(pub u16);

/// Why a section has no rows to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The explanation.
    pub text: String,
    /// A button that helps, such as one that tries again.
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
    /// Submitted the text of a field.
    Submitted {
        /// The field.
        id: ControlId,
        /// The text in the field.
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
            Self::Submitted { id, text } => formatter
                .debug_struct("Submitted")
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
