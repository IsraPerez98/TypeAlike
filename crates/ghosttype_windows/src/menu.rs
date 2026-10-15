//! Popup menus: the tray menu and the model menu of the settings window.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, MENU_ITEM_FLAGS, MF_CHECKED, MF_STRING, PostMessageW, SetForegroundWindow,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TPM_VERTICAL,
    TPMPARAMS, TRACK_POPUP_MENU_FLAGS, TrackPopupMenuEx, WM_NULL,
};
use windows::core::{BOOL, HSTRING, Owned, Result};

const FIRST_ITEM_ID: usize = 1;

/// Where a menu opens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MenuPlacement {
    /// Above this screen point, like a tray menu over the taskbar.
    AboveCursor(POINT),
    /// Below this screen rectangle, or above it when the screen has no room below, like a drop-down.
    Below(RECT),
}

/// Shows a menu of `labels` with the entry at `checked` marked, and returns the index of the
/// picked entry, if any.
pub(crate) fn pick(
    owner: HWND,
    labels: &[&str],
    checked: Option<usize>,
    placement: MenuPlacement,
) -> Result<Option<usize>> {
    // SAFETY: plain call; the menu is owned right after.
    let menu = unsafe { Owned::new(CreatePopupMenu()?) };
    for (index, (id, label)) in (FIRST_ITEM_ID..).zip(labels).enumerate() {
        let flags = if checked == Some(index) {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING | MENU_ITEM_FLAGS::default()
        };
        // SAFETY: the menu is alive and the label outlives the call.
        unsafe { AppendMenuW(*menu, flags, id, &HSTRING::from(*label)) }?;
    }
    // Best effort (KB135788): a menu dismisses on outside clicks only while its owner is foreground.
    // SAFETY: plain call on our own window.
    let _ = unsafe { SetForegroundWindow(owner) };
    let common = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON;
    let picked = match placement {
        MenuPlacement::AboveCursor(anchor) => track(*menu, common | TPM_BOTTOMALIGN, anchor, owner, None),
        MenuPlacement::Below(area) => {
            let exclusion = TPMPARAMS {
                cbSize: size_of::<TPMPARAMS>()
                    .try_into()
                    .expect("parameter size fits in u32"),
                rcExclude: area,
            };
            let corner = POINT {
                x: area.left,
                y: area.bottom,
            };
            let flags = common | TPM_LEFTALIGN | TPM_TOPALIGN | TPM_VERTICAL;
            track(*menu, flags, corner, owner, Some(&exclusion))
        }
    };
    // Completes the KB135788 workaround so the next menu opens on the first click.
    // SAFETY: plain call on our own window.
    unsafe { PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0)) }?;
    Ok(picked_index(picked))
}

fn track(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    flags: TRACK_POPUP_MENU_FLAGS,
    corner: POINT,
    owner: HWND,
    exclusion: Option<&TPMPARAMS>,
) -> BOOL {
    // SAFETY: menu and owner are alive; with TPM_RETURNCMD the result is the picked id, 0 for none.
    unsafe {
        TrackPopupMenuEx(
            menu,
            flags.0,
            corner.x,
            corner.y,
            owner,
            exclusion.map(std::ptr::from_ref),
        )
    }
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
