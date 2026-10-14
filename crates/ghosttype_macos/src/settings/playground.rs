//! The Playground pane: a model menu, a prompt field and the text the model adds.

use ghosttype_core::playground::{Note, NoteTone, PlaygroundAction, PlaygroundOutput, PlaygroundView};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly as _};
use objc2_app_kit::{
    NSAccessibility as _, NSBox, NSButton, NSColor, NSFont, NSFontWeightRegular, NSLineBreakMode, NSMenuItem,
    NSPopUpButton, NSProgressIndicator, NSScrollView, NSStackView, NSStackViewGravity, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSEdgeInsets, NSString};

use super::{
    ActionTarget, CONTENT_INSETS, DETAIL_FONT_SIZE, LIST_PART_SPACING, MESSAGE_SPACING, NO_INSETS,
    ROW_CONTAINER_INSETS, SECTION_SPACING, SUMMARY_FONT_SIZE, fill_width, label, rounded_group, scroll_view,
    set_text, spinner, stack, start_spinning, stop_spinning,
};
use crate::labels::title_case;

const FORM_ROW_HEIGHT: CGFloat = 44.0;
const PROMPT_FIELD_HEIGHT: CGFloat = 88.0;
const OUTPUT_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 12.0,
    left: 12.0,
    bottom: 12.0,
    right: 12.0,
};

/// The views of the Playground pane, kept to update them on each render.
#[derive(Debug)]
pub(super) struct PlaygroundPage {
    pub(super) root: Retained<NSScrollView>,
    summary: Retained<NSTextField>,
    model_label: Retained<NSTextField>,
    model_menu: Retained<NSPopUpButton>,
    model_note: NoteView,
    prompt_label: Retained<NSTextField>,
    prompt_field: Retained<NSTextField>,
    complete_button: Retained<NSButton>,
    output: OutputView,
    choice_titles: Vec<String>,
    choice_actions: Vec<Option<PlaygroundAction>>,
}

impl PlaygroundPage {
    /// Builds the pane. The model menu sends `target.model_picker_action`; the complete button and
    /// the return key in the prompt field send `target.complete_action`.
    pub(super) fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        let summary = secondary_wrapping_label(main_thread);
        summary.setFont(Some(&NSFont::systemFontOfSize(SUMMARY_FONT_SIZE)));
        let model_label = label(&NSFont::systemFontOfSize(NSFont::systemFontSize()), main_thread);
        let model_menu = model_menu(target, main_thread);
        let model_row = form_row(&model_label, &model_menu, main_thread);
        let model_group = rounded_group(&model_row, main_thread);
        let model_note = NoteView::new(main_thread);
        let prompt_label = heading(main_thread);
        let prompt_field = prompt_field(target, main_thread);
        let complete_button = complete_button(target, main_thread);
        let button_row = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            0.0,
            NO_INSETS,
            main_thread,
        );
        button_row.addView_inGravity(&complete_button, NSStackViewGravity::Trailing);
        let output = OutputView::new(main_thread);
        let parts: [&NSView; 9] = [
            &summary,
            &model_group,
            &model_note.container,
            &prompt_label,
            &prompt_field,
            &button_row,
            &output.heading,
            &output.group,
            &output.note.container,
        ];
        for part in parts {
            content.addArrangedSubview(part);
            fill_width(part, &content);
        }
        content.setCustomSpacing_afterView(SECTION_SPACING, &summary);
        content.setCustomSpacing_afterView(SECTION_SPACING, &model_note.container);
        content.setCustomSpacing_afterView(SECTION_SPACING, &button_row);
        Self {
            root: scroll_view(&content, main_thread),
            summary,
            model_label,
            model_menu,
            model_note,
            prompt_label,
            prompt_field,
            complete_button,
            output,
            choice_titles: Vec::new(),
            choice_actions: Vec::new(),
        }
    }

    /// Shows `view`. Rebuilds the model menu only when its entries change, so an open menu stays.
    pub(super) fn render(&mut self, view: &PlaygroundView) {
        set_text(&self.summary, view.summary);
        set_text(&self.model_label, view.model.label);
        self.model_menu
            .setAccessibilityLabel(Some(&NSString::from_str(view.model.label)));
        self.render_choices(view);
        self.model_note.render(view.model_note.as_ref());
        set_text(&self.prompt_label, view.prompt_label);
        self.prompt_field
            .setPlaceholderString(Some(&NSString::from_str(view.prompt_placeholder)));
        self.prompt_field
            .setAccessibilityLabel(Some(&NSString::from_str(view.prompt_label)));
        self.complete_button
            .setTitle(&NSString::from_str(&title_case(view.complete_label)));
        self.complete_button.setEnabled(view.is_complete_enabled);
        self.output.render(view.output_heading, view.output.as_ref());
    }

    /// Returns the action of the model menu entry at `index` in the last render.
    pub(super) fn choice_action(&self, index: isize) -> Option<PlaygroundAction> {
        let index = usize::try_from(index).ok()?;
        self.choice_actions.get(index).cloned().flatten()
    }

    /// Returns the text in the prompt field.
    pub(super) fn prompt(&self) -> String {
        self.prompt_field.stringValue().to_string()
    }

    fn render_choices(&mut self, view: &PlaygroundView) {
        let titles: Vec<String> = view
            .model
            .choices
            .iter()
            .map(|choice| choice.title.clone())
            .collect();
        if titles != self.choice_titles {
            self.rebuild_menu(&titles);
            self.choice_titles = titles;
        }
        self.choice_actions = view
            .model
            .choices
            .iter()
            .map(|choice| choice.action.clone())
            .collect();
        let selected = isize::try_from(view.model.selected).expect("a menu never holds isize::MAX entries");
        self.model_menu.selectItemAtIndex(selected);
        self.model_menu.setEnabled(view.model.is_enabled);
    }

    fn rebuild_menu(&self, titles: &[String]) {
        self.model_menu.removeAllItems();
        let Some(menu) = self.model_menu.menu() else {
            return;
        };
        for title in titles {
            // SAFETY: the item has no action, so the pop-up button sends its own action instead.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(self.model_menu.mtm()),
                    &NSString::from_str(title),
                    None,
                    &NSString::new(),
                )
            };
            menu.addItem(&item);
        }
    }
}

