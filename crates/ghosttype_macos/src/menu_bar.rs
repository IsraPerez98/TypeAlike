//! The menu bar extra and its menu.

use ghosttype_core::app::{TrayCommand, TrayItem, TrayView};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::NSString;

use crate::labels::title_case;
use crate::symbol::symbol_image;

const SETTINGS_KEY_EQUIVALENT: &str = ",";
const QUIT_KEY_EQUIVALENT: &str = "q";
const NO_KEY_EQUIVALENT: &str = "";

/// The extra in the menu bar, removed on drop.
#[derive(Debug)]
pub(crate) struct MenuBarItem {
    status_item: Retained<NSStatusItem>,
    target: Retained<NSObject>,
    action: Sel,
    main_thread: MainThreadMarker,
}

impl MenuBarItem {
    /// Adds the extra for `view`.
    ///
    /// Picking an entry sends `action` to `target` from a menu item whose tag is the entry's
    /// index in `view.items`; see [`command_for_tag`].
    pub(crate) fn add(
        view: &TrayView,
        target: Retained<NSObject>,
        action: Sel,
        main_thread: MainThreadMarker,
    ) -> Self {
        let status_item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        let item = Self {
            status_item,
            target,
            action,
            main_thread,
        };
        item.update(view);
        item
    }

    /// Re-renders the glyph, tooltip and menu for `view`.
    pub(crate) fn update(&self, view: &TrayView) {
        if let Some(button) = self.status_item.button(self.main_thread) {
            let image = symbol_image(view.status, view.tooltip);
            if image.is_none() {
                tracing::warn!(status = ?view.status, "menu bar symbol is unavailable");
            }
            button.setImage(image.as_deref());
            button.setToolTip(Some(&NSString::from_str(view.tooltip)));
        }
        self.status_item.setMenu(Some(&self.build_menu(&view.items)));
    }

    fn build_menu(&self, items: &[TrayItem]) -> Retained<NSMenu> {
        let menu = NSMenu::new(self.main_thread);
        for (tag, item) in (0..).zip(items) {
            if tag > 0 && item.command == TrayCommand::OpenSettings {
                menu.addItem(&NSMenuItem::separatorItem(self.main_thread));
            }
            menu.addItem(&self.build_menu_item(item, tag));
        }
        menu
    }

    fn build_menu_item(&self, item: &TrayItem, tag: isize) -> Retained<NSMenuItem> {
        // SAFETY: the action names a method the target implements, taking the sender.
        let menu_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.main_thread),
                &NSString::from_str(&title_case(item.label)),
                Some(self.action),
                &NSString::from_str(key_equivalent(item.command)),
            )
        };
        // SAFETY: this type keeps the target alive for as long as the menu exists.
        unsafe { menu_item.setTarget(Some(&self.target)) };
        menu_item.setTag(tag);
        menu_item
    }
}

impl Drop for MenuBarItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.status_item);
    }
}

/// Returns the command of the entry a menu item with `tag` was built for.
pub(crate) fn command_for_tag(items: &[TrayItem], tag: isize) -> Option<TrayCommand> {
    let index = usize::try_from(tag).ok()?;
    items.get(index).map(|item| item.command)
}

fn key_equivalent(command: TrayCommand) -> &'static str {
    match command {
        TrayCommand::OpenSettings => SETTINGS_KEY_EQUIVALENT,
        TrayCommand::Quit => QUIT_KEY_EQUIVALENT,
        TrayCommand::TogglePause => NO_KEY_EQUIVALENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: [TrayItem; 2] = [
        TrayItem {
            label: "Pause suggestions",
            command: TrayCommand::TogglePause,
        },
        TrayItem {
            label: "Quit GhostType",
            command: TrayCommand::Quit,
        },
    ];

    #[test]
    fn tag_maps_to_item_command() {
        assert_eq!(command_for_tag(&ITEMS, 1), Some(TrayCommand::Quit));
    }

    #[test]
    fn tag_past_last_item_maps_to_nothing() {
        assert_eq!(command_for_tag(&ITEMS, 2), None);
    }

    #[test]
    fn negative_tag_maps_to_nothing() {
        assert_eq!(command_for_tag(&ITEMS, -1), None);
    }

    #[test]
    fn quit_has_command_q() {
        assert_eq!(key_equivalent(TrayCommand::Quit), "q");
    }

    #[test]
    fn settings_has_command_comma() {
        assert_eq!(key_equivalent(TrayCommand::OpenSettings), ",");
    }

    #[test]
    fn pause_has_no_key_equivalent() {
        assert_eq!(key_equivalent(TrayCommand::TogglePause), "");
    }
}
