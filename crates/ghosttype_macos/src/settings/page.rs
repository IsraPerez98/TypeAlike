//! A settings page, rendered from its core tree: the summary and the search field, then each
//! section under its heading. Switches, menus, items and model output sit in rounded groups like
//! the grouped lists of System Settings. Text fields and status notes sit between the groups.
//!
//! A page with a search field pins the summary and the search field above the sections, which
//! scroll.

use std::ptr;

use ghosttype_core::settings_view::{self as view, Accessory, ChoiceText, ControlId, Row, SettingsInput};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2_app_kit::{
    NSAccessibility as _, NSBorderType, NSButton, NSCellImagePosition, NSColor, NSControl, NSControlSize,
    NSControlStateValueOff, NSControlStateValueOn, NSFont, NSFontWeightRegular, NSImage, NSImageView,
    NSLayoutConstraint, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultHigh,
    NSLayoutPriorityDefaultLow, NSLineBreakMode, NSPopUpButton, NSProgressIndicator, NSScrollView,
    NSSearchField, NSStackView, NSStackViewDistribution, NSStackViewGravity, NSSwitch, NSTextField,
    NSTextView, NSUserInterfaceLayoutOrientation, NSView, NSWorkspace,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSEdgeInsets, NSSize, NSString};

use super::{
    ActionTarget, CONTENT_INSETS, DETAIL_FONT_SIZE, LIST_PART_SPACING, MESSAGE_SPACING, NO_INSETS, NoteView,
    SECTION_SPACING, SUMMARY_FONT_SIZE, add_grouped_row, fill_width, label, pop_up_button, rebuild_menu,
    rounded_group, row_texts, scroll_view, secondary_wrapping_label, separator, set_text, show_status,
    spinner, stack, start_spinning, stop_spinning, symbol, truncating_label, wrapping_label,
};
use crate::labels::title_case;

const TEXT_AREA_HEIGHT: CGFloat = 120.0;
const TEXT_AREA_INSETS: NSSize = NSSize::new(4.0, 6.0);
const TEXT_FIELD_HEIGHT: CGFloat = 88.0;
const HEADER_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 20.0,
    left: 24.0,
    bottom: 16.0,
    right: 24.0,
};
const MESSAGE_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 28.0,
    left: 16.0,
    bottom: 28.0,
    right: 16.0,
};
const HEADER_SPACING: CGFloat = 4.0;
const SEARCH_FIELD_TOP_SPACING: CGFloat = 14.0;
const ACCESSORY_SPACING: CGFloat = 8.0;
const ICON_BUTTON_SIDE: CGFloat = 24.0;
const OPEN_SYMBOL: &str = "chevron.right";
const BACK_SYMBOL: &str = "chevron.left";
const CANCEL_SYMBOL: &str = "xmark.circle.fill";
const DELETE_SYMBOL: &str = "trash";
const INSTALLED_SYMBOL: &str = "checkmark.circle.fill";
const INSTALLED_DESCRIPTION: &str = "Downloaded";
const UNAVAILABLE_SYMBOL: &str = "lock.fill";
const UNAVAILABLE_DESCRIPTION: &str = "Unavailable";
const APP_ICON_SIDE: CGFloat = 20.0;
const UNKNOWN_APP_SYMBOL: &str = "app.dashed";
const UNKNOWN_APP_DESCRIPTION: &str = "App";

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

/// The views of a settings page, kept to update them on each render. The page keeps the search
/// field that it was made with.
#[derive(Debug)]
pub(super) struct PageView {
    pub(super) root: Retained<NSView>,
    content: Retained<NSStackView>,
    summary: Retained<NSTextField>,
    search: Option<SearchView>,
    sections: Vec<SectionView>,
}

