//! The notification-area icon and its menu.

use ghosttype_core::app::{Status, TrayCommand, TrayItem, TrayView};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NINF_KEY, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONDATAW_0,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{HICON, SM_CXSMICON, WM_APP, WM_CONTEXTMENU};
use windows::core::{Owned, Result};

use crate::gdi::glyph_icon;
use crate::menu;
use crate::pixels::Rgb;
use crate::struct_size;
use crate::system_theme::is_taskbar_light;
use crate::window::{high_word, low_word};

/// Message the shell sends to the owner window for icon clicks.
pub(crate) const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;

const ICON_ID: u32 = 1;
const ACTIVE_GLYPH: &str = "\u{E70F}";
const PAUSED_GLYPH: &str = "\u{E769}";

/// A click on the tray icon that asks for the menu, anchored at this screen point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MenuRequest {
    pub(crate) anchor: POINT,
}

/// Reads a tray callback; `Some` when the user asked for the menu by mouse or keyboard.
pub(crate) fn menu_request(wparam: WPARAM, lparam: LPARAM) -> Option<MenuRequest> {
    let event = u32::from(low_word(lparam.0.cast_unsigned()));
    let opens_menu = matches!(event, WM_CONTEXTMENU | NIN_SELECT) || event == NIN_SELECT | NINF_KEY;
    let anchor = POINT {
        x: signed(low_word(wparam.0)),
        y: signed(high_word(wparam.0)),
    };
    opens_menu.then_some(MenuRequest { anchor })
}

fn signed(word: u16) -> i32 {
    i32::from(word.cast_signed())
}

/// The icon in the notification area, removed on drop.
#[derive(Debug)]
pub(crate) struct Tray {
    window: HWND,
    icon: Owned<HICON>,
    tooltip: &'static str,
}

impl Tray {
    /// Adds the icon for `view`, sending clicks to `window`.
    pub(crate) fn add(window: HWND, view: &TrayView) -> Result<Self> {
        let tray = Self {
            window,
            icon: render_icon(view.status)?,
            tooltip: view.tooltip,
        };
        tray.register()?;
        Ok(tray)
    }

    /// Re-adds the icon after the taskbar was recreated (Explorer restart).
    pub(crate) fn register(&self) -> Result<()> {
        self.notify(NIM_ADD)?;
        self.notify(NIM_SETVERSION)
    }

    /// Re-renders the icon and tooltip for `view` and the current taskbar theme.
    pub(crate) fn update(&mut self, view: &TrayView) -> Result<()> {
        self.icon = render_icon(view.status)?;
        self.tooltip = view.tooltip;
        self.notify(NIM_MODIFY)
    }

    fn notify(&self, message: NOTIFY_ICON_MESSAGE) -> Result<()> {
        let notification = NOTIFYICONDATAW {
            cbSize: struct_size::<NOTIFYICONDATAW>(),
            hWnd: self.window,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY_CALLBACK_MESSAGE,
            hIcon: *self.icon,
            szTip: fixed_units(self.tooltip),
            Anonymous: NOTIFYICONDATAW_0 {
                uVersion: NOTIFYICON_VERSION_4,
            },
            ..NOTIFYICONDATAW::default()
        };
        // SAFETY: `notification` is fully initialized and outlives the call.
        unsafe { Shell_NotifyIconW(message, &raw const notification) }.ok()
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Err(error) = self.notify(NIM_DELETE) {
            tracing::warn!(%error, "failed to remove tray icon");
        }
    }
}

/// Shows the tray menu at `anchor` and returns the picked command, if any.
pub(crate) fn pick_command(window: HWND, items: &[TrayItem], anchor: POINT) -> Result<Option<TrayCommand>> {
    let labels: Vec<&str> = items.iter().map(|item| item.label).collect();
    let picked = menu::pick(window, &labels, anchor)?;
    Ok(picked.and_then(|index| items.get(index)).map(|item| item.command))
}

/// The glyph of the system icon font that shows `status`.
pub(crate) fn status_glyph(status: Status) -> &'static str {
    match status {
        Status::Active => ACTIVE_GLYPH,
        Status::Paused => PAUSED_GLYPH,
    }
}

fn render_icon(status: Status) -> Result<Owned<HICON>> {
    // SAFETY: the calls take no pointer and have no precondition.
    let side = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    let color = if is_taskbar_light() {
        Rgb::BLACK
    } else {
        Rgb::WHITE
    };
    glyph_icon(status_glyph(status), side, color)
}

fn fixed_units<const LENGTH: usize>(text: &str) -> [u16; LENGTH] {
    let mut units = [0_u16; LENGTH];
    let last_text_slot = LENGTH.saturating_sub(1);
    for (slot, unit) in units.iter_mut().take(last_text_slot).zip(text.encode_utf16()) {
        *slot = unit;
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callback(event: u32, x: i16, y: i16) -> (WPARAM, LPARAM) {
        let anchor = (usize::from(y.cast_unsigned()) << 16) | usize::from(x.cast_unsigned());
        let notification = (isize::try_from(ICON_ID).unwrap() << 16) | isize::try_from(event).unwrap();
        (WPARAM(anchor), LPARAM(notification))
    }

    #[test]
    fn right_click_requests_menu_at_anchor() {
        let (wparam, lparam) = callback(WM_CONTEXTMENU, 1200, 800);

        assert_eq!(
            menu_request(wparam, lparam),
            Some(MenuRequest {
                anchor: POINT { x: 1200, y: 800 }
            })
        );
    }

    #[test]
    fn left_click_requests_menu() {
        let (wparam, lparam) = callback(NIN_SELECT, 10, 20);

        assert!(menu_request(wparam, lparam).is_some());
    }

    #[test]
    fn keyboard_select_requests_menu() {
        let (wparam, lparam) = callback(NIN_SELECT | NINF_KEY, 10, 20);

        assert!(menu_request(wparam, lparam).is_some());
    }

    #[test]
    fn anchor_on_monitor_left_of_primary_is_negative() {
        let (wparam, lparam) = callback(WM_CONTEXTMENU, -300, 500);

        assert_eq!(
            menu_request(wparam, lparam),
            Some(MenuRequest {
                anchor: POINT { x: -300, y: 500 }
            })
        );
    }

    #[test]
    fn hover_does_not_request_menu() {
        let (wparam, lparam) = callback(windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE, 10, 20);

        assert_eq!(menu_request(wparam, lparam), None);
    }

    #[test]
    fn long_tooltip_is_truncated_and_terminated() {
        let units: [u16; 4] = fixed_units("GhostType");

        assert_eq!(units, [u16::from(b'G'), u16::from(b'h'), u16::from(b'o'), 0]);
    }
}
