//! The notification-area icon and its menu.

use std::ops::Range;

use typealike_core::app::{Status, TrayCommand, TrayItem, TrayView};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NINF_KEY, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONDATAW_0,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, HICON, LR_DEFAULTCOLOR, SM_CXSMICON, WM_APP, WM_CONTEXTMENU,
};
use windows::core::{Owned, Result};

use crate::menu;
use crate::struct_size;
use crate::system_theme::is_taskbar_light;
use crate::window::{high_word, low_word};

/// Message the shell sends to the owner window for icon clicks.
pub(crate) const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;

const ICON_ID: u32 = 1;

/// The robot of the TypeAlike logo, in black and in white. The paused robot has an empty visor
/// with a pause sign. `assets/build_icons.py` makes the files.
const ACTIVE_BLACK: &[u8] = include_bytes!("../assets/tray/active-black.ico");
const ACTIVE_WHITE: &[u8] = include_bytes!("../assets/tray/active-white.ico");
const PAUSED_BLACK: &[u8] = include_bytes!("../assets/tray/paused-black.ico");
const PAUSED_WHITE: &[u8] = include_bytes!("../assets/tray/paused-white.ico");

/// The version of the icon format that `CreateIconFromResourceEx` reads.
const ICON_FORMAT_VERSION: u32 = 0x0003_0000;
const ICON_DIRECTORY_BYTES: usize = 6;
const ICON_ENTRY_BYTES: usize = 16;
/// The width of 256 pixels, which the one byte of an icon entry gives as zero.
const LARGEST_SIDE: u32 = 256;

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
    tooltip: String,
}

impl Tray {
    /// Adds the icon for `view`, sending clicks to `window`.
    pub(crate) fn add(window: HWND, view: &TrayView) -> Result<Self> {
        let tray = Self {
            window,
            icon: render_icon(view.status)?,
            tooltip: view.tooltip.clone(),
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
        self.tooltip.clone_from(&view.tooltip);
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
            szTip: fixed_units(&self.tooltip),
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
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    let picked = menu::pick(window, &labels, anchor)?;
    Ok(picked.and_then(|index| items.get(index)).map(|item| item.command))
}

fn render_icon(status: Status) -> Result<Owned<HICON>> {
    // SAFETY: the calls take no pointer and have no precondition.
    let side = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    let color = if is_taskbar_light() {
        RobotColor::Black
    } else {
        RobotColor::White
    };
    robot_icon(status, color, side)
}

/// The color of the robot icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RobotColor {
    Black,
    White,
}

/// The robot that shows `status`, in `color`, `side` pixels wide.
pub(crate) fn robot_icon(status: Status, color: RobotColor, side: i32) -> Result<Owned<HICON>> {
    let icon_file = match (status, color) {
        (Status::Active, RobotColor::Black) => ACTIVE_BLACK,
        (Status::Active, RobotColor::White) => ACTIVE_WHITE,
        (Status::Paused, RobotColor::Black) => PAUSED_BLACK,
        (Status::Paused, RobotColor::White) => PAUSED_WHITE,
    };
    let image = best_image(icon_file, u32::try_from(side).unwrap_or(LARGEST_SIDE))
        .expect("each robot icon file has a PNG image, as a test checks");
    // SAFETY: the image is a PNG image of an icon file, and the call copies it into the new icon.
    let icon =
        unsafe { CreateIconFromResourceEx(image, true, ICON_FORMAT_VERSION, side, side, LR_DEFAULTCOLOR) }?;
    // SAFETY: CreateIconFromResourceEx returned a valid icon that nothing else owns.
    Ok(unsafe { Owned::new(icon) })
}

/// One image of an icon file: its width in pixels and the place of its bytes in the file.
struct IconEntry {
    side: u32,
    bytes: Range<usize>,
}

/// The image of `icon_file` that Windows scales the least to `side` pixels: the smallest image at
/// least `side` wide, else the largest one. `None` when the file is not a valid icon file.
fn best_image(icon_file: &[u8], side: u32) -> Option<&[u8]> {
    let count = u16::from_le_bytes(icon_file.get(4..ICON_DIRECTORY_BYTES)?.try_into().ok()?);
    let entries = (0..usize::from(count))
        .map(|index| icon_entry(icon_file, index))
        .collect::<Option<Vec<_>>>()?;
    let best = entries
        .iter()
        .filter(|entry| entry.side >= side)
        .min_by_key(|entry| entry.side)
        .or_else(|| entries.iter().max_by_key(|entry| entry.side))?;
    icon_file.get(best.bytes.clone())
}

fn icon_entry(icon_file: &[u8], index: usize) -> Option<IconEntry> {
    let start = ICON_DIRECTORY_BYTES + index * ICON_ENTRY_BYTES;
    let entry: &[u8; ICON_ENTRY_BYTES] = icon_file.get(start..start + ICON_ENTRY_BYTES)?.try_into().ok()?;
    let side = match entry[0] {
        0 => LARGEST_SIDE,
        width => u32::from(width),
    };
    let length = usize::try_from(u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]])).ok()?;
    let offset = usize::try_from(u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]])).ok()?;
    Some(IconEntry {
        side,
        bytes: offset..offset.checked_add(length)?,
    })
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
        let units: [u16; 4] = fixed_units("TypeAlike");

        assert_eq!(units, [u16::from(b'T'), u16::from(b'y'), u16::from(b'p'), 0]);
    }

    /// An icon file with one image of each of `sides`, where the image is the one byte of its side.
    fn icon_file(sides: &[u8]) -> Vec<u8> {
        let count = u16::try_from(sides.len()).unwrap();
        let mut file = [0, 1, count]
            .iter()
            .flat_map(|value: &u16| value.to_le_bytes())
            .collect::<Vec<_>>();
        let first_image = 6 + 16 * sides.len();
        for (index, &side) in sides.iter().enumerate() {
            let offset = u32::try_from(first_image + index).unwrap();
            file.extend_from_slice(&[side, side, 0, 0, 1, 0, 32, 0]);
            file.extend_from_slice(&1_u32.to_le_bytes());
            file.extend_from_slice(&offset.to_le_bytes());
        }
        file.extend_from_slice(sides);
        file
    }

    #[test]
    fn image_of_the_same_size_is_best() {
        let file = icon_file(&[16, 24, 32]);

        assert_eq!(best_image(&file, 24), Some(&[24][..]));
    }

    #[test]
    fn next_larger_image_is_best_between_sizes() {
        let file = icon_file(&[32, 16, 48]);

        assert_eq!(best_image(&file, 28), Some(&[32][..]));
    }

    #[test]
    fn largest_image_is_best_above_all_sizes() {
        let file = icon_file(&[16, 64, 32]);

        assert_eq!(best_image(&file, 96), Some(&[64][..]));
    }

    #[test]
    fn side_zero_in_the_entry_is_256() {
        let file = icon_file(&[0, 64]);

        assert_eq!(best_image(&file, 128), Some(&[0][..]));
    }

    #[test]
    fn cut_file_has_no_image() {
        let file = icon_file(&[16, 32]);

        assert_eq!(best_image(&file[..20], 16), None);
    }

    #[test]
    fn each_robot_icon_has_a_png_image() {
        let files = [ACTIVE_BLACK, ACTIVE_WHITE, PAUSED_BLACK, PAUSED_WHITE];

        let signatures = files.map(|file| best_image(file, 16).and_then(|image| image.get(1..4)));

        assert_eq!(signatures, [Some(&b"PNG"[..]); 4]);
    }
}