impl PageView {
    /// Makes the views of `page`.
    pub(super) fn new(page: &view::Page, target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            SECTION_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        let summary = secondary_wrapping_label(main_thread);
        summary.setFont(Some(&NSFont::systemFontOfSize(SUMMARY_FONT_SIZE)));
        let (root, search) = if page.search.is_some() {
            let search = SearchView::new(target, main_thread);
            let header = stack(
                NSUserInterfaceLayoutOrientation::Vertical,
                HEADER_SPACING,
                HEADER_INSETS,
                main_thread,
            );
            let header_views: [&NSView; 2] = [&summary, &search.field];
            for view in header_views {
                header.addArrangedSubview(view);
                fill_width(view, &header);
            }
            header.setCustomSpacing_afterView(SEARCH_FIELD_TOP_SPACING, &summary);
            let root = pinned_root(&header, &content, main_thread);
            (Retained::into_super(root), Some(search))
        } else {
            content.addArrangedSubview(&summary);
            fill_width(&summary, &content);
            (Retained::into_super(scroll_view(&content, main_thread)), None)
        };
        Self {
            root,
            content,
            summary,
            search,
            sections: Vec::new(),
        }
    }

    /// Shows `page`. A section keeps its views while its rows keep their kinds and controls, so
    /// that an open menu, the focus, the typed text and a click in progress stay.
    pub(super) fn render(&mut self, page: &view::Page, binder: &mut Binder<'_>) {
        set_text(&self.summary, page.summary);
        if let (Some(search), Some(field)) = (&mut self.search, &page.search) {
            search.render(field, binder);
        }
        let kept = page.sections.len().min(self.sections.len());
        for section in self.sections.drain(kept..) {
            section.container.removeFromSuperview();
        }
        while self.sections.len() < page.sections.len() {
            self.sections.push(SectionView::new(&self.content, binder));
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

    /// The text in the text field or the search field `id`, when it is on the page.
    pub(super) fn field_text(&self, id: &ControlId) -> Option<String> {
        let search = self
            .search
            .as_ref()
            .filter(|search| search.id.as_ref() == Some(id))
            .map(|search| search.field.stringValue().to_string());
        search.or_else(|| {
            self.rows().find_map(|row| match row {
                RowView::TextField(text_field) if text_field.id == *id => {
                    Some(text_field.field.stringValue().to_string())
                }
                _ => None,
            })
        })
    }

    fn rows(&self) -> impl Iterator<Item = &RowView> {
        self.sections.iter().flat_map(|section| &section.rows)
    }
}

/// The search field of a page, which sends the submit action when the user submits or clears it.
#[derive(Debug)]
struct SearchView {
    field: Retained<NSSearchField>,
    id: Option<ControlId>,
}

impl SearchView {
    fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let field = NSSearchField::new(main_thread);
        field.setControlSize(NSControlSize::Large);
        field.setSendsWholeSearchString(true);
        // SAFETY: the target implements the submit action, taking the field, and `SettingsWindow`
        // keeps the target alive as long as the field.
        unsafe {
            field.setTarget(Some(&target.target));
            field.setAction(Some(target.submit_action));
        }
        Self { field, id: None }
    }

    fn render(&mut self, shown: &view::SearchField, binder: &mut Binder<'_>) {
        self.field
            .setPlaceholderString(Some(&NSString::from_str(shown.placeholder)));
        binder.bind(&self.field, &shown.id);
        self.id = Some(shown.id.clone());
    }
}

/// The views of one section: the back button and the heading, the rows, then the footer.
#[derive(Debug)]
struct SectionView {
    container: Retained<NSStackView>,
    title_bar: Retained<NSStackView>,
    back: Retained<NSButton>,
    heading: Retained<NSTextField>,
    rows_container: Retained<NSStackView>,
    footer: Retained<NSTextField>,
    rows: Vec<RowView>,
}

impl SectionView {
    fn new(content: &NSStackView, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let back = icon_button(binder.target, main_thread);
        back.setImagePosition(NSCellImagePosition::ImageLeading);
        let heading = heading(main_thread);
        let title_bar = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            ACCESSORY_SPACING,
            NO_INSETS,
            main_thread,
        );
        title_bar.addArrangedSubview(&back);
        title_bar.addArrangedSubview(&heading);
        let rows_container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        let footer = secondary_wrapping_label(main_thread);
        footer.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        let container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        container.addArrangedSubview(&title_bar);
        let parts: [&NSView; 2] = [&rows_container, &footer];
        for part in parts {
            container.addArrangedSubview(part);
            fill_width(part, &container);
        }
        content.addArrangedSubview(&container);
        fill_width(&container, content);
        Self {
            container,
            title_bar,
            back,
            heading,
            rows_container,
            footer,
            rows: Vec::new(),
        }
    }

