//! The row of the settings pages for one thing of a list, with its accessory controls.

use ghosttype_core::settings_window::view::{self as view, Accessory};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSButton, NSColor, NSFont, NSImageView, NSProgressIndicator, NSStackView,
    NSTextField, NSView,
};
use objc2_foundation::NSString;

use crate::settings::Binder;
use crate::settings::controls::{
    body_font, detail_font, icon_button, label, push_button, set_text, show_push_button, show_status,
    spinner, start_spinning, stop_spinning, symbol, truncating_label,
};
use crate::settings::layout::{add_grouped_row, row_texts};

const OPEN_SYMBOL: &str = "chevron.right";
const CANCEL_SYMBOL: &str = "xmark.circle.fill";
const DELETE_SYMBOL: &str = "trash";
const INSTALLED_SYMBOL: &str = "checkmark.circle.fill";
const INSTALLED_DESCRIPTION: &str = "Downloaded";
const UNAVAILABLE_SYMBOL: &str = "lock.fill";
const UNAVAILABLE_DESCRIPTION: &str = "Unavailable";

/// A row of a group with one thing of a list: texts, a trailing detail and the accessory controls.
#[derive(Debug)]
pub(in crate::settings) struct ItemRow {
    title: Retained<NSTextField>,
    subtitle: Retained<NSTextField>,
    detail: Retained<NSTextField>,
    progress: Retained<NSProgressIndicator>,
    status: Retained<NSImageView>,
    primary: Retained<NSButton>,
    icon: Retained<NSButton>,
}

impl ItemRow {
    pub(super) fn add(rows_stack: &NSStackView, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let title = truncating_label(&body_font(), main_thread);
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

    pub(super) fn render(&self, shown: &view::Item, binder: &mut Binder<'_>) {
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
            Accessory::Open(button) => self.show_icon(OPEN_SYMBOL, button, subject, binder),
            Accessory::Downloading { progress, cancel } => {
                self.progress.setIndeterminate(false);
                self.progress.setDoubleValue(f64::from(progress.0));
                self.progress.setHidden(false);
                self.show_icon(CANCEL_SYMBOL, cancel, subject, binder);
            }
            Accessory::Canceling => start_spinning(&self.progress),
            Accessory::Installed { delete } => {
                show_status(
                    &self.status,
                    INSTALLED_SYMBOL,
                    INSTALLED_DESCRIPTION,
                    &NSColor::systemGreenColor(),
                );
                self.show_icon(DELETE_SYMBOL, delete, subject, binder);
            }
            Accessory::Unavailable => show_status(
                &self.status,
                UNAVAILABLE_SYMBOL,
                UNAVAILABLE_DESCRIPTION,
                &NSColor::tertiaryLabelColor(),
            ),
        }
    }

    /// Shows `button` in the icon button of the row, with the symbol `symbol_name`. The screen
    /// reader names it after its `subject`.
    fn show_icon(&self, symbol_name: &str, button: &view::Button, subject: &str, binder: &mut Binder<'_>) {
        let spoken_label = format!("{}, {subject}", button.label);
        self.icon.setImage(symbol(symbol_name, &spoken_label).as_deref());
        self.icon.setToolTip(Some(&NSString::from_str(button.label)));
        self.icon
            .setAccessibilityLabel(Some(&NSString::from_str(&spoken_label)));
        self.icon.setEnabled(button.is_enabled);
        binder.bind(&self.icon, &button.id);
        self.icon.setHidden(false);
    }
}
