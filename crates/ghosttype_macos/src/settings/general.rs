//! The General pane: the autostart switch, the clipboard switch, the prompt preparation switch,
//! the suggestion display switch, the suggestion length menu and the suggestion model menu in one
//! rounded group, then the system prompt editor.

use ghosttype_core::general_pane::{
    GeneralAction, GeneralView, SettingPicker, SettingSwitch, SystemPromptEditor,
};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2_app_kit::{
    NSAccessibility as _, NSBorderType, NSButton, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSLayoutConstraintOrientation, NSLayoutPriorityDefaultHigh, NSLayoutPriorityDefaultLow, NSPopUpButton,
    NSScrollView, NSStackView, NSStackViewGravity, NSSwitch, NSTextDidChangeNotification, NSTextField,
    NSTextView, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSEdgeInsets, NSNotificationCenter, NSSize, NSString};

use super::{
    ActionTarget, CONTENT_INSETS, GroupedRow, LIST_PART_SPACING, ModelMenu, NO_INSETS, NoteView,
    SECTION_SPACING, SUMMARY_FONT_SIZE, fill_width, label, pop_up_button, rebuild_menu, rounded_group,
    row_texts, scroll_view, secondary_wrapping_label, set_text, stack, truncating_label,
};
use crate::labels::title_case;

const SYSTEM_PROMPT_HEIGHT: CGFloat = 120.0;
/// Puts a section spacing above the editor, also while the suggestion model note above it hides.
const SYSTEM_PROMPT_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: SECTION_SPACING - LIST_PART_SPACING,
    left: 0.0,
    bottom: 0.0,
    right: 0.0,
};
const SYSTEM_PROMPT_TEXT_INSETS: NSSize = NSSize::new(4.0, 6.0);

/// The views of the General pane, kept to update them on each render.
#[derive(Debug)]
pub(super) struct GeneralPage {
    pub(super) root: Retained<NSScrollView>,
    pub(super) suggestion_model_menu: ModelMenu,
    pub(super) suggestion_length_menu: SettingMenu,
    summary: Retained<NSTextField>,
    autostart_row: SettingRow,
    autostart_switch: Retained<NSSwitch>,
    clipboard_row: SettingRow,
    clipboard_switch: Retained<NSSwitch>,
    preparation_row: SettingRow,
    preparation_switch: Retained<NSSwitch>,
    display_row: SettingRow,
    display_switch: Retained<NSSwitch>,
    suggestion_length_row: SettingRow,
    suggestion_model_row: SettingRow,
    suggestion_model_note: NoteView,
    system_prompt: SystemPromptView,
}

