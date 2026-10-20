//! The menu that a click on the badge at the caret shows.

use ghosttype_core::app::{BadgeEntry, BadgeItem, BadgeMenu};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

use crate::labels::title_case;

const NO_KEY_EQUIVALENT: &str = "";

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
    let menu = builder.menu();
    let mut tags = 0..;
    let mut next_tag = move || tags.next().expect("the tags never run out");
    for entry in &view.entries {
        let menu_item = match entry {
            BadgeEntry::Item(item) => builder.item(item, next_tag()),
            BadgeEntry::Separator => NSMenuItem::separatorItem(main_thread),
            BadgeEntry::Submenu {
                label,
                items,
                is_enabled,
            } => {
                let submenu = builder.menu();
                for item in items {
                    submenu.addItem(&builder.item(item, next_tag()));
                }
                let menu_item = builder.titled(label, None);
                menu_item.setSubmenu(Some(&submenu));
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
    fn menu(&self) -> Retained<NSMenu> {
        let menu = NSMenu::new(self.main_thread);
        menu.setAutoenablesItems(false);
        menu
    }

    fn item(&self, item: &BadgeItem, tag: isize) -> Retained<NSMenuItem> {
        let menu_item = self.titled(&item.label, Some(self.action));
        // SAFETY: the runtime keeps the target alive for as long as the menu exists.
        unsafe { menu_item.setTarget(Some(self.target)) };
        menu_item.setTag(tag);
        menu_item.setEnabled(item.is_enabled);
        menu_item.setState(if item.is_checked {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        menu_item
    }

    fn titled(&self, label: &str, action: Option<Sel>) -> Retained<NSMenuItem> {
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