/// A spinner and a line of status text.
#[derive(Debug)]
struct NoteView {
    container: Retained<NSStackView>,
    spinner: Retained<NSProgressIndicator>,
    text: Retained<NSTextField>,
}

impl NoteView {
    fn new(main_thread: MainThreadMarker) -> Self {
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

/// The heading and rounded group that show the text the model added, or a note instead.
#[derive(Debug)]
struct OutputView {
    heading: Retained<NSTextField>,
    group: Retained<NSBox>,
    text: Retained<NSTextField>,
    detail: Retained<NSTextField>,
    note: NoteView,
}

impl OutputView {
    fn new(main_thread: MainThreadMarker) -> Self {
        let heading = heading(main_thread);
        let text = NSTextField::wrappingLabelWithString(&NSString::new(), main_thread);
        text.setSelectable(true);
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
            OUTPUT_INSETS,
            main_thread,
        );
        texts.addArrangedSubview(&text);
        texts.addArrangedSubview(&detail);
        fill_width(&text, &texts);
        fill_width(&detail, &texts);
        let group = rounded_group(&texts, main_thread);
        Self {
            heading,
            group,
            text,
            detail,
            note: NoteView::new(main_thread),
        }
    }

    fn render(&self, heading: &str, output: Option<&PlaygroundOutput>) {
        set_text(&self.heading, heading);
        self.heading.setHidden(output.is_none());
        match output {
            Some(PlaygroundOutput::Completion { text, detail }) => {
                set_text(&self.text, text);
                set_text(&self.detail, detail);
                self.group.setHidden(false);
                self.note.render(None);
            }
            Some(PlaygroundOutput::Note(note)) => {
                self.group.setHidden(true);
                self.note.render(Some(note));
            }
            None => {
                self.group.setHidden(true);
                self.note.render(None);
            }
        }
    }
}

fn form_row(leading: &NSView, trailing: &NSView, main_thread: MainThreadMarker) -> Retained<NSStackView> {
    let row = stack(
        NSUserInterfaceLayoutOrientation::Horizontal,
        LIST_PART_SPACING,
        ROW_CONTAINER_INSETS,
        main_thread,
    );
    row.addView_inGravity(leading, NSStackViewGravity::Leading);
    row.addView_inGravity(trailing, NSStackViewGravity::Trailing);
    row.heightAnchor()
        .constraintEqualToConstant(FORM_ROW_HEIGHT)
        .setActive(true);
    row
}

fn model_menu(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSPopUpButton> {
    let menu = NSPopUpButton::new(main_thread);
    menu.setPullsDown(false);
    // SAFETY: the target implements the model picker action, taking the pop-up button, and
    // `SettingsWindow` keeps the target alive as long as the button.
    unsafe {
        menu.setTarget(Some(&target.target));
        menu.setAction(Some(target.model_picker_action));
    }
    menu
}

fn prompt_field(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::textFieldWithString(&NSString::new(), main_thread);
    field.setUsesSingleLineMode(false);
    field.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    field
        .heightAnchor()
        .constraintEqualToConstant(PROMPT_FIELD_HEIGHT)
        .setActive(true);
    // SAFETY: the target implements the complete action, taking the field, and `SettingsWindow`
    // keeps the target alive as long as the field.
    unsafe {
        field.setTarget(Some(&target.target));
        field.setAction(Some(target.complete_action));
    }
    field
}

fn complete_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: the target implements the complete action, taking the button, and `SettingsWindow`
    // keeps the target alive as long as the button.
    unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::new(),
            Some(&target.target),
            Some(target.complete_action),
            main_thread,
        )
    }
}

fn heading(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    label(
        &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
        main_thread,
    )
}

fn secondary_wrapping_label(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::wrappingLabelWithString(&NSString::new(), main_thread);
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label
}
