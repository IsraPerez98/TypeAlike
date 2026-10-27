//! The rows of the settings pages.
//!
//! It has one view type for each kind of row of the core tree, and the status note of a page.

mod app;
mod item;
mod text;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibility as _, NSButton, NSColor, NSImageView, NSLayoutAttribute, NSLayoutConstraintOrientation,
    NSLayoutPriorityDefaultHigh, NSLayoutPriorityDefaultLow, NSProgressIndicator, NSStackView,
    NSStackViewGravity, NSSwitch, NSTextField, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSEdgeInsets, NSString};
use typealike_core::settings_window::view::{
    self as view, Access, ChoiceText, ControlId, Note, NoteTone, Row, SettingsInput,
};

use crate::labels::menu_label;
use crate::settings::controls::{
    MenuChoices, body_font, control_state, detail_font, label, push_button, secondary_wrapping_label,
    set_text, show_push_button, show_status, small_secondary_label, small_symbol_view, spinner,
    start_spinning, stop_spinning, truncating_label, warning_color, wrapping_label,
};
use crate::settings::layout::{LIST_PART_SPACING, NO_INSETS, add_grouped_row, fill_width, row_texts, stack};
use crate::settings::rows::app::{AppPickerRow, AppSwitchRow, SiteSwitchRow};
use crate::settings::rows::item::ItemRow;
use crate::settings::rows::text::{TextAreaRow, TextFieldRow, TextLineRow};
use crate::settings::{Binder, ControlAction};

const MESSAGE_SPACING: CGFloat = 8.0;
const WARNING_SYMBOL: &str = "exclamationmark.triangle.fill";
const WARNING_DESCRIPTION: &str = "Warning";
const GRANTED_SYMBOL: &str = "checkmark.circle.fill";
const MESSAGE_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 28.0,
    left: 16.0,
    bottom: 28.0,
    right: 16.0,
};

/// The views of one row.
#[derive(Debug)]
pub(super) enum RowView {
    Switch(SwitchRow),
    AppSwitch(AppSwitchRow),
    SiteSwitch(SiteSwitchRow),
    Picker(PickerRow),
    AppPicker(AppPickerRow),
    TextArea(TextAreaRow),
    TextLine(TextLineRow),
    TextField(TextFieldRow),
    Note(NoteView),
    Output(OutputRow),
    Item(ItemRow),
    Permission(PermissionRowView),
    Message(MessageRow),
}

impl RowView {
    /// Whether `row` sits in a rounded group, which it shares with the grouped rows next to it.
    pub(super) fn is_grouped(row: &Row) -> bool {
        !matches!(row, Row::TextArea(_) | Row::TextField(_) | Row::Note(_))
    }

    /// Adds the views of `row` at the bottom of `container`.
    pub(super) fn add(row: &Row, container: &NSStackView, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        match row {
            Row::Switch(switch) => Self::Switch(SwitchRow::add(container, &switch.id, binder)),
            Row::AppSwitch(switch) => Self::AppSwitch(AppSwitchRow::add(container, &switch.id, binder)),
            Row::SiteSwitch(switch) => Self::SiteSwitch(SiteSwitchRow::add(container, &switch.id, binder)),
            Row::Picker(picker) => Self::Picker(PickerRow::add(container, &picker.id, binder)),
            Row::AppPicker(picker) => Self::AppPicker(AppPickerRow::add(container, &picker.id, binder)),
            Row::TextLine(text_line) => {
                Self::TextLine(TextLineRow::add(container, &text_line.id, main_thread))
            }
            Row::Output(_) => Self::Output(OutputRow::add(container, main_thread)),
            Row::Item(_) => Self::Item(ItemRow::add(container, binder)),
            Row::Permission(_) => Self::Permission(PermissionRowView::add(container, binder)),
            Row::Loading(_) | Row::Message(_) => Self::Message(MessageRow::add(container, binder)),
            Row::TextArea(text_area) => Self::TextArea(TextAreaRow::add(container, &text_area.id, binder)),
            Row::TextField(text_field) => {
                Self::TextField(TextFieldRow::add(container, &text_field.id, binder))
            }
            Row::Note(_) => Self::Note(NoteView::add(container, main_thread)),
        }
    }

