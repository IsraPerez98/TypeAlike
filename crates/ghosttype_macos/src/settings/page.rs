//! A settings page, rendered from its core tree: the summary, then each section under its
//! heading. Switches, menus and model output sit in rounded groups like the grouped lists of
//! System Settings. Text fields and status notes sit between the groups.

use std::ptr;

use ghosttype_core::settings_view::{self as view, ChoiceText, ControlId, Row, SettingsInput};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2_app_kit::{
    NSAccessibility as _, NSBorderType, NSButton, NSControl, NSControlStateValueOff, NSControlStateValueOn,
    NSFont, NSFontWeightRegular, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultHigh,
    NSLayoutPriorityDefaultLow, NSLineBreakMode, NSPopUpButton, NSScrollView, NSStackView,
    NSStackViewGravity, NSSwitch, NSTextField, NSTextView, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSSize, NSString};

use super::{
    ActionTarget, CONTENT_INSETS, DETAIL_FONT_SIZE, GroupedRow, LIST_PART_SPACING, NO_INSETS, NoteView,
    SECTION_SPACING, SUMMARY_FONT_SIZE, fill_width, label, pop_up_button, rebuild_menu, rounded_group,
    row_texts, scroll_view, secondary_wrapping_label, set_text, stack, truncating_label, wrapping_label,
};
use crate::labels::title_case;

const TEXT_AREA_HEIGHT: CGFloat = 120.0;
const TEXT_AREA_INSETS: NSSize = NSSize::new(4.0, 6.0);
const TEXT_FIELD_HEIGHT: CGFloat = 88.0;

/// Creates the controls of the settings pages, which report to the action target, and tags each
/// control with the index of its id.
pub(super) struct Binder<'a> {
    pub(super) target: &'a ActionTarget,
    pub(super) ids: &'a mut Vec<ControlId>,
    pub(super) main_thread: MainThreadMarker,
}

impl Binder<'_> {
    /// Tags `control` with `id`, so that its action reads `id` back.
    fn bind(&mut self, control: &NSControl, id: &ControlId) {
        let tag = isize::try_from(self.ids.len()).expect("a window never holds isize::MAX controls");
        self.ids.push(id.clone());
        control.setTag(tag);
    }
}

/// The views of a settings page, kept to update them on each render.
#[derive(Debug)]
pub(super) struct PageView {
    pub(super) root: Retained<NSScrollView>,
    content: Retained<NSStackView>,
    summary: Retained<NSTextField>,
    sections: Vec<SectionView>,
}

impl PageView {
    pub(super) fn new(main_thread: MainThreadMarker) -> Self {
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            SECTION_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        let summary = secondary_wrapping_label(main_thread);
        summary.setFont(Some(&NSFont::systemFontOfSize(SUMMARY_FONT_SIZE)));
        content.addArrangedSubview(&summary);
        fill_width(&summary, &content);
        Self {
            root: scroll_view(&content, main_thread),
            content,
            summary,
            sections: Vec::new(),
        }
    }

    /// Shows `page`. A section keeps its views while its rows keep their kinds and controls, so
    /// that an open menu, the focus and the typed text stay.
    pub(super) fn render(&mut self, page: &view::Page, binder: &mut Binder<'_>) {
        set_text(&self.summary, page.summary);
        let kept = page.sections.len().min(self.sections.len());
        for section in self.sections.drain(kept..) {
            section.container.removeFromSuperview();
        }
        while self.sections.len() < page.sections.len() {
            self.sections
                .push(SectionView::new(&self.content, binder.main_thread));
        }
        for (section, shown) in self.sections.iter_mut().zip(&page.sections) {
            section.render(shown, binder);
        }
    }

    /// The input of a change of the text in `text_view`, when it is a text area of the page.
    pub(super) fn edited(&self, text_view: &AnyObject) -> Option<SettingsInput> {
        self.rows().find_map(|row| match row {
            RowView::TextArea(text_area) => text_area.edited(text_view),
            _ => None,
        })
    }

    /// The text in the text field `id`, when it is on the page.
    pub(super) fn field_text(&self, id: &ControlId) -> Option<String> {
        self.rows().find_map(|row| match row {
            RowView::TextField(text_field) if text_field.id == *id => {
                Some(text_field.field.stringValue().to_string())
            }
            _ => None,
        })
    }

    fn rows(&self) -> impl Iterator<Item = &RowView> {
        self.sections.iter().flat_map(|section| &section.rows)
    }
}

/// The views of one section: its heading, then its rows.
#[derive(Debug)]
struct SectionView {
    container: Retained<NSStackView>,
    heading: Retained<NSTextField>,
    rows_container: Retained<NSStackView>,
    rows: Vec<RowView>,
}

impl SectionView {
    fn new(content: &NSStackView, main_thread: MainThreadMarker) -> Self {
        let container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        let heading = heading(main_thread);
        let rows_container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        container.addArrangedSubview(&heading);
        container.addArrangedSubview(&rows_container);
        fill_width(&rows_container, &container);
        content.addArrangedSubview(&container);
        fill_width(&container, content);
        Self {
            container,
            heading,
            rows_container,
            rows: Vec::new(),
        }
    }

