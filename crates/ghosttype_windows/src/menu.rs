//! The popup menu of the tray icon.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, MF_STRING, PostMessageW, SetForegroundWindow, TPM_BOTTOMALIGN,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_NULL,
};
use windows::core::{BOOL, HSTRING, Owned, Result};

const FIRST_ITEM_ID: usize = 1;

/// Shows a menu of `labels` above `anchor`, like a tray menu over the taskbar, and returns the
/// index of the picked entry, if any.
pub(crate) fn pick(owner: HWND, labels: &[&str], anchor: POINT) -> Result<Option<usize>> {
    // SAFETY: plain call; the menu is owned right after.
    let menu = unsafe { Owned::new(CreatePopupMenu()?) };
    for (id, label) in (FIRST_ITEM_ID..).zip(labels) {
        // SAFETY: the menu is alive and the label outlives the call.
        unsafe { AppendMenuW(*menu, MF_STRING, id, &HSTRING::from(*label)) }?;
    }
    // Best effort (KB135788): a menu dismisses on outside clicks only while its owner is foreground.
    // SAFETY: plain call on our own window.
    let _ = unsafe { SetForegroundWindow(owner) };
    let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN;
    // SAFETY: menu and owner are alive; with TPM_RETURNCMD the result is the picked id, 0 for none.
    let picked = unsafe { TrackPopupMenuEx(*menu, flags.0, anchor.x, anchor.y, owner, None) };
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
}
