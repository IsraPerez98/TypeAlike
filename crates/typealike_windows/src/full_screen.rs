//! Tells whether the window in front shows in full screen: it fills its whole monitor and has no
//! title bar, as a game, a video or a slideshow in full screen. A maximized window keeps its title
//! bar, also when it fills the monitor because the taskbar hides itself. The windows of the shell,
//! such as the desktop and the task switcher, also fill the monitor, so they do not count.

use typealike_core::desktop::FullScreenActivity;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetShellWindow, GetWindowLongW, GetWindowRect, WINDOW_STYLE, WS_CAPTION,
};

use crate::{display, focus};

/// Reads whether the window in the foreground shows in full screen.
pub(crate) fn read() -> FullScreenActivity {
    let window = focus::foreground_hwnd();
    if window.is_invalid() || is_shell_window(window) {
        return FullScreenActivity::None;
    }
    window_shape(window).map_or(FullScreenActivity::None, |shape| shape.activity())
}

fn is_shell_window(window: HWND) -> bool {
    // SAFETY: the call takes no argument and has no precondition.
    let shell = unsafe { GetShellWindow() };
    !shell.is_invalid() && focus::process_id(shell) == focus::process_id(window)
}

fn window_shape(window: HWND) -> Option<WindowShape> {
    let mut frame = RECT::default();
    // SAFETY: the output is a local, and a window that closed only makes the call fail.
    unsafe { GetWindowRect(window, &raw mut frame) }.ok()?;
    let monitor = display::monitor_area(window).ok()?;
    Some(WindowShape {
        frame,
        monitor,
        has_title_bar: has_title_bar(window),
    })
}

fn has_title_bar(window: HWND) -> bool {
    // SAFETY: plain call. A window that closed gives 0, which has no title bar.
    let style = unsafe { GetWindowLongW(window, GWL_STYLE) };
    WINDOW_STYLE(u32::from_ne_bytes(style.to_ne_bytes())).contains(WS_CAPTION)
}

/// The place and the frame of a window, in physical screen pixels.
#[derive(Debug, Clone, Copy)]
struct WindowShape {
    frame: RECT,
    /// The whole monitor that shows the most of the window, with the taskbar.
    monitor: RECT,
    has_title_bar: bool,
}

impl WindowShape {
    fn activity(&self) -> FullScreenActivity {
        let fills_monitor = self.frame.left <= self.monitor.left
            && self.frame.top <= self.monitor.top
            && self.frame.right >= self.monitor.right
            && self.frame.bottom >= self.monitor.bottom;
        if fills_monitor && !self.has_title_bar {
            FullScreenActivity::InFront
        } else {
            FullScreenActivity::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: RECT = RECT {
        left: 0,
        top: 0,
        right: 2560,
        bottom: 1440,
    };

    fn shape(frame: RECT, has_title_bar: bool) -> WindowShape {
        WindowShape {
            frame,
            monitor: MONITOR,
            has_title_bar,
        }
    }

    #[test]
    fn window_without_a_title_bar_that_fills_its_monitor_is_in_full_screen() {
        let window = shape(MONITOR, false);

        assert_eq!(window.activity(), FullScreenActivity::InFront);
    }

    #[test]
    fn window_with_a_title_bar_that_fills_its_monitor_is_not_in_full_screen() {
        let maximized = RECT {
            left: -8,
            top: -8,
            right: 2568,
            bottom: 1448,
        };

        assert_eq!(shape(maximized, true).activity(), FullScreenActivity::None);
    }

    #[test]
    fn window_that_leaves_the_taskbar_free_is_not_in_full_screen() {
        let above_the_taskbar = RECT {
            bottom: 1392,
            ..MONITOR
        };

        assert_eq!(
            shape(above_the_taskbar, false).activity(),
            FullScreenActivity::None
        );
    }
}