    fn render(&mut self, section: &view::Section, binder: &mut Binder<'_>) {
        set_text(&self.heading, section.heading.as_deref().unwrap_or_default());
        match &section.back {
            Some(back) => show_back_button(&self.back, back, binder),
            None => self.back.setHidden(true),
        }
        self.title_bar
            .setHidden(section.heading.is_none() && section.back.is_none());
        set_text(&self.footer, section.footer.unwrap_or_default());
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
                Row::AppSwitch(switch) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::AppSwitch(AppSwitchRow::add(rows_stack, &switch.id, binder))
                }
                Row::Picker(picker) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::Picker(PickerRow::add(rows_stack, &picker.id, binder))
                }
                Row::Output(_) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::Output(OutputRow::add(rows_stack, binder.main_thread))
                }
                Row::Item(_) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::Item(ItemRow::add(rows_stack, binder))
                }
                Row::Loading(_) | Row::Message(_) => {
                    let rows_stack = group.get_or_insert_with(|| self.add_group(binder.main_thread));
                    RowView::Message(MessageRow::add(rows_stack, binder))
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
    AppSwitch(AppSwitchRow),
    Picker(PickerRow),
    TextArea(TextAreaRow),
    TextField(TextFieldRow),
    Note(NoteView),
    Output(OutputRow),
    Item(ItemRow),
    Message(MessageRow),
}

impl RowView {
    /// Whether the views show `row`: a row of their kind, for their control.
    fn shows(&self, row: &Row) -> bool {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.id == switch.id,
            (Self::AppSwitch(row_view), Row::AppSwitch(switch)) => row_view.id == switch.id,
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.id == picker.id,
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.id == text_area.id,
            (Self::TextField(row_view), Row::TextField(text_field)) => row_view.id == text_field.id,
            (Self::Note(_), Row::Note(_))
            | (Self::Output(_), Row::Output(_))
            | (Self::Item(_), Row::Item(_))
            | (Self::Message(_), Row::Loading(_) | Row::Message(_)) => true,
            _ => false,
        }
    }

    /// Shows `row`, when the views show it, see [`Self::shows`].
    fn render(&mut self, row: &Row, binder: &mut Binder<'_>) {
        match (self, row) {
            (Self::Switch(row_view), Row::Switch(switch)) => row_view.render(switch, binder),
            (Self::AppSwitch(row_view), Row::AppSwitch(switch)) => row_view.render(switch, binder),
            (Self::Picker(row_view), Row::Picker(picker)) => row_view.render(picker, binder),
            (Self::TextArea(row_view), Row::TextArea(text_area)) => row_view.render(text_area, binder),
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

/// A row with the icon and the name of an app, and a switch that turns a setting on and off for
/// the app.
#[derive(Debug)]
struct AppSwitchRow {
    icon: Retained<NSImageView>,
    name: Retained<NSTextField>,
    switch: Retained<NSSwitch>,
    id: ControlId,
    /// The app whose icon shows, so that a render loads the icon only for another app.
    icon_app_id: Option<String>,
}

impl AppSwitchRow {
    fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let switch = NSSwitch::new(main_thread);
        // SAFETY: the target implements the toggle action, taking the switch, and `SettingsWindow`
        // keeps the target alive as long as the switch.
        unsafe {
            switch.setTarget(Some(&binder.target.target));
            switch.setAction(Some(binder.target.toggle_action));
        }
        let icon = NSImageView::new(main_thread);
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            icon.widthAnchor().constraintEqualToConstant(APP_ICON_SIDE),
            icon.heightAnchor().constraintEqualToConstant(APP_ICON_SIDE),
        ]));
        let name = truncating_label(&NSFont::systemFontOfSize(NSFont::systemFontSize()), main_thread);
        let texts = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            ACCESSORY_SPACING,
            NO_INSETS,
            main_thread,
        );
        texts.addArrangedSubview(&icon);
        texts.addArrangedSubview(&name);
        texts.setContentHuggingPriority_forOrientation(
            NSLayoutPriorityDefaultLow,
            NSLayoutConstraintOrientation::Horizontal,
        );
        switch.setContentHuggingPriority_forOrientation(
            NSLayoutPriorityDefaultHigh,
            NSLayoutConstraintOrientation::Horizontal,
        );
        add_grouped_row(rows_stack, &texts, &[&switch], main_thread);
        Self {
            icon,
            name,
            switch,
            id: id.clone(),
            icon_app_id: None,
        }
    }

    fn render(&mut self, shown: &view::AppSwitch, binder: &mut Binder<'_>) {
        set_text(&self.name, &shown.app_name);
        if self.icon_app_id.as_deref() != Some(shown.app_id.as_str()) {
            self.icon.setImage(app_icon(&shown.app_id).as_deref());
            self.icon_app_id = Some(shown.app_id.clone());
        }
        self.switch
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.app_name)));
        let state = if shown.is_on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        };
        self.switch.setState(state);
        self.switch.setEnabled(shown.is_enabled);
        binder.bind(&self.switch, &shown.id);
    }
}

