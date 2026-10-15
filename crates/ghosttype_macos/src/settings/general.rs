//! The General pane: the autostart switch and the suggestion model menu in one rounded group.

use ghosttype_core::general_pane::{AutostartSwitch, GeneralView};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSLayoutConstraintOrientation, NSLayoutPriorityDefaultHigh, NSLayoutPriorityDefaultLow, NSScrollView,
    NSStackView, NSSwitch, NSTextField, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::NSString;

use super::{
    ActionTarget, CONTENT_INSETS, GroupedRow, LIST_PART_SPACING, ModelMenu, NO_INSETS, NoteView,
    SECTION_SPACING, SUMMARY_FONT_SIZE, fill_width, rounded_group, row_texts, scroll_view,
    secondary_wrapping_label, set_text, stack, truncating_label,
};

/// The views of the General pane, kept to update them on each render.
#[derive(Debug)]
pub(super) struct GeneralPage {
    pub(super) root: Retained<NSScrollView>,
    pub(super) suggestion_model_menu: ModelMenu,
    summary: Retained<NSTextField>,
    autostart_row: SettingRow,
    autostart_switch: Retained<NSSwitch>,
    suggestion_model_row: SettingRow,
    suggestion_model_note: NoteView,
}

impl GeneralPage {
    /// Builds the pane. The autostart switch sends `target.autostart_action`; the suggestion model
    /// menu sends `target.suggestion_model_action`.
    pub(super) fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        let summary = secondary_wrapping_label(main_thread);
        summary.setFont(Some(&NSFont::systemFontOfSize(SUMMARY_FONT_SIZE)));
        let autostart_switch = autostart_switch(target, main_thread);
        let suggestion_model_menu = ModelMenu::new(target, target.suggestion_model_action, main_thread);
        let rows_stack = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            0.0,
            NO_INSETS,
            main_thread,
        );
        let autostart_row = SettingRow::add(&rows_stack, &autostart_switch, main_thread);
        autostart_row.row.separator.setHidden(true);
        let suggestion_model_row = SettingRow::add(&rows_stack, &suggestion_model_menu.button, main_thread);
        let group = rounded_group(&rows_stack, main_thread);
        let suggestion_model_note = NoteView::new(main_thread);
        let parts: [&NSView; 3] = [&summary, &group, &suggestion_model_note.container];
        for part in parts {
            content.addArrangedSubview(part);
            fill_width(part, &content);
        }
        content.setCustomSpacing_afterView(SECTION_SPACING, &summary);
        Self {
            root: scroll_view(&content, main_thread),
            suggestion_model_menu,
            summary,
            autostart_row,
            autostart_switch,
            suggestion_model_row,
            suggestion_model_note,
        }
    }

    /// Shows `view`.
    pub(super) fn render(&mut self, view: &GeneralView) {
        set_text(&self.summary, view.summary);
        self.render_autostart(view.autostart.as_ref());
        let picker = &view.suggestion_model;
        self.suggestion_model_row
            .render(picker.menu.label, picker.description);
        self.suggestion_model_menu.render(&picker.menu);
        self.suggestion_model_note.render(picker.note.as_ref());
    }

    fn render_autostart(&self, autostart: Option<&AutostartSwitch>) {
        self.autostart_row.row.container.setHidden(autostart.is_none());
        self.suggestion_model_row
            .row
            .separator
            .setHidden(autostart.is_none());
        let Some(autostart) = autostart else {
            return;
        };
        self.autostart_row.render(autostart.label, autostart.description);
        self.autostart_switch
            .setAccessibilityLabel(Some(&NSString::from_str(autostart.label)));
        let state = if autostart.is_on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        };
        self.autostart_switch.setState(state);
    }
}

/// A row of the group: a label over a description, and the control that changes the setting.
#[derive(Debug)]
struct SettingRow {
    row: GroupedRow,
    label: Retained<NSTextField>,
    description: Retained<NSTextField>,
}

impl SettingRow {
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
        Self {
            row: GroupedRow::add(rows_stack, &texts, &[control], main_thread),
            label,
            description,
        }
    }

    fn render(&self, label: &str, description: &str) {
        set_text(&self.label, label);
        set_text(&self.description, description);
    }
}

fn autostart_switch(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSSwitch> {
    let switch = NSSwitch::new(main_thread);
    // SAFETY: the target implements the autostart action, taking the switch, and `SettingsWindow`
    // keeps the target alive as long as the switch.
    unsafe {
        switch.setTarget(Some(&target.target));
        switch.setAction(Some(target.autostart_action));
    }
    switch
}
