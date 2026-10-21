//! The rows of the settings pages.
//!
//! It has one view type for each kind of row of the core tree, and the status note of a page.

mod app;
mod item;
mod text;

pub(super) use crate::settings::rows::app::{AppPickerRow, AppSwitchRow};
pub(super) use crate::settings::rows::item::ItemRow;
pub(super) use crate::settings::rows::text::{TextAreaRow, TextFieldRow, TextLineRow};

use ghosttype_core::settings_window::view::{self as view, ChoiceText, ControlId, Note, NoteTone, Row};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSLayoutConstraintOrientation, NSLayoutPriorityDefaultHigh, NSLayoutPriorityDefaultLow, NSPopUpButton,
    NSProgressIndicator, NSStackView, NSStackViewGravity, NSSwitch, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSEdgeInsets, NSString};

use crate::labels::title_case;
use crate::settings::Binder;
use crate::settings::controls::{
    detail_font, pop_up_button, push_button, rebuild_menu, secondary_wrapping_label, set_text,
    show_push_button, spinner, start_spinning, stop_spinning, truncating_label, wrapping_label,
};
use crate::settings::layout::{LIST_PART_SPACING, NO_INSETS, add_grouped_row, fill_width, row_texts, stack};

const MESSAGE_SPACING: CGFloat = 8.0;
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
    Picker(PickerRow),
    AppPicker(AppPickerRow),
    TextArea(TextAreaRow),
    TextLine(TextLineRow),
    TextField(TextFieldRow),
    Note(NoteView),
    Output(OutputRow),
    Item(ItemRow),
    Message(MessageRow),
}

impl RowView {
    /// Whether the views show `row`: a row of their kind, for their control.
    pub(super) fn shows(&self, row: &Row) -> bool {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.id == switch.id,
            (Self::AppSwitch(row_view), Row::AppSwitch(switch)) => row_view.id == switch.id,
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.id == picker.id,
            (Self::AppPicker(row_view), Row::AppPicker(picker)) => row_view.id == picker.id,
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.id == text_area.id,
            (Self::TextLine(row_view), Row::TextLine(text_line)) => row_view.id == text_line.id,
            (Self::TextField(row_view), Row::TextField(text_field)) => row_view.id == text_field.id,
            (Self::Note(_), Row::Note(_))
            | (Self::Output(_), Row::Output(_))
            | (Self::Item(_), Row::Item(_))
            | (Self::Message(_), Row::Loading(_) | Row::Message(_)) => true,
            _ => false,
        }
    }

    /// Shows `row`, when the views show it, see [`Self::shows`].
    pub(super) fn render(&mut self, row: &Row, binder: &mut Binder<'_>) {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.render(switch, binder),
            (Self::AppSwitch(row_view), Row::AppSwitch(switch)) => row_view.render(switch, binder),
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.render(picker, binder),
            (Self::AppPicker(row_view), Row::AppPicker(picker)) => row_view.render(picker, binder),
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.render(text_area, binder),
            (Self::TextLine(row_view), Row::TextLine(text_line)) => row_view.render(text_line),
            (Self::TextField(row_view), Row::TextField(text_field)) => row_view.render(text_field, binder),
            (Self::Note(row_view), Row::Note(note)) => row_view.render(Some(note)),
            (Self::Output(row_view), Row::Output(output)) => row_view.render(output),
            (Self::Item(row_view), Row::Item(item)) => row_view.render(item, binder),
            (Self::Message(row_view), Row::Loading(text)) => row_view.render_loading(text),
            (Self::Message(row_view), Row::Message(message)) => row_view.render_message(message, binder),
            _ => {}
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
    /// Adds a row with `control` at the bottom of `rows_stack`.
    fn add(rows_stack: &NSStackView, control: &NSView, main_thread: MainThreadMarker) -> Self {
        let label = truncating_label(&NSFont::systemFontOfSize(NSFont::systemFontSize()), main_thread);
        let description = secondary_wrapping_label(main_thread);
        description.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        description.setContentCompressionResistancePriority_forOrientation(
            NSLayoutPriorityDefaultLow,
            NSLayoutConstraintOrientation::Horizontal,
        );
        let texts = row_texts(&label, &description, main_thread);
        fill_width(&description, &texts);
        control.setContentHuggingPriority_forOrientation(
            NSLayoutPriorityDefaultHigh,
            NSLayoutConstraintOrientation::Horizontal,
        );
        add_grouped_row(rows_stack, &texts, &[control], main_thread);
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

/// A row with a switch that turns a setting on and off.
#[derive(Debug)]
pub(super) struct SwitchRow {
    setting: SettingRow,
    switch: Retained<NSSwitch>,
    id: ControlId,
}

impl SwitchRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let switch = NSSwitch::new(binder.main_thread);
        // SAFETY: the target implements the toggle action, taking the switch, and `SettingsWindow`
        // keeps the target alive as long as the switch.
        unsafe {
            switch.setTarget(Some(&binder.target.target));
            switch.setAction(Some(binder.target.toggle_action));
        }
        Self {
            setting: SettingRow::add(rows_stack, &switch, binder.main_thread),
            switch,
            id: id.clone(),
        }
    }

    fn render(&self, shown: &view::Switch, binder: &mut Binder<'_>) {
        self.setting.render(shown.label, shown.description);
        self.switch
            .setAccessibilityLabel(Some(&NSString::from_str(shown.label)));
        let state = if shown.is_on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        };
        self.switch.setState(state);
        binder.bind(&self.switch, &shown.id);
    }
}

