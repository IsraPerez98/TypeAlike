//! The rows of the settings pages for one app: its icon and name, with a switch or a pop-up menu.

use ghosttype_core::settings_window::view::{self as view, ControlId};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSFont, NSImage, NSImageView, NSLayoutConstraint, NSLayoutConstraintOrientation,
    NSLayoutPriorityDefaultHigh, NSLayoutPriorityDefaultLow, NSPopUpButton, NSStackView, NSSwitch,
    NSTextField, NSUserInterfaceLayoutOrientation, NSView, NSWorkspace,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSString};

use crate::labels::title_case;
use crate::settings::Binder;
use crate::settings::controls::{
    control_state, pop_up_button, rebuild_menu, set_text, symbol, truncating_label,
};
use crate::settings::layout::{ACCESSORY_SPACING, NO_INSETS, add_grouped_row, stack};

const APP_ICON_SIDE: CGFloat = 20.0;
const UNKNOWN_APP_SYMBOL: &str = "app.dashed";
const UNKNOWN_APP_DESCRIPTION: &str = "App";

/// The icon and the name of an app at the leading edge of a row.
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
pub(crate) struct AppSwitchRow {
    app: AppLabel,
    switch: Retained<NSSwitch>,
    pub(super) id: ControlId,
}

impl AppSwitchRow {
    pub(crate) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let switch = NSSwitch::new(main_thread);
        // SAFETY: the target implements the toggle action, taking the switch, and `SettingsWindow`
        // keeps the target alive as long as the switch.
        unsafe {
            switch.setTarget(Some(&binder.target.target));
            switch.setAction(Some(binder.target.toggle_action));
        }
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

/// A row with the icon and the name of an app, and a pop-up menu that picks the value of a setting
/// for the app. The menu rebuilds its entries only when they change, so an open menu stays open.
#[derive(Debug)]
pub(crate) struct AppPickerRow {
    app: AppLabel,
    menu: Retained<NSPopUpButton>,
    titles: Vec<String>,
    pub(super) id: ControlId,
}

impl AppPickerRow {
    pub(crate) fn add(rows_stack: &NSStackView, id: &ControlId, binder: &Binder<'_>) -> Self {
        let menu = pop_up_button(binder.target, binder.target.pick_action, binder.main_thread);
        let app = AppLabel::new(binder.main_thread);
        app.add_row(rows_stack, &menu, binder.main_thread);
        Self {
            app,
            menu,
            titles: Vec::new(),
            id: id.clone(),
        }
    }

    pub(super) fn render(&mut self, shown: &view::AppPicker, binder: &mut Binder<'_>) {
        self.app.render(&shown.app_id, &shown.app_name);
        self.menu
            .setAccessibilityLabel(Some(&NSString::from_str(&shown.app_name)));
        let titles: Vec<String> = shown.choices.iter().map(|choice| title_case(choice)).collect();
        if titles != self.titles {
            rebuild_menu(&self.menu, &titles);
            self.titles = titles;
        }
        let selected = isize::try_from(shown.selected).expect("a menu never holds isize::MAX entries");
        self.menu.selectItemAtIndex(selected);
        self.menu.setEnabled(shown.is_enabled);
        binder.bind(&self.menu, &shown.id);
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