    /// Whether the views show `row`: a row of their kind, for their control.
    pub(super) fn shows(&self, row: &Row) -> bool {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.id == switch.id,
            (Self::AppSwitch(row_view), Row::AppSwitch(switch)) => row_view.id == switch.id,
            (Self::SiteSwitch(row_view), Row::SiteSwitch(switch)) => row_view.id == switch.id,
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.id == picker.id,
            (Self::AppPicker(row_view), Row::AppPicker(picker)) => row_view.id == picker.id,
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.id == text_area.id,
            (Self::TextLine(row_view), Row::TextLine(text_line)) => row_view.id == text_line.id,
            (Self::TextField(row_view), Row::TextField(text_field)) => row_view.id == text_field.id,
            (Self::Note(_), Row::Note(_))
            | (Self::Output(_), Row::Output(_))
            | (Self::Item(_), Row::Item(_))
            | (Self::Permission(_), Row::Permission(_))
            | (Self::Message(_), Row::Loading(_) | Row::Message(_)) => true,
            _ => false,
        }
    }

    /// Shows `row`, when the views show it, see [`Self::shows`].
    pub(super) fn render(&mut self, row: &Row, binder: &mut Binder<'_>) {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.render(switch, binder),
            (Self::AppSwitch(row_view), Row::AppSwitch(switch)) => row_view.render(switch, binder),
            (Self::SiteSwitch(row_view), Row::SiteSwitch(switch)) => row_view.render(switch, binder),
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.render(picker, binder),
            (Self::AppPicker(row_view), Row::AppPicker(picker)) => row_view.render(picker, binder),
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.render(text_area, binder),
            (Self::TextLine(row_view), Row::TextLine(text_line)) => row_view.render(text_line),
            (Self::TextField(row_view), Row::TextField(text_field)) => row_view.render(text_field, binder),
            (Self::Note(row_view), Row::Note(note)) => row_view.render(Some(note)),
            (Self::Output(row_view), Row::Output(output)) => row_view.render(output),
            (Self::Item(row_view), Row::Item(item)) => row_view.render(item, binder),
            (Self::Permission(row_view), Row::Permission(permission)) => row_view.render(permission, binder),
            (Self::Message(row_view), Row::Loading(text)) => row_view.render_loading(text),
            (Self::Message(row_view), Row::Message(message)) => row_view.render_message(message, binder),
            _ => {}
        }
    }

    /// The input of a change of the text in `view`, when it is the text area or the line of text
    /// of these views.
    pub(super) fn edited(&self, view: &AnyObject) -> Option<SettingsInput> {
        match self {
            Self::TextArea(text_area) => text_area.edited(view),
            Self::TextLine(text_line) => text_line.edited(view),
            _ => None,
        }
    }

    /// The text in the text field `id`, when it is the text field of these views.
    pub(super) fn field_text(&self, id: &ControlId) -> Option<String> {
        match self {
            Self::TextField(text_field) => text_field.field_text(id),
            _ => None,
        }
    }
}

/// A row of a group: a label over a description, and the control that changes the setting.
#[derive(Debug)]
struct SettingRow {
    texts: Retained<NSStackView>,
    label: Retained<NSTextField>,
    description: Retained<NSTextField>,
}

impl SettingRow {
    /// Adds a row with `controls` at the bottom of `rows_stack`.
    fn add(rows_stack: &NSStackView, controls: &[&NSView], main_thread: MainThreadMarker) -> Self {
        let label = truncating_label(&body_font(), main_thread);
        let description = small_secondary_label(main_thread);
        let texts = row_texts(&label, &description, main_thread);
        fill_width(&description, &texts);
        for control in controls {
            control.setContentHuggingPriority_forOrientation(
                NSLayoutPriorityDefaultHigh,
                NSLayoutConstraintOrientation::Horizontal,
            );
        }
        add_grouped_row(rows_stack, &texts, controls, main_thread);
        Self {
            texts,
            label,
            description,
        }
    }

    fn render(&self, label: &str, description: &str) {
        set_text(&self.label, label);
        set_text(&self.description, description);
    }
}

/// A row with a switch that turns a setting on and off, and a note below its description.
#[derive(Debug)]
pub(super) struct SwitchRow {
    setting: SettingRow,
    switch: Retained<NSSwitch>,
    note: NoteView,
    id: ControlId,
}

impl SwitchRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let switch = NSSwitch::new(binder.main_thread);
        binder.target.connect(&switch, ControlAction::Toggle);
        let setting = SettingRow::add(rows_stack, &[&switch], binder.main_thread);
        let note = NoteView::add(&setting.texts, binder.main_thread);
        Self {
            setting,
            switch,
            note,
            id: id.clone(),
        }
    }

    fn render(&self, shown: &view::Switch, binder: &mut Binder<'_>) {
        self.setting.render(&shown.label, &shown.description);
        self.switch
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.label)));
        self.switch.setState(control_state(shown.is_on));
        self.note.render(shown.note.as_ref());
        binder.bind(&self.switch, &shown.id);
    }
}

