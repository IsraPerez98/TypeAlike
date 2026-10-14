//! The notification-area icon and its menu.

use ghosttype_core::app::{Status, TrayCommand, TrayItem, TrayView};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::CreateBitmap;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NINF_KEY, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONDATAW_0,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, HICON, ICONINFO, MF_STRING, PostMessageW, SM_CXSMICON,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx,
    WM_APP, WM_CONTEXTMENU, WM_NULL,
};
use windows::core::{BOOL, Error, HSTRING, Owned, Result, w};

use crate::gdi::{Font, FontHeight, TextBitmap, TextLayout};
use crate::pixels::Rgb;

/// Message the shell sends to the owner window for icon clicks.
pub(crate) const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;

const ICON_ID: u32 = 1;
const FIRST_ITEM_ID: usize = 1;
const ICON_FONT_FACES: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];
const ACTIVE_GLYPH: &str = "\u{E70F}";
const PAUSED_GLYPH: &str = "\u{E769}";
const MONOCHROME_PLANES: u32 = 1;
const MONOCHROME_BITS: u32 = 1;

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

fn low_word(value: usize) -> u16 {
    u16::try_from(value & usize::from(u16::MAX)).expect("masked to 16 bits")
}

fn high_word(value: usize) -> u16 {
    low_word(value >> u16::BITS)
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
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>()
                .try_into()
                .expect("data size fits in u32"),
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
        // SAFETY: `data` is fully initialized and outlives the call.
        unsafe { Shell_NotifyIconW(message, &raw const data) }.ok()
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
    // SAFETY: plain call; the menu is owned right after.
    let menu = unsafe { Owned::new(CreatePopupMenu()?) };
    for (id, item) in (FIRST_ITEM_ID..).zip(items) {
        // SAFETY: the menu is alive and the label outlives the call.
        unsafe { AppendMenuW(*menu, MF_STRING, id, &HSTRING::from(item.label)) }?;
    }
    // Best effort (KB135788): a menu dismisses on outside clicks only while its owner is foreground.
    // SAFETY: plain call on our own window.
    let _ = unsafe { SetForegroundWindow(window) };
    let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN;
    // SAFETY: menu and window are alive; with TPM_RETURNCMD the result is the picked id, 0 for none.
    let picked = unsafe { TrackPopupMenuEx(*menu, flags.0, anchor.x, anchor.y, window, None) };
    // Completes the KB135788 workaround so the next menu opens on the first click.
    // SAFETY: plain call on our own window.
    unsafe { PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0)) }?;
    Ok(command_for_id(items, picked))
}

fn command_for_id(items: &[TrayItem], picked: BOOL) -> Option<TrayCommand> {
    let index = usize::try_from(picked.0).ok()?.checked_sub(FIRST_ITEM_ID)?;
    items.get(index).map(|item| item.command)
}

fn render_icon(status: Status) -> Result<Owned<HICON>> {
    // SAFETY: plain calls.
    let side = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    let font = Font::first_installed(&ICON_FONT_FACES, FontHeight::Em(side))?;
    let glyph = match status {
        Status::Active => ACTIVE_GLYPH,
        Status::Paused => PAUSED_GLYPH,
    };
    let color = if is_taskbar_light() {
        Rgb::BLACK
    } else {
        Rgb::WHITE
    };
    let bitmap = TextBitmap::render(glyph, &font, color, TextLayout::CenteredSquare(side))?;
    icon_from_bitmap(&bitmap)
}

fn icon_from_bitmap(bitmap: &TextBitmap) -> Result<Owned<HICON>> {
    let size = bitmap.size();
    // SAFETY: plain call; the bitmap is owned right after.
    let ignored_mask = unsafe { CreateBitmap(size.cx, size.cy, MONOCHROME_PLANES, MONOCHROME_BITS, None) };
    if ignored_mask.is_invalid() {
        return Err(Error::from_thread());
    }
    // SAFETY: CreateBitmap returned a valid bitmap that nothing else owns.
    let ignored_mask = unsafe { Owned::new(ignored_mask) };
    let info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: *ignored_mask,
        hbmColor: bitmap.handle(),
    };
    // SAFETY: both bitmaps are alive and unselected; the system copies them into the icon.
    let icon = unsafe { CreateIconIndirect(&raw const info) }?;
    // SAFETY: CreateIconIndirect returned a valid icon that nothing else owns.
    Ok(unsafe { Owned::new(icon) })
}

fn is_taskbar_light() -> bool {
    let mut value: u32 = 0;
    let mut value_size: u32 = size_of::<u32>().try_into().expect("dword size fits in u32");
    // SAFETY: the output buffer is a local of exactly the size passed.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&raw mut value_size),
        )
    };
    status.is_ok() && value != 0
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
    fn picked_id_maps_to_item_command() {
        let items = [
            TrayItem {
                label: "Pause suggestions",
                command: TrayCommand::TogglePause,
            },
            TrayItem {
                label: "Quit GhostType",
                command: TrayCommand::Quit,
            },
        ];

        assert_eq!(command_for_id(&items, BOOL(2)), Some(TrayCommand::Quit));
    }

    #[test]
    fn dismissed_menu_picks_nothing() {
        let items = [TrayItem {
            label: "Quit GhostType",
            command: TrayCommand::Quit,
        }];

        assert_eq!(command_for_id(&items, BOOL(0)), None);
    }

    #[test]
    fn long_tooltip_is_truncated_and_terminated() {
        let units: [u16; 4] = fixed_units("GhostType");

        assert_eq!(units, [u16::from(b'G'), u16::from(b'h'), u16::from(b'o'), 0]);
    }
}
