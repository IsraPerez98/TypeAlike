//! The rows of the settings pages that edit text: a line of text, a text area and a text field.

use std::ptr;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibility as _, NSButton, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultHigh,
    NSLayoutPriorityDefaultLow, NSResponder, NSStackView, NSStackViewDistribution, NSTextField, NSTextView,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::NSString;
use typealike_core::settings_window::view::{self as view, ControlId, SettingsInput};

use crate::labels::menu_label;
use crate::settings::controls::{button, heading, set_text, small_secondary_label, text_area};
use crate::settings::layout::{ACCESSORY_SPACING, NO_INSETS, fill_width, stack, trailing_row};
use crate::settings::rows::SettingRow;
use crate::settings::{Binder, ControlAction};

const TEXT_LINE_WIDTH: CGFloat = 200.0;

/// A row of a group with a field that edits one line of text.
#[derive(Debug)]
pub(in crate::settings) struct TextLineRow {
    setting: SettingRow,
    field: Retained<NSTextField>,
    pub(super) id: ControlId,
}

impl TextLineRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, main_thread: MainThreadMarker) -> Self {
        let field = NSTextField::textFieldWithString(&NSString::new(), main_thread);
        field.setUsesSingleLineMode(true);
        field
            .widthAnchor()
            .constraintEqualToConstant(TEXT_LINE_WIDTH)
            .setActive(true);
        Self {
            setting: SettingRow::add(rows_stack, &[&field], main_thread),
            field,
            id: id.clone(),
        }
    }

    pub(super) fn render(&self, shown: &view::TextLine) {
        self.setting.render(&shown.label, &shown.description);
        self.field
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.label)));
        let is_user_editing = self.field.currentEditor().is_some();
        if !is_user_editing && self.field.stringValue().to_string() != shown.text {
            self.field.setStringValue(&NSString::from_str(&shown.text));
        }
    }

    /// The input of a change of the text in `field`, when it is the field of this row.
    pub(super) fn edited(&self, field: &AnyObject) -> Option<SettingsInput> {
        let own_field: &AnyObject = &self.field;
        ptr::eq(own_field, field).then(|| SettingsInput::Edited {
            id: self.id.clone(),
            text: self.field.stringValue().to_string(),
        })
    }
}

/// A text area under its heading and description, with its button below it.
#[derive(Debug)]
pub(in crate::settings) struct TextAreaRow {
    heading: Retained<NSTextField>,
    description: Retained<NSTextField>,
    text_view: Retained<NSTextView>,
    button_row: Retained<NSStackView>,
    button: Retained<NSButton>,
    pub(super) id: ControlId,
}

impl TextAreaRow {
    /// Adds the row at the bottom of `container`.
    pub(super) fn add(container: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let heading = heading(main_thread);
        let description = small_secondary_label(main_thread);
        let (text_scroll_view, text_view) = text_area(main_thread);
        let button = button(binder.target, binder.target.press_action, main_thread);
        let button_row = trailing_row(&button, main_thread);
        let parts: [&NSView; 4] = [&heading, &description, &text_scroll_view, &button_row];
        for part in parts {
            container.addArrangedSubview(part);
            fill_width(part, container);
        }
        Self {
            heading,
            description,
            text_view,
            button_row,
            button,
            id: id.clone(),
        }
    }

    pub(super) fn render(&self, shown: &view::TextArea, binder: &mut Binder<'_>) {
        set_text(&self.heading, &shown.label);
        set_text(&self.description, &shown.description);
        self.text_view
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.label)));
        if !self.is_user_editing() && self.text_view.string().to_string() != shown.text {
            self.text_view.setString(&NSString::from_str(&shown.text));
        }
        self.button_row.setHidden(shown.button.is_none());
        if let Some(button) = &shown.button {
            self.button
                .setTitle(&NSString::from_str(&menu_label(&button.label, binder.language)));
            self.button.setEnabled(button.is_enabled);
            binder.bind(&self.button, &button.id);
        }
    }

    /// Whether the user edits the text view now: it has the keyboard focus of its window. A render
    /// then keeps the text of the text view, so that it never moves the caret or removes the text
    /// that the user types.
    fn is_user_editing(&self) -> bool {
        let text_view: &NSResponder = &self.text_view;
        self.text_view
            .window()
            .and_then(|window| window.firstResponder())
            .is_some_and(|responder| ptr::eq(Retained::as_ptr(&responder), text_view))
    }

    /// The input of a change of the text in `text_view`, when it is this text area.
    pub(super) fn edited(&self, text_view: &AnyObject) -> Option<SettingsInput> {
        let own_text_view: &AnyObject = &self.text_view;
        ptr::eq(own_text_view, text_view).then(|| SettingsInput::Edited {
            id: self.id.clone(),
            text: self.text_view.string().to_string(),
        })
    }
}

/// A line of text under its heading, with the button that submits its text at its trailing edge.
/// The return key in the field submits the text too.
#[derive(Debug)]
pub(in crate::settings) struct TextFieldRow {
    heading: Retained<NSTextField>,
    field: Retained<NSTextField>,
    button: Retained<NSButton>,
    pub(super) id: ControlId,
}

impl TextFieldRow {
    /// Adds the row at the bottom of `container`.
    pub(super) fn add(container: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let heading = heading(main_thread);
        let field = NSTextField::textFieldWithString(&NSString::new(), main_thread);
        field.setUsesSingleLineMode(true);
        field.setContentHuggingPriority_forOrientation(
            NSLayoutPriorityDefaultLow,
            NSLayoutConstraintOrientation::Horizontal,
        );
        binder.target.connect(&field, ControlAction::Submit);
        let button = button(binder.target, binder.target.submit_action, main_thread);
        button.setContentHuggingPriority_forOrientation(
            NSLayoutPriorityDefaultHigh,
            NSLayoutConstraintOrientation::Horizontal,
        );
        let field_row = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            ACCESSORY_SPACING,
            NO_INSETS,
            main_thread,
        );
        field_row.setDistribution(NSStackViewDistribution::Fill);
        field_row.addArrangedSubview(&field);
        field_row.addArrangedSubview(&button);
        let parts: [&NSView; 2] = [&heading, &field_row];
        for part in parts {
            container.addArrangedSubview(part);
            fill_width(part, container);
        }
        Self {
            heading,
            field,
            button,
            id: id.clone(),
        }
    }

    pub(super) fn render(&self, shown: &view::TextField, binder: &mut Binder<'_>) {
        set_text(&self.heading, &shown.label);
        self.field
            .setPlaceholderString(Some(&NSString::from_str(&shown.placeholder)));
        self.field
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.label)));
        self.button.setTitle(&NSString::from_str(&menu_label(
            &shown.submit_label,
            binder.language,
        )));
        self.button.setEnabled(shown.is_submit_enabled);
        binder.bind(&self.field, &shown.id);
        binder.bind(&self.button, &shown.id);
    }

    /// The text in the field, when `id` is the id of this row.
    pub(super) fn field_text(&self, id: &ControlId) -> Option<String> {
        (self.id == *id).then(|| self.field.stringValue().to_string())
    }
}