    fn render(&mut self, section: &view::Section, binder: &mut Binder<'_>) {
        set_text(&self.heading, section.heading.as_deref().unwrap_or_default());
        let has_same_rows = self.rows.len() == section.rows.len()
            && self
                .rows
                .iter()
                .zip(&section.rows)
                .all(|(row_view, row)| row_view.shows(row));
        if !has_same_rows {
            self.rebuild(&section.rows, binder);
        }
        for (row_view, row) in self.rows.iter_mut().zip(&section.rows) {
            row_view.render(row, binder);
        }
    }

    /// Makes new views for `rows`. Grouped rows that follow each other share a rounded group.
    fn rebuild(&mut self, rows: &[Row], binder: &Binder<'_>) {
        for part in self.rows_container.arrangedSubviews().to_vec() {
            part.removeFromSuperview();
        }
        self.rows.clear();
        let mut group = None;
        for row in rows {
            let row_view = match row {
                Row::Switch(switch) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::Switch(SwitchRow::add(rows_stack, &switch.id, binder))
                }
                Row::Picker(picker) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::Picker(PickerRow::add(rows_stack, &picker.id, binder))
                }
                Row::Output(_) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::Output(OutputRow::add(rows_stack, binder.main_thread))
                }
                Row::TextArea(text_area) => {
                    group = None;
                    RowView::TextArea(TextAreaRow::add(&self.rows_container, &text_area.id, binder))
                }
                Row::TextField(text_field) => {
                    group = None;
                    RowView::TextField(TextFieldRow::add(&self.rows_container, &text_field.id, binder))
                }
                Row::Note(_) => {
                    group = None;
                    let note = NoteView::new(binder.main_thread);
                    self.rows_container.addArrangedSubview(&note.container);
                    fill_width(&note.container, &self.rows_container);
                    RowView::Note(note)
                }
            };
            self.rows.push(row_view);
        }
    }

    /// Adds a rounded group below the rows, and returns the stack of its rows.
    fn add_group(&self, main_thread: MainThreadMarker) -> Retained<NSStackView> {
        let rows_stack = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            0.0,
            NO_INSETS,
            main_thread,
        );
        let group = rounded_group(&rows_stack, main_thread);
        self.rows_container.addArrangedSubview(&group);
        fill_width(&group, &self.rows_container);
        rows_stack
    }
}

/// The views of one row.
#[derive(Debug)]
enum RowView {
    Switch(SwitchRow),
    Picker(PickerRow),
    TextArea(TextAreaRow),
    TextField(TextFieldRow),
    Note(NoteView),
    Output(OutputRow),
}