/// The icon of the app with the bundle identifier `app_id`, or a plain app symbol when no app on
/// this computer has it.
fn app_icon(app_id: &str) -> Option<Retained<NSImage>> {
    let workspace = NSWorkspace::sharedWorkspace();
    let path = workspace
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(app_id))
        .and_then(|url| url.path());
    match path {
        Some(path) => Some(workspace.iconForFile(&path)),
        None => symbol(UNKNOWN_APP_SYMBOL, UNKNOWN_APP_DESCRIPTION),
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

/// A row of a group with one thing of a list: texts, a trailing detail and the accessory controls.
#[derive(Debug)]
struct ItemRow {
    title: Retained<NSTextField>,
    subtitle: Retained<NSTextField>,
    detail: Retained<NSTextField>,
    progress: Retained<NSProgressIndicator>,
    status: Retained<NSImageView>,
    primary: Retained<NSButton>,
    icon: Retained<NSButton>,
}

impl ItemRow {
    fn add(rows_stack: &NSStackView, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let title = truncating_label(&NSFont::systemFontOfSize(NSFont::systemFontSize()), main_thread);
        let subtitle = truncating_label(
            &NSFont::systemFontOfSize(NSFont::smallSystemFontSize()),
            main_thread,
        );
        let texts = row_texts(&title, &subtitle, main_thread);
        let detail = label(&detail_font(), main_thread);
        detail.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let progress = spinner(main_thread);
        let status = NSImageView::new(main_thread);
        let primary = push_button(binder.target, main_thread);
        let icon = icon_button(binder.target, main_thread);
        let controls: [&NSView; 5] = [&detail, &progress, &status, &primary, &icon];
        add_grouped_row(rows_stack, &texts, &controls, main_thread);
        Self {
            title,
            subtitle,
            detail,
            progress,
            status,
            primary,
            icon,
        }
    }

    fn render(&self, shown: &view::Item, binder: &mut Binder<'_>) {
        set_text(&self.title, &shown.title);
        set_text(&self.subtitle, &shown.subtitle);
        set_text(&self.detail, &shown.detail);
        let subtitle_color = match shown.accessory {
            Accessory::Failed { .. } => NSColor::systemRedColor(),
            _ => NSColor::secondaryLabelColor(),
        };
        self.subtitle.setTextColor(Some(&subtitle_color));
        let controls: [&NSView; 4] = [&self.progress, &self.status, &self.primary, &self.icon];
        for control in controls {
            control.setHidden(true);
        }
        stop_spinning(&self.progress);
        self.show_accessory(&shown.accessory, &shown.title, binder);
    }

    fn show_accessory(&self, accessory: &Accessory, subject: &str, binder: &mut Binder<'_>) {
        match accessory {
            Accessory::Download(button) | Accessory::Failed { retry: button } => {
                show_push_button(&self.primary, button, Some(subject), binder);
            }
            Accessory::Open(button) => show_icon_button(&self.icon, OPEN_SYMBOL, button, subject, binder),
            Accessory::Downloading { progress, cancel } => {
                self.progress.setIndeterminate(false);
                self.progress.setDoubleValue(f64::from(progress.0));
                self.progress.setHidden(false);
                show_icon_button(&self.icon, CANCEL_SYMBOL, cancel, subject, binder);
            }
            Accessory::Canceling => start_spinning(&self.progress),
            Accessory::Installed { delete } => {
                show_status(
                    &self.status,
                    INSTALLED_SYMBOL,
                    INSTALLED_DESCRIPTION,
                    &NSColor::systemGreenColor(),
                );
                show_icon_button(&self.icon, DELETE_SYMBOL, delete, subject, binder);
            }
            Accessory::Unavailable => show_status(
                &self.status,
                UNAVAILABLE_SYMBOL,
                UNAVAILABLE_DESCRIPTION,
                &NSColor::tertiaryLabelColor(),
            ),
        }
    }
}

/// What shows in a group instead of items: a spinner or a message, with an optional button.
#[derive(Debug)]
struct MessageRow {
    spinner: Retained<NSProgressIndicator>,
    text: Retained<NSTextField>,
    button: Retained<NSButton>,
}

impl MessageRow {
    fn add(rows_stack: &NSStackView, binder: &Binder<'_>) -> Self {
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

/// The layout of a page with a search field: the pinned `header`, a divider, and `content` that
/// scrolls.
fn pinned_root(
    header: &NSView,
    content: &NSStackView,
    main_thread: MainThreadMarker,
) -> Retained<NSStackView> {
    let scroll_view = scroll_view(content, main_thread);
    let divider = separator(main_thread);
    let root = stack(
        NSUserInterfaceLayoutOrientation::Vertical,
        0.0,
        NO_INSETS,
        main_thread,
    );
    let root_views: [&NSView; 3] = [header, &divider, &scroll_view];
    for view in root_views {
        root.addArrangedSubview(view);
        fill_width(view, &root);
    }
    root.setDistribution(NSStackViewDistribution::Fill);
    root
}

/// Shows `button` in `view`, a push button with a title. The screen reader names it after its
/// `subject` when there is one.
fn show_push_button(view: &NSButton, button: &view::Button, subject: Option<&str>, binder: &mut Binder<'_>) {
    view.setTitle(&NSString::from_str(&title_case(button.label)));
    let spoken_label = subject.map(|subject| NSString::from_str(&format!("{}, {subject}", button.label)));
    view.setAccessibilityLabel(spoken_label.as_deref());
    view.setEnabled(button.is_enabled);
    binder.bind(view, &button.id);
    view.setHidden(false);
}

/// Shows `button` in `view`, an icon button with the symbol `symbol_name`. The screen reader
/// names it after its `subject`.
fn show_icon_button(
    view: &NSButton,
    symbol_name: &str,
    button: &view::Button,
    subject: &str,
    binder: &mut Binder<'_>,
) {
    let spoken_label = format!("{}, {subject}", button.label);
    view.setImage(symbol(symbol_name, &spoken_label).as_deref());
    view.setToolTip(Some(&NSString::from_str(button.label)));
    view.setAccessibilityLabel(Some(&NSString::from_str(&spoken_label)));
    view.setEnabled(button.is_enabled);
    binder.bind(view, &button.id);
    view.setHidden(false);
}

/// Shows `button` in `view`, the back button of a section title bar.
fn show_back_button(view: &NSButton, button: &view::Button, binder: &mut Binder<'_>) {
    view.setImage(symbol(BACK_SYMBOL, button.label).as_deref());
    view.setTitle(&NSString::from_str(&title_case(button.label)));
    view.setEnabled(button.is_enabled);
    binder.bind(view, &button.id);
    view.setHidden(false);
}

/// Returns a hidden push button that sends the press action to the target.
fn push_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    let button = button(target, target.press_action, main_thread);
    button.setHidden(true);
    button
}

/// Returns a hidden borderless button for a symbol, which sends the press action to the target.
fn icon_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    let button = push_button(target, main_thread);
    button.setBordered(false);
    button.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        button
            .widthAnchor()
            .constraintGreaterThanOrEqualToConstant(ICON_BUTTON_SIDE),
        button
            .heightAnchor()
            .constraintGreaterThanOrEqualToConstant(ICON_BUTTON_SIDE),
    ]));
    button
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

/// The font of short facts, such as sizes and timings, with digits of one width.
fn detail_font() -> Retained<NSFont> {
    // SAFETY: reads an AppKit constant that is initialized before any code runs.
    let regular_weight = unsafe { NSFontWeightRegular };
    NSFont::monospacedDigitSystemFontOfSize_weight(DETAIL_FONT_SIZE, regular_weight)
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
