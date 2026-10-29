//! The place and the frame of the setup window. It opens in the center of the main monitor, and the
//! user can move it but not resize or maximize it.

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetWindowLongW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetWindowLongW, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WS_MAXIMIZEBOX, WS_OVERLAPPEDWINDOW,
    WS_THICKFRAME,
};
use windows::core::{Error, Result};
use windows_reactor::{ScreenPoint, WindowHandle, WindowVisuals};

use crate::display::{DEFAULT_DPI, monitor_dpi, to_pixels, work_area};

/// A point on the main monitor, because Windows puts the top left corner of the main monitor at the
/// origin of the screen.
const MAIN_MONITOR: POINT = POINT { x: 0, y: 0 };

/// A width and a height, in DIPs or in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Size {
    pub(super) width: i32,
    pub(super) height: i32,
}

/// The frame of a window around its client area, in physical pixels. The content of the window
/// covers the title bar, so the top of the frame shows, and the frame at the sides and at the bottom
/// is the invisible frame that resizes the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frame {
    side: i32,
    top: i32,
}

/// Where a window opens and the size of its client area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct WindowPlace {
    /// The top left corner of the window in physical pixels, or `None` to let Windows place it.
    origin: Option<ScreenPoint>,
    width: f64,
    height: f64,
}

impl WindowPlace {
    /// A window with a client area of `size` DIPs in the center of the work area of the main
    /// monitor. A work area that is too small for that size gets a smaller window, so that the
    /// whole window shows. When the work area is not known, the window keeps its size and Windows
    /// places it.
    pub(super) fn centered(size: Size) -> Self {
        let in_dips = |pixels: i32, dpi: i32| f64::from(pixels) * f64::from(DEFAULT_DPI) / f64::from(dpi);
        let dpi = monitor_dpi(MAIN_MONITOR);
        let place = work_area(MAIN_MONITOR).and_then(|area| Ok((area, standard_frame(dpi)?)));
        let (area, frame) = match place {
            Ok(place) => place,
            Err(error) => {
                tracing::warn!(%error, "failed to read the work area of the main monitor");
                return Self {
                    origin: None,
                    width: f64::from(size.width),
                    height: f64::from(size.height),
                };
            }
        };
        let pixels = Size {
            width: to_pixels(size.width, dpi),
            height: to_pixels(size.height, dpi),
        };
        let (origin, fitted) = centered_in(area, pixels, frame);
        Self {
            origin: Some(ScreenPoint::new(origin.x, origin.y)),
            width: in_dips(fitted.width, dpi),
            height: in_dips(fitted.height, dpi),
        }
    }

    /// `visuals` with this size of the client area and, when it is known, this place.
    pub(super) fn applied_to(self, visuals: WindowVisuals) -> WindowVisuals {
        let visuals = visuals.client_size(self.width, self.height);
        match self.origin {
            Some(origin) => visuals.initial_position(origin),
            None => visuals,
        }
    }
}

/// Removes the frame that resizes `window` and its maximize button, so that the user can only move
/// it.
pub(super) fn fix_size(window: &WindowHandle<'_>) -> Result<()> {
    let window = HWND(window.as_raw());
    // SAFETY: the call takes no pointer, and reactor gives the handle of the open window.
    let style = unsafe { GetWindowLongW(window, GWL_STYLE) };
    if style == 0 {
        return Err(Error::from_thread());
    }
    let fixed = without_resizing(WINDOW_STYLE(u32::from_ne_bytes(style.to_ne_bytes())));
    // SAFETY: as above. A style without a frame and a maximize box is valid for any window.
    let previous = unsafe { SetWindowLongW(window, GWL_STYLE, i32::from_ne_bytes(fixed.0.to_ne_bytes())) };
    if previous == 0 {
        return Err(Error::from_thread());
    }
    let unchanged = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
    // SAFETY: as above. The call only makes Windows draw the frame of the new style.
    unsafe { SetWindowPos(window, None, 0, 0, 0, 0, unchanged | SWP_FRAMECHANGED) }
}

/// The frame that Windows gives a window with a title bar that the user can resize, as the setup
/// window has when it opens, on a monitor of `dpi`.
fn standard_frame(dpi: i32) -> Result<Frame> {
    let dpi = u32::try_from(dpi).unwrap_or(DEFAULT_DPI.unsigned_abs());
    let mut rect = RECT::default();
    // SAFETY: `rect` is a local.
    unsafe { AdjustWindowRectExForDpi(&raw mut rect, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0), dpi) }?;
    Ok(Frame {
        side: -rect.left,
        top: -rect.top,
    })
}

/// The top left corner of a window with `frame` whose client area of `size` pixels and title bar
/// are in the center of `area`, and the size of that client area, lower when `area` is too small.
fn centered_in(area: RECT, size: Size, frame: Frame) -> (POINT, Size) {
    let area_width = area.right - area.left;
    let area_height = area.bottom - area.top;
    let fitted = Size {
        width: size.width.min(area_width),
        height: size.height.min(area_height - frame.top),
    };
    let origin = POINT {
        x: area.left + (area_width - fitted.width) / 2 - frame.side,
        y: area.top + (area_height - fitted.height - frame.top) / 2,
    };
    (origin, fitted)
}

fn without_resizing(style: WINDOW_STYLE) -> WINDOW_STYLE {
    style & !(WS_THICKFRAME | WS_MAXIMIZEBOX)
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU};

    use super::*;

    const WINDOW: Size = Size {
        width: 720,
        height: 660,
    };
    const FRAME: Frame = Frame { side: 8, top: 31 };

    #[test]
    fn window_opens_in_the_center_of_the_work_area() {
        let cases = [
            (
                RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1040,
                },
                POINT { x: 592, y: 174 },
            ),
            (
                RECT {
                    left: 0,
                    top: 40,
                    right: 1920,
                    bottom: 1080,
                },
                POINT { x: 592, y: 214 },
            ),
            (
                RECT {
                    left: 0,
                    top: 0,
                    right: 2560,
                    bottom: 1392,
                },
                POINT { x: 912, y: 350 },
            ),
        ];

        let places = cases.map(|(area, _)| centered_in(area, WINDOW, FRAME));

        assert_eq!(places, cases.map(|(_, origin)| (origin, WINDOW)));
    }

    #[test]
    fn small_work_area_gets_a_lower_window_that_shows_whole() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 1366,
            bottom: 728,
        };
        let large = Size {
            width: 1080,
            height: 990,
        };

        let place = centered_in(area, large, Frame { side: 11, top: 45 });

        assert_eq!(
            place,
            (
                POINT { x: 132, y: 0 },
                Size {
                    width: 1080,
                    height: 683
                }
            )
        );
    }

    #[test]
    fn fixed_window_keeps_its_title_bar_and_minimize_button() {
        let style = without_resizing(WS_OVERLAPPEDWINDOW);

        assert_eq!(style, WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX);
    }
}
