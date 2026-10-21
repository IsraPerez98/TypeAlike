//! The menu that a click on the badge at the caret shows.

use ghosttype_core::app::{BadgeEntry, BadgeItem, BadgeMenu};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::NSString;

use crate::labels::title_case;
use crate::menu_bar::NO_KEY_EQUIVALENT;
use crate::settings::control_state;

/// Builds the menu of `view`.
///
/// Picking an entry sends `action` to `target` from a menu item whose tag is the index of the
/// entry in [`BadgeMenu::items`].
pub(crate) fn build(
    view: &BadgeMenu,
    target: &NSObject,
    action: Sel,
    main_thread: MainThreadMarker,
) -> Retained<NSMenu> {
    let builder = MenuBuilder {
        target,
        action,
        main_thread,
    };
    let menu = builder.build_menu();
    let mut tags = 0..;
    let mut next_tag = move || tags.next().expect("the tags never run out");
    for entry in &view.entries {
        let menu_item = match entry {
            BadgeEntry::Item(item) => builder.build_item(item, next_tag()),
            BadgeEntry::Separator => NSMenuItem::separatorItem(main_thread),
            BadgeEntry::Submenu {
                label,
                items,
                is_enabled,
            } => {
                let menu_item = builder.build_submenu(label, items, &mut next_tag);
                menu_item.setEnabled(*is_enabled);
                menu_item
            }
        };
        menu.addItem(&menu_item);
    }
    menu
}

struct MenuBuilder<'target> {
    target: &'target NSObject,
    action: Sel,
    main_thread: MainThreadMarker,
}

impl MenuBuilder<'_> {
    /// A menu whose entries are enabled only as the badge menu says.
    fn build_menu(&self) -> Retained<NSMenu> {
        let menu = NSMenu::new(self.main_thread);
        menu.setAutoenablesItems(false);
        menu
    }

    fn build_item(&self, item: &BadgeItem, tag: isize) -> Retained<NSMenuItem> {
        let menu_item = self.build_titled_item(&item.label, Some(self.action));
        // SAFETY: the runtime keeps the target alive for as long as the menu exists.
        unsafe { menu_item.setTarget(Some(self.target)) };
        menu_item.setTag(tag);
        menu_item.setEnabled(item.is_enabled);
        menu_item.setState(control_state(item.is_checked));
        menu_item
    }

    /// An entry `label` that opens a submenu of `items`, which take their tags from `next_tag`.
    fn build_submenu(
        &self,
        label: &str,
        items: &[BadgeItem],
        next_tag: &mut impl FnMut() -> isize,
    ) -> Retained<NSMenuItem> {
        let submenu = self.build_menu();
        for item in items {
            submenu.addItem(&self.build_item(item, next_tag()));
        }
        let menu_item = self.build_titled_item(label, None);
        menu_item.setSubmenu(Some(&submenu));
        menu_item
    }

    fn build_titled_item(&self, label: &str, action: Option<Sel>) -> Retained<NSMenuItem> {
        // SAFETY: the action names a method the target implements, taking the sender.
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.main_thread),
                &NSString::from_str(&title_case(label)),
                action,
                &NSString::from_str(NO_KEY_EQUIVALENT),
            )
        }
    }
}