impl GeneralPage {
    /// Builds the pane. The autostart switch sends `target.autostart_action`, the clipboard switch
    /// sends `target.clipboard_action`, the prompt preparation switch sends
    /// `target.preparation_action`, the suggestion display switch sends `target.display_action`,
    /// the suggestion length menu sends `target.suggestion_length_action`, and the suggestion model
    /// menu sends `target.suggestion_model_action`. Each edit of the system prompt sends
    /// `target.system_prompt_action` with the notification, and its reset button sends
    /// `target.reset_system_prompt_action`.
    pub(super) fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        let summary = secondary_wrapping_label(main_thread);
        summary.setFont(Some(&NSFont::systemFontOfSize(SUMMARY_FONT_SIZE)));
        let autostart_switch = switch(target, target.autostart_action, main_thread);
        let clipboard_switch = switch(target, target.clipboard_action, main_thread);
        let preparation_switch = switch(target, target.preparation_action, main_thread);
        let display_switch = switch(target, target.display_action, main_thread);
        let suggestion_length_menu = SettingMenu::new(target, main_thread);
        let suggestion_model_menu = ModelMenu::new(target, target.suggestion_model_action, main_thread);
        let rows_stack = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            0.0,
            NO_INSETS,
            main_thread,
        );
        let autostart_row = SettingRow::add(&rows_stack, &autostart_switch, main_thread);
        autostart_row.row.separator.setHidden(true);
        let clipboard_row = SettingRow::add(&rows_stack, &clipboard_switch, main_thread);
        let preparation_row = SettingRow::add(&rows_stack, &preparation_switch, main_thread);
        let display_row = SettingRow::add(&rows_stack, &display_switch, main_thread);
        let suggestion_length_row = SettingRow::add(&rows_stack, &suggestion_length_menu.button, main_thread);
        let suggestion_model_row = SettingRow::add(&rows_stack, &suggestion_model_menu.button, main_thread);
        let group = rounded_group(&rows_stack, main_thread);
        let suggestion_model_note = NoteView::new(main_thread);
        let system_prompt = SystemPromptView::new(target, main_thread);
        let parts: [&NSView; 4] = [
            &summary,
            &group,
            &suggestion_model_note.container,
            &system_prompt.container,
        ];
        for part in parts {
            content.addArrangedSubview(part);
            fill_width(part, &content);
        }
        content.setCustomSpacing_afterView(SECTION_SPACING, &summary);
        Self {
            root: scroll_view(&content, main_thread),
            suggestion_model_menu,
            suggestion_length_menu,
            summary,
            autostart_row,
            autostart_switch,
            clipboard_row,
            clipboard_switch,
            preparation_row,
            preparation_switch,
            display_row,
            display_switch,
            suggestion_length_row,
            suggestion_model_row,
            suggestion_model_note,
            system_prompt,
        }
    }

    /// Returns the text in the system prompt field.
    pub(super) fn system_prompt(&self) -> String {
        self.system_prompt.text_view.string().to_string()
    }

    /// Shows `view`.
    pub(super) fn render(&mut self, view: &GeneralView) {
        set_text(&self.summary, view.summary);
        self.render_autostart(view.autostart.as_ref());
        self.clipboard_row
            .render_switch(&self.clipboard_switch, &view.clipboard);
        self.preparation_row
            .render_switch(&self.preparation_switch, &view.prompt_preparation);
        self.display_row
            .render_switch(&self.display_switch, &view.suggestion_display);
        let length = &view.suggestion_length;
        self.suggestion_length_row
            .render(length.label, length.description);
        self.suggestion_length_menu.render(length);
        let picker = &view.suggestion_model;
        self.suggestion_model_row
            .render(picker.menu.label, picker.description);
        self.suggestion_model_menu.render(&picker.menu);
        self.suggestion_model_note.render(picker.note.as_ref());
        self.system_prompt.render(&view.system_prompt);
    }

    fn render_autostart(&self, autostart: Option<&SettingSwitch>) {
        self.autostart_row.row.container.setHidden(autostart.is_none());
        self.clipboard_row.row.separator.setHidden(autostart.is_none());
        if let Some(autostart) = autostart {
            self.autostart_row
                .render_switch(&self.autostart_switch, autostart);
        }
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

    /// Shows `setting` in the row and in its `switch`.
    fn render_switch(&self, switch: &NSSwitch, setting: &SettingSwitch) {
        self.render(setting.label, setting.description);
        switch.setAccessibilityLabel(Some(&NSString::from_str(setting.label)));
        let state = if setting.is_on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        };
        switch.setState(state);
    }
}

/// Returns a switch that sends `action` to the target.
fn switch(target: &ActionTarget, action: Sel, main_thread: MainThreadMarker) -> Retained<NSSwitch> {
    let switch = NSSwitch::new(main_thread);
    // SAFETY: the target implements each switch action, taking the switch, and `SettingsWindow`
    // keeps the target alive as long as the switch.
    unsafe {
        switch.setTarget(Some(&target.target));
        switch.setAction(Some(action));
    }
    switch
}

/// A pop-up menu of the values of a setting.
#[derive(Debug)]
pub(super) struct SettingMenu {
    button: Retained<NSPopUpButton>,
    titles: Vec<String>,
    actions: Vec<GeneralAction>,
}

