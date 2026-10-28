//! The content of a setup step, which scrolls: the picture of the step, the title and the body, the
//! demo, the sections with the rows of the settings window, and the footnote, top to bottom.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibility as _, NSColor, NSFont, NSFontWeightRegular, NSImageSymbolConfiguration, NSImageView,
    NSLayoutAttribute, NSScrollView, NSStackView, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSEdgeInsets, NSPoint};
use typealike_core::settings_window::view::{ControlId, SettingsInput};
use typealike_core::setup::{SetupStep, SetupView};

use crate::settings::Binder;
use crate::settings::controls::{
    secondary_wrapping_label, set_text, small_secondary_label, symbol, wrapping_label,
};
use crate::settings::layout::{NO_INSETS, fill_width, scroll_view, stack};
use crate::settings::page::SectionList;
use crate::settings::setup::demo::DemoView;

const CONTENT_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 8.0,
    left: 40.0,
    bottom: 28.0,
    right: 40.0,
};
const PART_SPACING: CGFloat = 24.0;
const HERO_SPACING: CGFloat = 16.0;
const TITLE_SPACING: CGFloat = 8.0;
const SECTION_SPACING: CGFloat = 20.0;
const HERO_SYMBOL_SIZE: CGFloat = 56.0;
const LARGE_TITLE_SIZE: CGFloat = 26.0;

/// The SF Symbol of each step, and its system color, which names the step and never a state.
fn hero(step: SetupStep) -> (&'static str, Retained<NSColor>) {
    match step {
        SetupStep::Welcome => ("character.cursor.ibeam", NSColor::systemBlueColor()),
        SetupStep::Model => ("arrow.down.circle", NSColor::systemIndigoColor()),
        SetupStep::Permission => ("hand.raised", NSColor::systemOrangeColor()),
        SetupStep::About => ("person.text.rectangle", NSColor::systemGreenColor()),
        SetupStep::Ready => ("checkmark.circle", NSColor::systemTealColor()),
    }
}

/// The views of the content, kept to update them on each render.
#[derive(Debug)]
pub(super) struct StepView {
    pub(super) root: Retained<NSScrollView>,
    content: Retained<NSStackView>,
    hero: Retained<NSImageView>,
    title: Retained<NSTextField>,
    body: Retained<NSTextField>,
    pub(super) demo: DemoView,
    sections_stack: Retained<NSStackView>,
    sections: SectionList,
    footnote: Retained<NSTextField>,
}

impl StepView {
    pub(super) fn new(main_thread: MainThreadMarker) -> Self {
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            PART_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        content.setAlignment(NSLayoutAttribute::CenterX);
        content.setWantsLayer(true);
        let hero = hero_view(main_thread);
        let title = centered_label(wrapping_label(main_thread));
        title.setFont(Some(&NSFont::boldSystemFontOfSize(LARGE_TITLE_SIZE)));
        let body = centered_label(secondary_wrapping_label(main_thread));
        let demo = DemoView::new(main_thread);
        let sections_stack = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            SECTION_SPACING,
            NO_INSETS,
            main_thread,
        );
        let footnote = centered_label(small_secondary_label(main_thread));
        content.addArrangedSubview(&hero);
        let parts: [&NSView; 5] = [&title, &body, &demo.root, &sections_stack, &footnote];
        for part in parts {
            content.addArrangedSubview(part);
            fill_width(part, &content);
        }
        content.setCustomSpacing_afterView(HERO_SPACING, &hero);
        content.setCustomSpacing_afterView(TITLE_SPACING, &title);
        Self {
            root: scroll_view(&content, main_thread),
            content,
            hero,
            title,
            body,
            demo,
            sections: SectionList::new(sections_stack.clone()),
            sections_stack,
            footnote,
        }
    }

    /// The view whose changes the transition between two steps animates.
    pub(super) fn animated_view(&self) -> &NSView {
        &self.content
    }

    /// Shows `view`. The sections keep their views while their rows keep their kinds and
    /// controls, so that the focus and the typed text stay.
    pub(super) fn render(&mut self, view: &SetupView, binder: &mut Binder<'_>) {
        show_hero(&self.hero, view.step, &view.title);
        set_text(&self.title, &view.title);
        set_text(&self.body, &view.body);
        self.demo.render(view.demo.as_ref());
        self.sections_stack.setHidden(view.sections.is_empty());
        self.sections.render(&view.sections, binder);
        set_text(&self.footnote, view.footnote.as_deref().unwrap_or_default());
        self.footnote.setHidden(view.footnote.is_none());
    }

    /// Scrolls the content to its top.
    pub(super) fn scroll_to_top(&self) {
        if let Some(document) = self.root.documentView() {
            document.scrollPoint(NSPoint::ZERO);
        }
    }

    /// The input of a change of the text in `view`, when it is a text area or a line of text of
    /// the step.
    pub(super) fn edited(&self, view: &AnyObject) -> Option<SettingsInput> {
        self.sections.edited(view)
    }

    /// The text in the text field `id`, when it is in the step.
    pub(super) fn field_text(&self, id: &ControlId) -> Option<String> {
        self.sections.field_text(id)
    }
}

/// An image view for the large symbol of a step. Assistive technology skips it, because the title
/// names the step.
fn hero_view(main_thread: MainThreadMarker) -> Retained<NSImageView> {
    let view = NSImageView::new(main_thread);
    view.setAccessibilityElement(false);
    view
}

fn show_hero(view: &NSImageView, step: SetupStep, description: &str) {
    let (name, color) = hero(step);
    view.setImage(symbol(name, description).as_deref());
    let size = NSImageSymbolConfiguration::configurationWithPointSize_weight(
        HERO_SYMBOL_SIZE,
        // SAFETY: the weight is a constant of AppKit.
        unsafe { NSFontWeightRegular },
    );
    let colors = NSImageSymbolConfiguration::configurationWithHierarchicalColor(&color);
    view.setSymbolConfiguration(Some(&size.configurationByApplyingConfiguration(&colors)));
}

/// `label` with its text in the center, and not selectable, so that the keyboard focus moves only
/// through the controls.
fn centered_label(label: Retained<NSTextField>) -> Retained<NSTextField> {
    label.setAlignment(NSTextAlignment::Center);
    label.setSelectable(false);
    label
}