/// A row with a pop-up menu that picks the value of a setting, and a note below its description.
#[derive(Debug)]
pub(super) struct PickerRow {
    setting: SettingRow,
    choices: MenuChoices,
    note: NoteView,
    id: ControlId,
}

impl PickerRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let choices = MenuChoices::new(binder.target, binder.main_thread);
        let setting = SettingRow::add(rows_stack, &[&choices.menu], binder.main_thread);
        let note = NoteView::add(&setting.texts, binder.main_thread);
        Self {
            setting,
            choices,
            note,
            id: id.clone(),
        }
    }

    fn render(&mut self, shown: &view::Picker, binder: &mut Binder<'_>) {
        self.setting.render(&shown.label, &shown.description);
        self.choices
            .menu
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.label)));
        let titles: Vec<String> = shown
            .choices
            .iter()
            .map(|choice| match shown.choice_text {
                ChoiceText::Label => menu_label(choice, binder.language),
                ChoiceText::Name => choice.clone(),
            })
            .collect();
        self.choices.render(titles, shown.selected);
        self.choices.menu.setEnabled(shown.is_enabled);
        self.note.render(shown.note.as_ref());
        binder.bind(&self.choices.menu, &shown.id);
    }
}

/// A row with the label and the reason of a permission, and at its trailing edge a check mark with
/// the state, or the button that asks for the permission.
#[derive(Debug)]
pub(super) struct PermissionRowView {
    setting: SettingRow,
    check_mark: Retained<NSImageView>,
    state: Retained<NSTextField>,
    button: Retained<NSButton>,
}

impl PermissionRowView {
    pub(super) fn add(rows_stack: &NSStackView, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let check_mark = NSImageView::new(main_thread);
        let state = label(&body_font(), main_thread);
        state.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let button = push_button(binder.target, main_thread);
        let setting = SettingRow::add(rows_stack, &[&check_mark, &state, &button], main_thread);
        Self {
            setting,
            check_mark,
            state,
            button,
        }
    }

    fn render(&self, shown: &view::PermissionRow, binder: &mut Binder<'_>) {
        self.setting.render(&shown.label, &shown.description);
        let is_granted = matches!(shown.access, Access::Granted(_));
        self.check_mark.setHidden(!is_granted);
        self.state.setHidden(!is_granted);
        self.button.setHidden(is_granted);
        match &shown.access {
            Access::Granted(text) => {
                show_status(
                    &self.check_mark,
                    GRANTED_SYMBOL,
                    text,
                    &NSColor::systemGreenColor(),
                );
                set_text(&self.state, text);
            }
            Access::Missing(button) => show_push_button(&self.button, button, Some(&shown.label), binder),
        }
    }
}

/// A row of a group with the text that a model wrote, and the facts about it below the text.
#[derive(Debug)]
pub(super) struct OutputRow {
    text: Retained<NSTextField>,
    detail: Retained<NSTextField>,
}

impl OutputRow {
    pub(super) fn add(rows_stack: &NSStackView, main_thread: MainThreadMarker) -> Self {
        let text = wrapping_label(main_thread);
        let detail = secondary_wrapping_label(main_thread);
        detail.setFont(Some(&detail_font()));
        let texts = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        texts.addArrangedSubview(&text);
        texts.addArrangedSubview(&detail);
        texts.setContentHuggingPriority_forOrientation(
            NSLayoutPriorityDefaultLow,
            NSLayoutConstraintOrientation::Horizontal,
        );
        fill_width(&text, &texts);
        fill_width(&detail, &texts);
        add_grouped_row(rows_stack, &texts, &[], main_thread);
        Self { text, detail }
    }

    fn render(&self, shown: &view::Output) {
        set_text(&self.text, &shown.text);
        set_text(&self.detail, &shown.detail);
    }
}

/// What shows in a group instead of items: a spinner or a message, with an optional button.
#[derive(Debug)]
pub(super) struct MessageRow {
    spinner: Retained<NSProgressIndicator>,
    text: Retained<NSTextField>,
    button: Retained<NSButton>,
}