impl SettingMenu {
    /// Builds a menu that sends `target.suggestion_length_action` when the user picks a value.
    fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        Self {
            button: pop_up_button(target, target.suggestion_length_action, main_thread),
            titles: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Shows `picker`, and keeps the actions of its entries for [`Self::action_at`].
    fn render(&mut self, picker: &SettingPicker) {
        self.button
            .setAccessibilityLabel(Some(&NSString::from_str(picker.label)));
        let titles: Vec<String> = picker
            .choices
            .iter()
            .map(|choice| title_case(choice.title))
            .collect();
        if titles != self.titles {
            rebuild_menu(&self.button, &titles);
            self.titles = titles;
        }
        self.actions = picker
            .choices
            .iter()
            .map(|choice| choice.action.clone())
            .collect();
        let selected = isize::try_from(picker.selected).expect("a menu never holds isize::MAX entries");
        self.button.selectItemAtIndex(selected);
        self.button.setEnabled(picker.is_enabled);
    }

    /// Returns the action of the entry at `index` in the last render.
    pub(super) fn action_at(&self, index: isize) -> Option<GeneralAction> {
        let index = usize::try_from(index).ok()?;
        self.actions.get(index).cloned()
    }
}

/// The system prompt editor: a heading, a description, the text field and the reset button.
#[derive(Debug)]
struct SystemPromptView {
    container: Retained<NSStackView>,
    heading: Retained<NSTextField>,
    description: Retained<NSTextField>,
    text_view: Retained<NSTextView>,
    reset_button: Retained<NSButton>,
}

impl SystemPromptView {
    fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let heading = label(
            &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
            main_thread,
        );
        let description = secondary_wrapping_label(main_thread);
        description.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        let (text_scroll_view, text_view) = system_prompt_text_view(target, main_thread);
        let reset_button = reset_button(target, main_thread);
        let button_row = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            0.0,
            NO_INSETS,
            main_thread,
        );
        button_row.addView_inGravity(&reset_button, NSStackViewGravity::Trailing);
        let container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            SYSTEM_PROMPT_INSETS,
            main_thread,
        );
        let parts: [&NSView; 4] = [&heading, &description, &text_scroll_view, &button_row];
        for part in parts {
            container.addArrangedSubview(part);
            fill_width(part, &container);
        }
        Self {
            container,
            heading,
            description,
            text_view,
            reset_button,
        }
    }

    fn render(&self, editor: &SystemPromptEditor) {
        set_text(&self.heading, editor.label);
        set_text(&self.description, editor.description);
        self.text_view
            .setAccessibilityLabel(Some(&NSString::from_str(editor.label)));
        if self.text_view.string().to_string() != editor.text {
            self.text_view.setString(&NSString::from_str(&editor.text));
        }
        self.reset_button
            .setTitle(&NSString::from_str(&title_case(editor.reset_label)));
        self.reset_button.setEnabled(editor.is_reset_enabled);
    }
}

/// Returns a plain text view with a bezel, in the scroll view that holds it.
fn system_prompt_text_view(
    target: &ActionTarget,
    main_thread: MainThreadMarker,
) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let text_scroll_view = NSTextView::scrollableTextView(main_thread);
    text_scroll_view.setBorderType(NSBorderType::BezelBorder);
    text_scroll_view
        .heightAnchor()
        .constraintEqualToConstant(SYSTEM_PROMPT_HEIGHT)
        .setActive(true);
    let text_view = text_scroll_view
        .documentView()
        .and_then(|view| view.downcast::<NSTextView>().ok())
        .expect("a scrollable text view holds a text view");
    text_view.setRichText(false);
    text_view.setAllowsUndo(true);
    text_view.setFont(Some(&NSFont::systemFontOfSize(NSFont::systemFontSize())));
    text_view.setTextContainerInset(SYSTEM_PROMPT_TEXT_INSETS);
    // SAFETY: the target implements the action, taking the notification, and the center drops
    // deallocated observers itself.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &target.target,
            target.system_prompt_action,
            Some(NSTextDidChangeNotification),
            Some(&text_view),
        );
    }
    (text_scroll_view, text_view)
}

fn reset_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: the target implements the reset action, taking the button, and `SettingsWindow`
    // keeps the target alive as long as the button.
    unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::new(),
            Some(&target.target),
            Some(target.reset_system_prompt_action),
            main_thread,
        )
    }
}