/// A row with a pop-up menu that picks the value of a setting, and a note below its description.
/// The menu rebuilds its entries only when they change, so an open menu stays open.
#[derive(Debug)]
pub(super) struct PickerRow {
    setting: SettingRow,
    menu: Retained<NSPopUpButton>,
    titles: Vec<String>,
    note: NoteView,
    id: ControlId,
}

impl PickerRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let menu = pop_up_button(binder.target, binder.target.pick_action, binder.main_thread);
        let setting = SettingRow::add(rows_stack, &menu, binder.main_thread);
        let note = NoteView::new(binder.main_thread);
        setting.texts.addArrangedSubview(&note.container);
        fill_width(&note.container, &setting.texts);
        Self {
            setting,
            menu,
            titles: Vec::new(),
            note,
            id: id.clone(),
        }
    }

    fn render(&mut self, shown: &view::Picker, binder: &mut Binder<'_>) {
        self.setting.render(shown.label, shown.description);
        self.menu
            .setAccessibilityLabel(Some(&NSString::from_str(shown.label)));
        let titles: Vec<String> = shown
            .choices
            .iter()
            .map(|choice| match shown.choice_text {
                ChoiceText::Label => title_case(choice),
                ChoiceText::Name => choice.clone(),
            })
            .collect();
        if titles != self.titles {
            rebuild_menu(&self.menu, &titles);
            self.titles = titles;
        }
        let selected = isize::try_from(shown.selected).expect("a menu never holds isize::MAX entries");
        self.menu.selectItemAtIndex(selected);
        self.menu.setEnabled(shown.is_enabled);
        self.note.render(shown.note.as_ref());
        binder.bind(&self.menu, &shown.id);
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

/// A spinner and a line of status text.
#[derive(Debug)]
pub(super) struct NoteView {
    pub(super) container: Retained<NSStackView>,
    spinner: Retained<NSProgressIndicator>,
    text: Retained<NSTextField>,
}

impl NoteView {
    pub(super) fn new(main_thread: MainThreadMarker) -> Self {
        let spinner = spinner(main_thread);
        let text = secondary_wrapping_label(main_thread);
        text.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        let container = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            MESSAGE_SPACING,
            NO_INSETS,
            main_thread,
        );
        container.addArrangedSubview(&spinner);
        container.addArrangedSubview(&text);
        Self {
            container,
            spinner,
            text,
        }
    }

    fn render(&self, note: Option<&Note>) {
        stop_spinning(&self.spinner);
        self.container.setHidden(note.is_none());
        let Some(note) = note else {
            return;
        };
        set_text(&self.text, note.text);
        let color = match note.tone {
            NoteTone::Plain | NoteTone::Busy => NSColor::secondaryLabelColor(),
            NoteTone::Error => NSColor::systemRedColor(),
        };
        self.text.setTextColor(Some(&color));
        if note.tone == NoteTone::Busy {
            start_spinning(&self.spinner);
        }
    }
}