impl MessageRow {
    pub(super) fn add(rows_stack: &NSStackView, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let spinner = spinner(main_thread);
        let text = wrapping_label(main_thread);
        text.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let button = push_button(binder.target, main_thread);
        let container = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            MESSAGE_SPACING,
            MESSAGE_INSETS,
            main_thread,
        );
        let parts: [&NSView; 3] = [&spinner, &text, &button];
        for part in parts {
            container.addView_inGravity(part, NSStackViewGravity::Center);
        }
        rows_stack.addArrangedSubview(&container);
        fill_width(&container, rows_stack);
        Self {
            spinner,
            text,
            button,
        }
    }

    fn render_loading(&self, text: &str) {
        set_text(&self.text, text);
        self.button.setHidden(true);
        start_spinning(&self.spinner);
    }

    fn render_message(&self, message: &view::Message, binder: &mut Binder<'_>) {
        set_text(&self.text, &message.text);
        stop_spinning(&self.spinner);
        match &message.button {
            Some(button) => show_push_button(&self.button, button, None, binder),
            None => self.button.setHidden(true),
        }
    }
}

/// A spinner or a warning sign, and a line of status text.
#[derive(Debug)]
pub(super) struct NoteView {
    container: Retained<NSStackView>,
    spinner: Retained<NSProgressIndicator>,
    warning_sign: Retained<NSImageView>,
    text: Retained<NSTextField>,
}

impl NoteView {
    /// Adds the note at the bottom of the vertical stack `parent`.
    fn add(parent: &NSStackView, main_thread: MainThreadMarker) -> Self {
        let spinner = spinner(main_thread);
        let warning_sign = small_symbol_view(main_thread);
        let text = small_secondary_label(main_thread);
        let container = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            MESSAGE_SPACING,
            NO_INSETS,
            main_thread,
        );
        container.setAlignment(NSLayoutAttribute::Top);
        container.addArrangedSubview(&spinner);
        container.addArrangedSubview(&warning_sign);
        container.addArrangedSubview(&text);
        text.trailingAnchor()
            .constraintEqualToAnchor(&container.trailingAnchor())
            .setActive(true);
        parent.addArrangedSubview(&container);
        fill_width(&container, parent);
        Self {
            container,
            spinner,
            warning_sign,
            text,
        }
    }

    fn render(&self, note: Option<&Note>) {
        stop_spinning(&self.spinner);
        self.warning_sign.setHidden(true);
        self.container.setHidden(note.is_none());
        let Some(note) = note else {
            return;
        };
        set_text(&self.text, &note.text);
        let color = match note.tone {
            NoteTone::Plain | NoteTone::Busy => NSColor::secondaryLabelColor(),
            NoteTone::Warning => warning_color(),
            NoteTone::Error => NSColor::systemRedColor(),
        };
        self.text.setTextColor(Some(&color));
        match note.tone {
            NoteTone::Busy => start_spinning(&self.spinner),
            NoteTone::Warning => show_status(&self.warning_sign, WARNING_SYMBOL, WARNING_DESCRIPTION, &color),
            NoteTone::Plain | NoteTone::Error => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use typealike_core::settings_window::view::{
        Message, Output, RowIcon, Switch, TextArea, TextField, TextLine,
    };

    use super::*;

    const ID: ControlId = ControlId::new("setting");
    const ICON: RowIcon = RowIcon::Note;

    #[test]
    fn rows_of_settings_and_lists_share_a_rounded_group() {
        let rows = [
            Row::Switch(Switch {
                id: ID,
                icon: ICON,
                label: "Label".to_owned(),
                description: String::new(),
                is_on: true,
                note: None,
            }),
            Row::TextLine(TextLine {
                id: ID,
                icon: ICON,
                label: "Label".to_owned(),
                description: String::new(),
                text: String::new(),
            }),
            Row::Output(Output {
                text: String::new(),
                detail: String::new(),
            }),
            Row::Loading("Loading".to_owned()),
            Row::Message(Message {
                text: String::new(),
                button: None,
            }),
        ];

        let grouped = rows.map(|row| RowView::is_grouped(&row));

        assert_eq!(grouped, [true; 5]);
    }

    #[test]
    fn text_areas_text_fields_and_notes_stand_outside_the_rounded_groups() {
        let rows = [
            Row::TextArea(TextArea {
                id: ID,
                icon: ICON,
                label: "Label".to_owned(),
                description: String::new(),
                text: String::new(),
                button: None,
            }),
            Row::TextField(TextField {
                id: ID,
                icon: ICON,
                label: "Label".to_owned(),
                placeholder: String::new(),
                submit_label: "Submit".to_owned(),
                is_submit_enabled: true,
            }),
            Row::Note(Note {
                text: "Note".to_owned(),
                tone: NoteTone::Plain,
            }),
        ];

        let grouped = rows.map(|row| RowView::is_grouped(&row));

        assert_eq!(grouped, [false; 3]);
    }
}
