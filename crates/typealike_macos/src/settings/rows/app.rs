//! The rows of the settings pages for one app or one website: its icon and name, with a switch
//! or a pop-up menu.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSImage, NSImageView, NSLayoutConstraint, NSLayoutConstraintOrientation,
    NSLayoutPriorityDefaultHigh, NSLayoutPriorityDefaultLow, NSStackView, NSSwitch, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWorkspace,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSString};
use typealike_core::settings_window::view::{self as view, ControlId};

use crate::labels::title_case;
use crate::settings::controls::{MenuChoices, body_font, control_state, set_text, symbol, truncating_label};
use crate::settings::layout::{ACCESSORY_SPACING, NO_INSETS, add_grouped_row, stack};
use crate::settings::{Binder, ControlAction};

const APP_ICON_SIDE: CGFloat = 20.0;
const UNKNOWN_APP_SYMBOL: &str = "app.dashed";
const UNKNOWN_APP_DESCRIPTION: &str = "App";
const WEBSITE_SYMBOL: &str = "globe";
const WEBSITE_DESCRIPTION: &str = "Website";

/// The icon and the name of an app or a website at the leading edge of a row.
#[derive(Debug)]
struct AppLabel {
    texts: Retained<NSStackView>,
    icon: Retained<NSImageView>,
    name: Retained<NSTextField>,
    /// The app whose icon shows, so that a render loads the icon only for another app.
    icon_app_id: Option<String>,
}

impl AppLabel {
    fn new(main_thread: MainThreadMarker) -> Self {
        let icon = NSImageView::new(main_thread);
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            icon.widthAnchor().constraintEqualToConstant(APP_ICON_SIDE),
            icon.heightAnchor().constraintEqualToConstant(APP_ICON_SIDE),
        ]));
        let name = truncating_label(&body_font(), main_thread);
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
        Self {
            texts,
            icon,
            name,
            icon_app_id: None,
        }
    }

    /// Adds a row with the label and `control` at the bottom of `rows_stack`.
    fn add_row(&self, rows_stack: &NSStackView, control: &NSView, main_thread: MainThreadMarker) {
        control.setContentHuggingPriority_forOrientation(
            NSLayoutPriorityDefaultHigh,
            NSLayoutConstraintOrientation::Horizontal,
        );
        add_grouped_row(rows_stack, &self.texts, &[control], main_thread);
    }

    fn render(&mut self, app_id: &str, app_name: &str) {
        set_text(&self.name, app_name);
        if self.icon_app_id.as_deref() != Some(app_id) {
            self.icon.setImage(app_icon(app_id).as_deref());
            self.icon_app_id = Some(app_id.to_owned());
        }
    }
}

/// A row with the icon and the name of an app, and a switch that turns a setting on and off for
/// the app.
#[derive(Debug)]
pub(in crate::settings) struct AppSwitchRow {
    app: AppLabel,
    switch: Retained<NSSwitch>,
    pub(super) id: ControlId,
}

impl AppSwitchRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let switch = NSSwitch::new(main_thread);
        binder.target.connect(&switch, ControlAction::Toggle);
        let app = AppLabel::new(main_thread);
        app.add_row(rows_stack, &switch, main_thread);
        Self {
            app,
            switch,
            id: id.clone(),
        }
    }

    pub(super) fn render(&mut self, shown: &view::AppSwitch, binder: &mut Binder<'_>) {
        self.app.render(&shown.app_id, &shown.app_name);
        self.switch
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.app_name)));
        self.switch.setState(control_state(shown.is_on));
        self.switch.setEnabled(shown.is_enabled);
        binder.bind(&self.switch, &shown.id);
    }
}

/// A row with a globe and the host of a website, and a switch that turns a setting on and off for
/// the website.
#[derive(Debug)]
pub(in crate::settings) struct SiteSwitchRow {
    site: AppLabel,
    switch: Retained<NSSwitch>,
    pub(super) id: ControlId,
}

impl SiteSwitchRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let switch = NSSwitch::new(main_thread);
        binder.target.connect(&switch, ControlAction::Toggle);
        let site = AppLabel::new(main_thread);
        site.icon
            .setImage(symbol(WEBSITE_SYMBOL, WEBSITE_DESCRIPTION).as_deref());
        site.add_row(rows_stack, &switch, main_thread);
        Self {
            site,
            switch,
            id: id.clone(),
        }
    }

    pub(super) fn render(&mut self, shown: &view::SiteSwitch, binder: &mut Binder<'_>) {
        set_text(&self.site.name, &shown.host);
        self.switch
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.host)));
        self.switch.setState(control_state(shown.is_on));
        binder.bind(&self.switch, &shown.id);
    }
}

/// A row with the icon and the name of an app, and a pop-up menu that picks the value of a setting
/// for the app.
#[derive(Debug)]
pub(in crate::settings) struct AppPickerRow {
    app: AppLabel,
    choices: MenuChoices,
    pub(super) id: ControlId,
}

impl AppPickerRow {
    pub(super) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let choices = MenuChoices::new(binder.target, binder.main_thread);
        let app = AppLabel::new(binder.main_thread);
        app.add_row(rows_stack, &choices.menu, binder.main_thread);
        Self {
            app,
            choices,
            id: id.clone(),
        }
    }

    pub(super) fn render(&mut self, shown: &view::AppPicker, binder: &mut Binder<'_>) {
        self.app.render(&shown.app_id, &shown.app_name);
        self.choices
            .menu
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.app_name)));
        let titles: Vec<String> = shown.choices.iter().map(|choice| title_case(choice)).collect();
        self.choices.render(titles, shown.selected);
        self.choices.menu.setEnabled(shown.is_enabled);
        binder.bind(&self.choices.menu, &shown.id);
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
