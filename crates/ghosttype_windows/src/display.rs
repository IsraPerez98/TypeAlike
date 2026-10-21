//! The monitors and the physical pixels of the screen.
//!
//! It gives the DPI and the work area of a monitor, the scale from DIPs to pixels, and the screen
//! rectangles of core.

use ghosttype_core::desktop::ScreenRect;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::Result;

use crate::struct_size;

/// The pixels in one inch at a scale of 100 %.
pub(crate) const DEFAULT_DPI: i32 = 96;

/// Reads the DPI of the monitor that shows `point`, or the default DPI when the monitor does not tell.
pub(crate) fn monitor_dpi(point: POINT) -> i32 {
    // SAFETY: the call takes no pointer, and it gives the nearest monitor for any point.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut horizontal = 0;
    let mut vertical = 0;
    // SAFETY: the outputs are locals.
    let read =
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut horizontal, &raw mut vertical) };
    read.ok()
        .and_then(|()| i32::try_from(horizontal).ok())
        .unwrap_or(DEFAULT_DPI)
}

/// Scales `dips` to physical pixels on a monitor of `dpi`.
pub(crate) fn to_pixels(dips: i32, dpi: i32) -> i32 {
    (dips * dpi + DEFAULT_DPI / 2) / DEFAULT_DPI
}

/// Reads the work area of the monitor that shows `point`: the monitor without the taskbar.
pub(crate) fn work_area(point: POINT) -> Result<RECT> {
    // SAFETY: the call takes no pointer, and it gives the nearest monitor for any point.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut areas = MONITORINFO {
        cbSize: struct_size::<MONITORINFO>(),
        ..MONITORINFO::default()
    };
    // SAFETY: `areas` is a local with its size set.
    unsafe { GetMonitorInfoW(monitor, &raw mut areas) }.ok()?;
    Ok(areas.rcWork)
}

/// The rectangle of the screen that `rect`, in physical screen pixels, covers.
pub(crate) fn screen_rect(rect: RECT) -> ScreenRect {
    ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

/// Rounds `coordinate`, in physical pixels, to the nearest whole pixel.
#[expect(
    clippy::cast_possible_truncation,
    reason = "no conversion from f64 to i32 exists without `as`, and `as` saturates"
)]
pub(crate) fn round_to_pixel(coordinate: f64) -> i32 {
    coordinate.round() as i32
}
