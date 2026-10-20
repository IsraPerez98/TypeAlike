//! The popup menus of the tray icon and of the badge at the caret.

use std::mem;

use ghosttype_core::app::{BadgeEntry, BadgeItem, BadgeMenu};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, GetForegroundWindow, HMENU, MENU_ITEM_FLAGS, MF_CHECKED, MF_ENABLED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow, TPM_BOTTOMALIGN,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TRACK_POPUP_MENU_FLAGS, TrackPopupMenuEx,
    WM_NULL,
};
use windows::core::{BOOL, HSTRING, Owned, PCWSTR, Result};

const FIRST_ITEM_ID: usize = 1;
const NO_ITEM_ID: usize = 0;

/// Shows a menu of `labels` above `anchor`, like a tray menu over the taskbar, and returns the
/// index of the picked entry, if any.
pub(crate) fn pick(owner: HWND, labels: &[&str], anchor: POINT) -> Result<Option<usize>> {
    let menu = popup_menu()?;
    for (id, label) in (FIRST_ITEM_ID..).zip(labels) {
        append(*menu, MF_STRING, id, label)?;
    }
    track(owner, *menu, anchor, TPM_BOTTOMALIGN)
}

/// Shows the menu of the badge below `anchor`, and returns the index of the picked entry in
/// [`BadgeMenu::items`], if any. The focus then goes back to the window that had it.
pub(crate) fn pick_badge_item(owner: HWND, view: &BadgeMenu, anchor: POINT) -> Result<Option<usize>> {
    let menu = popup_menu()?;
    let mut ids = FIRST_ITEM_ID..;
    let mut next_id = move || ids.next().expect("the ids never run out");
    for entry in &view.entries {
        match entry {
            BadgeEntry::Item(item) => append_item(*menu, item, next_id())?,
            BadgeEntry::Separator => {
                // SAFETY: the menu is alive, and a separator has no text.
                unsafe { AppendMenuW(*menu, MF_SEPARATOR, NO_ITEM_ID, PCWSTR::null()) }?;
            }
            BadgeEntry::Submenu {
                label,
                items,
                is_enabled,
            } => {
                let submenu = popup_menu()?;
                for item in items {
                    append_item(*submenu, item, next_id())?;
                }
                let handle = submenu.0.expose_provenance();
                append(*menu, MF_POPUP | enabled_flag(*is_enabled), handle, label)?;
                // The menu destroys its submenus itself.
                mem::forget(submenu);
            }
        }
    }
    // SAFETY: plain call.
    let focused = unsafe { GetForegroundWindow() };
    let picked = track(owner, *menu, anchor, TPM_TOPALIGN);
    if !focused.is_invalid() {
        // Best effort: when Windows refuses, the user clicks the field again.
        // SAFETY: plain call.
        let _ = unsafe { SetForegroundWindow(focused) };
    }
    picked
}

fn popup_menu() -> Result<Owned<HMENU>> {
    // SAFETY: plain call; the menu is owned right after.
    Ok(unsafe { Owned::new(CreatePopupMenu()?) })
}

fn append_item(menu: HMENU, item: &BadgeItem, id: usize) -> Result<()> {
    let check = if item.is_checked {
        MF_CHECKED
    } else {
        MENU_ITEM_FLAGS::default()
    };
    append(
        menu,
        MF_STRING | check | enabled_flag(item.is_enabled),
        id,
        &item.label,
    )
}

fn enabled_flag(is_enabled: bool) -> MENU_ITEM_FLAGS {
    if is_enabled { MF_ENABLED } else { MF_GRAYED }
}

fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, label: &str) -> Result<()> {
    // SAFETY: the menu is alive and the label outlives the call.
    unsafe { AppendMenuW(menu, flags, id, &HSTRING::from(menu_text(label))) }
}

/// The text of a menu entry that shows `label`: a menu underlines the letter after a single `&`,
/// as in the name of an app.
fn menu_text(label: &str) -> String {
    label.replace('&', "&&")
}

/// Shows `menu` at `anchor` with `alignment`, and returns the index of the picked entry, if any.
fn track(
    owner: HWND,
    menu: HMENU,
    anchor: POINT,
    alignment: TRACK_POPUP_MENU_FLAGS,
) -> Result<Option<usize>> {
    // Best effort (KB135788): a menu dismisses on outside clicks only while its owner is foreground.
    // SAFETY: plain call on our own window.
    let _ = unsafe { SetForegroundWindow(owner) };
    let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | alignment;
    // SAFETY: menu and owner are alive; with TPM_RETURNCMD the result is the picked id, 0 for none.
    let picked = unsafe { TrackPopupMenuEx(menu, flags.0, anchor.x, anchor.y, owner, None) };
    // Completes the KB135788 workaround so the next menu opens on the first click.
    // SAFETY: plain call on our own window.
    unsafe { PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0)) }?;
    Ok(picked_index(picked))
}

fn picked_index(picked: BOOL) -> Option<usize> {
    usize::try_from(picked.0).ok()?.checked_sub(FIRST_ITEM_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_id_is_entry_index() {
        assert_eq!(picked_index(BOOL(2)), Some(1));
    }

    #[test]
    fn dismissed_menu_picks_nothing() {
        assert_eq!(picked_index(BOOL(0)), None);
    }

    #[test]
    fn menu_text_shows_each_ampersand_of_the_label() {
        assert_eq!(menu_text("Use screen text in AT&T"), "Use screen text in AT&&T");
    }
}
