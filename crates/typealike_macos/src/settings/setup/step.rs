//! The content of a setup step, which scrolls: the picture of the step, the title and the body, the
//! demo, the sections with the rows of the settings window, and the footnote, top to bottom.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAccessibility as _, NSColor, NSFont, NSFontWeightRegular, NSImage, NSImageSymbolConfiguration,
    NSImageView, NSLayoutAttribute, NSScrollView, NSStackView, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSData, NSEdgeInsets, NSPoint, NSSize, NSString};
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
/// The size of an app icon in an alert.
const APP_ICON_SIZE: CGFloat = 64.0;
const LARGE_TITLE_SIZE: CGFloat = 26.0;

/// The app icon of `TypeAlike.icns` at 2x. `assets/build_icons.py` makes the file.
const APP_ICON: &[u8] = include_bytes!("../../../assets/logo.png");

/// The picture above the title of a step.
enum Hero {
    /// The app icon.
    AppIcon,
    /// An SF Symbol and its system color, which names the step and never a state.
    Symbol(&'static str, Retained<NSColor>),
}

/// The picture of each step. The welcome step shows the app icon.
fn hero(step: SetupStep) -> Hero {
    match step {
        SetupStep::Welcome => Hero::AppIcon,
        SetupStep::Languages => Hero::Symbol("character.bubble", NSColor::systemBlueColor()),
        SetupStep::About => Hero::Symbol("person.text.rectangle", NSColor::systemGreenColor()),
        SetupStep::Sources => Hero::Symbol("text.viewfinder", NSColor::systemPurpleColor()),
        SetupStep::Permission => Hero::Symbol("hand.raised", NSColor::systemOrangeColor()),
        SetupStep::Model => Hero::Symbol("arrow.down.circle", NSColor::systemIndigoColor()),
        SetupStep::Ready => Hero::Symbol("checkmark.circle", NSColor::systemTealColor()),
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
        set_text(&self.body, view.body.as_deref().unwrap_or_default());
        self.body.setHidden(view.body.is_none());
        let title_spacing = if view.body.is_some() {
            TITLE_SPACING
        } else {
            PART_SPACING
        };
        self.content
            .setCustomSpacing_afterView(title_spacing, &self.title);
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

/// An image view for the large picture of a step. Assistive technology skips it, because the title
/// names the step.
fn hero_view(main_thread: MainThreadMarker) -> Retained<NSImageView> {
    let view = NSImageView::new(main_thread);
    view.setAccessibilityElement(false);
    view
}

fn show_hero(view: &NSImageView, step: SetupStep, description: &str) {
    match hero(step) {
        Hero::AppIcon => {
            view.setSymbolConfiguration(None);
            view.setImage(app_icon(description).as_deref());
        }
        Hero::Symbol(name, color) => {
            view.setImage(symbol(name, description).as_deref());
            let size = NSImageSymbolConfiguration::configurationWithPointSize_weight(
                HERO_SYMBOL_SIZE,
                // SAFETY: the weight is a constant of AppKit.
                unsafe { NSFontWeightRegular },
            );
            let colors = NSImageSymbolConfiguration::configurationWithHierarchicalColor(&color);
            view.setSymbolConfiguration(Some(&size.configurationByApplyingConfiguration(&colors)));
        }
    }
}

/// The app icon at the size of an app icon in an alert, which assistive technology reads as
/// `description`.
fn app_icon(description: &str) -> Option<Retained<NSImage>> {
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(APP_ICON));
    let Some(image) = image else {
        tracing::warn!("the app icon of the setup window is unreadable");
        return None;
    };
    image.setSize(NSSize::new(APP_ICON_SIZE, APP_ICON_SIZE));
    image.setAccessibilityDescription(Some(&NSString::from_str(description)));
    Some(image)
}

/// `label` with its text in the center, and not selectable, so that the keyboard focus moves only
/// through the controls.
fn centered_label(label: Retained<NSTextField>) -> Retained<NSTextField> {
    label.setAlignment(NSTextAlignment::Center);
    label.setSelectable(false);
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_shows_the_app_icon() {
        let hero = hero(SetupStep::Welcome);

        assert!(matches!(hero, Hero::AppIcon));
    }

    #[test]
    fn app_icon_shows_at_the_size_of_an_alert_icon() {
        let icon = app_icon("TypeAlike").expect("the app icon is a PNG file");

        let size = icon.size();

        assert_eq!(size, NSSize::new(64.0, 64.0));
    }

    #[test]
    fn app_icon_is_a_bitmap_for_two_times_its_size() {
        let size = bitmap_size(APP_ICON);

        assert_eq!(size, Some((128, 128)));
    }

    /// The width and the height of the PNG file `bytes`, from its header.
    fn bitmap_size(bytes: &[u8]) -> Option<(u32, u32)> {
        let header = bytes.strip_prefix(b"\x89PNG\r\n\x1a\n")?;
        let dimension =
            |range: std::ops::Range<usize>| Some(u32::from_be_bytes(header.get(range)?.try_into().ok()?));
        Some((dimension(8..12)?, dimension(12..16)?))
    }
}
