//! The app's main menu. It stays hidden while TypeAlike runs as an accessory app, but AppKit still
//! matches its key equivalents, so text editing and Close work in TypeAlike's own windows.

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

const EDIT_MENU_TITLE: &str = "Edit";
const WINDOW_MENU_TITLE: &str = "Window";

/// Installs a main menu with the standard Edit commands and Close Window.
pub(crate) fn install(main_thread: MainThreadMarker) {
    let edit_items = [
        ("Undo", sel!(undo:), "z"),
        ("Redo", sel!(redo:), "Z"),
        ("Cut", sel!(cut:), "x"),
        ("Copy", sel!(copy:), "c"),
        ("Paste", sel!(paste:), "v"),
        ("Select All", sel!(selectAll:), "a"),
    ];
    let window_items = [("Close", sel!(performClose:), "w")];
    let main_menu = NSMenu::new(main_thread);
    main_menu.addItem(&submenu_item(EDIT_MENU_TITLE, &edit_items, main_thread));
    main_menu.addItem(&submenu_item(WINDOW_MENU_TITLE, &window_items, main_thread));
    NSApplication::sharedApplication(main_thread).setMainMenu(Some(&main_menu));
}

fn submenu_item(
    title: &str,
    items: &[(&str, Sel, &str)],
    main_thread: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let title = NSString::from_str(title);
    let submenu = NSMenu::initWithTitle(NSMenu::alloc(main_thread), &title);
    for &(item_title, action, key_equivalent) in items {
        // SAFETY: the action is a standard responder method; with no target, AppKit sends it to
        // the first responder that implements it.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(main_thread),
                &NSString::from_str(item_title),
                Some(action),
                &NSString::from_str(key_equivalent),
            )
        };
        submenu.addItem(&item);
    }
    let item = NSMenuItem::new(main_thread);
    item.setTitle(&title);
    item.setSubmenu(Some(&submenu));
    item
}
