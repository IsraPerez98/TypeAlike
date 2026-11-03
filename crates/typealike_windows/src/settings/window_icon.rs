//! The app icon in the title bar and on the taskbar button of the settings and setup windows.

use std::ptr;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_SHARED, LoadImageW, SM_CXICON, SM_CXSMICON, SYSTEM_METRICS_INDEX,
    SendMessageW, WM_SETICON,
};
use windows::core::{PCWSTR, Result};
use windows_reactor::WindowHandle;

/// The ID of the app icon in the resources of TypeAlike.exe (`crates/typealike/TypeAlike.rc`).
const APP_ICON_ID: usize = 1;

/// The two icons of a window: the big one for Alt+Tab, the small one for the title bar and the
/// taskbar button.
const WINDOW_ICONS: [(u32, SYSTEM_METRICS_INDEX); 2] = [(ICON_BIG, SM_CXICON), (ICON_SMALL, SM_CXSMICON)];

/// Gives `window` the app icon of TypeAlike.exe, at the sizes of the DPI of its monitor.
///
/// # Errors
///
/// Fails when the program has no app icon of that ID.
pub(super) fn show_app_icon(window: &WindowHandle<'_>) -> Result<()> {
    let window = HWND(window.as_raw());
    // SAFETY: the calls take no pointer, and reactor gives the handle of the open window.
    let dpi = unsafe { GetDpiForWindow(window) };
    // SAFETY: `None` asks for the module of the program, which stays loaded while it runs.
    let program = HINSTANCE::from(unsafe { GetModuleHandleW(None) }?);
    for (kind, size_metric) in WINDOW_ICONS {
        // SAFETY: the call takes no pointer.
        let side = unsafe { GetSystemMetricsForDpi(size_metric, dpi) };
        // SAFETY: an integer resource ID in place of a name is valid for LoadImageW. LR_SHARED makes
        // the system own the icon, so it stays valid for as long as the window shows it.
        let icon = unsafe {
            LoadImageW(
                Some(program),
                PCWSTR(ptr::without_provenance(APP_ICON_ID)),
                IMAGE_ICON,
                side,
                side,
                LR_SHARED,
            )
        }?;
        // SAFETY: the window and the icon are valid. WM_SETICON takes no ownership of the icon.
        unsafe {
            SendMessageW(
                window,
                WM_SETICON,
                Some(WPARAM(
                    usize::try_from(kind).expect("a u32 fits in a usize on Windows"),
                )),
                Some(LPARAM(icon.0.addr().cast_signed())),
            )
        };
    }
    Ok(())
}