impl RowView {
    /// Whether the views show `row`: a row of their kind, for their control.
    fn shows(&self, row: &Row) -> bool {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.id == switch.id,
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.id == picker.id,
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.id == text_area.id,
            (Self::TextField(row_view), Row::TextField(text_field)) => row_view.id == text_field.id,
            (Self::Note(_), Row::Note(_)) | (Self::Output(_), Row::Output(_)) => true,
            _ => false,
        }
    }

    /// Shows `row`, when the views show it, see [`Self::shows`].
    fn render(&mut self, row: &Row, binder: &mut Binder<'_>) {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.render(switch, binder),
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.render(picker, binder),
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.render(text_area, binder),
            (Self::TextField(row_view), Row::TextField(text_field)) => row_view.render(text_field, binder),
            (Self::Note(row_view), Row::Note(note)) => row_view.render(Some(note)),
            (Self::Output(row_view), Row::Output(output)) => row_view.render(output),
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
struct SwitchRow {
    setting: SettingRow,
    switch: Retained<NSSwitch>,
    id: ControlId,
}

impl SwitchRow {
    fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
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
struct PickerRow {
    setting: SettingRow,
    menu: Retained<NSPopUpButton>,
    titles: Vec<String>,
    note: NoteView,
    id: ControlId,
}

impl PickerRow {
    fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
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
struct OutputRow {
    text: Retained<NSTextField>,
    detail: Retained<NSTextField>,
}

impl OutputRow {
    fn add(rows_stack: &NSStackView, main_thread: MainThreadMarker) -> Self {
        let text = wrapping_label(main_thread);
        let detail = secondary_wrapping_label(main_thread);
        // SAFETY: reads an AppKit constant that is initialized before any code runs.
        let regular_weight = unsafe { NSFontWeightRegular };
        detail.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
            DETAIL_FONT_SIZE,
            regular_weight,
        )));
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

/// A text area under its heading and description, with its button below it.
#[derive(Debug)]
struct TextAreaRow {
    heading: Retained<NSTextField>,
    description: Retained<NSTextField>,
    text_view: Retained<NSTextView>,
    button_row: Retained<NSStackView>,
    button: Retained<NSButton>,
    id: ControlId,
}

impl TextAreaRow {
    /// Adds the row at the bottom of `container`.
    fn add(container: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let heading = heading(main_thread);
        let description = secondary_wrapping_label(main_thread);
        description.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
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

    fn render(&self, shown: &view::TextArea, binder: &mut Binder<'_>) {
        set_text(&self.heading, shown.label);
        set_text(&self.description, shown.description);
        self.text_view
            .setAccessibilityLabel(Some(&NSString::from_str(shown.label)));
        if self.text_view.string().to_string() != shown.text {
            self.text_view.setString(&NSString::from_str(&shown.text));
        }
        self.button_row.setHidden(shown.button.is_none());
        if let Some(button) = &shown.button {
            self.button
                .setTitle(&NSString::from_str(&title_case(button.label)));
            self.button.setEnabled(button.is_enabled);
            binder.bind(&self.button, &button.id);
        }
    }

    /// The input of a change of the text in `text_view`, when it is this text area.
    fn edited(&self, text_view: &AnyObject) -> Option<SettingsInput> {
        let own_text_view: &AnyObject = &self.text_view;
        ptr::eq(own_text_view, text_view).then(|| SettingsInput::Edited {
            id: self.id.clone(),
            text: self.text_view.string().to_string(),
        })
    }
}

/// A text field under its heading, with the button that submits its text below it. The return
/// key in the field submits the text too.
#[derive(Debug)]
struct TextFieldRow {
    heading: Retained<NSTextField>,
    field: Retained<NSTextField>,
    button: Retained<NSButton>,
    id: ControlId,
}

impl TextFieldRow {
    /// Adds the row at the bottom of `container`.
    fn add(container: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let heading = heading(main_thread);
        let field = NSTextField::textFieldWithString(&NSString::new(), main_thread);
        field.setUsesSingleLineMode(false);
        field.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        field
            .heightAnchor()
            .constraintEqualToConstant(TEXT_FIELD_HEIGHT)
            .setActive(true);
        // SAFETY: the target implements the submit action, taking the field, and `SettingsWindow`
        // keeps the target alive as long as the field.
        unsafe {
            field.setTarget(Some(&binder.target.target));
            field.setAction(Some(binder.target.submit_action));
        }
        let button = button(binder.target, binder.target.submit_action, main_thread);
        let button_row = trailing_row(&button, main_thread);
        let parts: [&NSView; 3] = [&heading, &field, &button_row];
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

    fn render(&self, shown: &view::TextField, binder: &mut Binder<'_>) {
        set_text(&self.heading, shown.label);
        self.field
            .setPlaceholderString(Some(&NSString::from_str(shown.placeholder)));
        self.field
            .setAccessibilityLabel(Some(&NSString::from_str(shown.label)));
        self.button
            .setTitle(&NSString::from_str(&title_case(shown.submit_label)));
        self.button.setEnabled(shown.is_submit_enabled);
        binder.bind(&self.field, &shown.id);
        binder.bind(&self.button, &shown.id);
    }
}

/// Adds a row of `texts`, then `controls`, at the bottom of the group `rows_stack`. The first row
/// of a group has no separator above it.
fn add_grouped_row(
    rows_stack: &NSStackView,
    texts: &NSView,
    controls: &[&NSView],
    main_thread: MainThreadMarker,
) {
    let row = GroupedRow::add(rows_stack, texts, controls, main_thread);
    row.separator
        .setHidden(rows_stack.arrangedSubviews().count() == 1);
}

/// Returns a push button that sends `action` to the target of `target`.
fn button(target: &ActionTarget, action: Sel, main_thread: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: the target implements `action`, taking the button, and `SettingsWindow` keeps the
    // target alive as long as the button.
    unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::new(),
            Some(&target.target),
            Some(action),
            main_thread,
        )
    }
}

/// Returns a row that holds `view` at its trailing edge.
fn trailing_row(view: &NSView, main_thread: MainThreadMarker) -> Retained<NSStackView> {
    let row = stack(
        NSUserInterfaceLayoutOrientation::Horizontal,
        0.0,
        NO_INSETS,
        main_thread,
    );
    row.addView_inGravity(view, NSStackViewGravity::Trailing);
    row
}

/// Returns a bold label that names what follows it.
fn heading(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    label(
        &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
        main_thread,
    )
}

/// Returns a plain text view with a bezel, in the scroll view that holds it.
fn text_area(main_thread: MainThreadMarker) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let text_scroll_view = NSTextView::scrollableTextView(main_thread);
    text_scroll_view.setBorderType(NSBorderType::BezelBorder);
    text_scroll_view
        .heightAnchor()
        .constraintEqualToConstant(TEXT_AREA_HEIGHT)
        .setActive(true);
    let text_view = text_scroll_view
        .documentView()
        .and_then(|view| view.downcast::<NSTextView>().ok())
        .expect("a scrollable text view holds a text view");
    text_view.setRichText(false);
    text_view.setAllowsUndo(true);
    text_view.setFont(Some(&NSFont::systemFontOfSize(NSFont::systemFontSize())));
    text_view.setTextContainerInset(TEXT_AREA_INSETS);
    (text_scroll_view, text_view)
}
