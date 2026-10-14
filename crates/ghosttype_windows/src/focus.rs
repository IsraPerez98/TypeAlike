//! What has keyboard focus: whether GhostType may suggest there, and where its caret is.

use ghosttype_core::app::Field;
use windows::Win32::Foundation::{HANDLE, HWND, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ES_PASSWORD, GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowLongW, GetWindowThreadProcessId,
};
use windows::core::{Owned, Result};

const EDIT_CLASS_NAME: &str = "Edit";
const MAX_CLASS_NAME_UNITS: usize = 16;

/// Where ghost text starts: just right of the caret, in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Caret {
    pub(crate) origin: POINT,
    pub(crate) height: i32,
}

impl Caret {
    /// Builds a caret from its screen-space corners; `None` when it has no height.
    pub(crate) fn from_corners(top_left: POINT, bottom_right: POINT) -> Option<Self> {
        let height = bottom_right.y.checked_sub(top_left.y)?;
        (height > 0).then_some(Self {
            origin: POINT {
                x: bottom_right.x,
                y: top_left.y,
            },
            height,
        })
    }
}

/// Whether the foreground window changed since it was last observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusChange {
    Unchanged,
    Changed,
}

/// Follows the foreground window and classifies whether GhostType may observe and type into it.
#[derive(Debug)]
pub(crate) struct FocusTracker {
    foreground: HWND,
    is_foreground_eligible: bool,
    is_self_elevated: bool,
}

impl FocusTracker {
    pub(crate) fn new() -> Self {
        // SAFETY: plain call returning a pseudo handle that needs no closing.
        let is_self_elevated = is_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false);
        Self {
            foreground: HWND::default(),
            is_foreground_eligible: false,
            is_self_elevated,
        }
    }

    /// Observes the foreground window; returns whether it changed since the last call.
    pub(crate) fn refresh(&mut self) -> FocusChange {
        // SAFETY: plain call.
        let foreground = unsafe { GetForegroundWindow() };
        if foreground == self.foreground {
            return FocusChange::Unchanged;
        }
        self.foreground = foreground;
        self.is_foreground_eligible = self.is_eligible_window(foreground);
        FocusChange::Changed
    }

    /// Classifies the focused control right now: excluded for passwords and ineligible windows.
    pub(crate) fn field(&self) -> Field {
        if self.is_foreground_eligible && !is_password_focused() {
            Field::Open
        } else {
            Field::Excluded
        }
    }

    fn is_eligible_window(&self, window: HWND) -> bool {
        let mut process_id = 0;
        // SAFETY: the output pointer refers to a local.
        unsafe { GetWindowThreadProcessId(window, Some(&raw mut process_id)) };
        // SAFETY: plain call.
        let is_own_process = process_id == unsafe { GetCurrentProcessId() };
        let has_owner = process_id != 0;
        if !has_owner || is_own_process {
            return false;
        }
        self.is_self_elevated || !is_process_elevated(process_id).unwrap_or(true)
    }
}

/// Locates the caret of the focused control in the foreground window, when it exposes one.
pub(crate) fn caret() -> Option<Caret> {
    let info = foreground_thread_info()?;
    if info.hwndCaret.is_invalid() {
        return None;
    }
    let mut top_left = POINT {
        x: info.rcCaret.left,
        y: info.rcCaret.top,
    };
    let mut bottom_right = POINT {
        x: info.rcCaret.right,
        y: info.rcCaret.bottom,
    };
    // SAFETY: the caret window comes from the system and the points are locals.
    let is_mapped = unsafe {
        ClientToScreen(info.hwndCaret, &raw mut top_left).as_bool()
            && ClientToScreen(info.hwndCaret, &raw mut bottom_right).as_bool()
    };
    is_mapped
        .then(|| Caret::from_corners(top_left, bottom_right))
        .flatten()
}

fn is_password_focused() -> bool {
    let Some(info) = foreground_thread_info() else {
        return false;
    };
    if info.hwndFocus.is_invalid() || !is_edit_control(info.hwndFocus) {
        return false;
    }
    // SAFETY: plain call.
    let style = unsafe { GetWindowLongW(info.hwndFocus, GWL_STYLE) };
    style & ES_PASSWORD != 0
}

fn is_edit_control(window: HWND) -> bool {
    let mut units = [0_u16; MAX_CLASS_NAME_UNITS];
    // SAFETY: the buffer is valid for the call.
    let written = unsafe { GetClassNameW(window, &mut units) };
    let name = usize::try_from(written)
        .ok()
        .and_then(|written| units.get(..written))
        .unwrap_or_default();
    String::from_utf16_lossy(name).eq_ignore_ascii_case(EDIT_CLASS_NAME)
}

fn foreground_thread_info() -> Option<GUITHREADINFO> {
    // SAFETY: plain calls; a null foreground window yields thread 0, which GetGUIThreadInfo rejects.
    let thread = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), None) };
    let mut info = GUITHREADINFO {
        cbSize: size_of::<GUITHREADINFO>()
            .try_into()
            .expect("info size fits in u32"),
        ..GUITHREADINFO::default()
    };
    // SAFETY: `info` is a local with its size set.
    unsafe { GetGUIThreadInfo(thread, &raw mut info) }.ok()?;
    Some(info)
}

fn is_process_elevated(process_id: u32) -> Result<bool> {
    // SAFETY: plain call; the handle is owned right after.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }?;
    // SAFETY: OpenProcess returned a valid handle that nothing else owns.
    let process = unsafe { Owned::new(process) };
    is_elevated(*process)
}

fn is_elevated(process: HANDLE) -> Result<bool> {
    let mut token = HANDLE::default();
    // SAFETY: `process` is valid for the call and the output is a local.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }?;
    // SAFETY: OpenProcessToken returned a valid handle that nothing else owns.
    let token = unsafe { Owned::new(token) };
    let mut elevation = TOKEN_ELEVATION::default();
    let mut written = 0;
    // SAFETY: the output buffer is a local of exactly the size passed.
    unsafe {
        GetTokenInformation(
            *token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>()
                .try_into()
                .expect("elevation size fits in u32"),
            &raw mut written,
        )
    }?;
    Ok(elevation.TokenIsElevated != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_starts_right_of_caret_box() {
        let caret = Caret::from_corners(POINT { x: 100, y: 40 }, POINT { x: 102, y: 60 });

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 102, y: 40 },
                height: 20
            })
        );
    }

    #[test]
    fn caret_without_height_is_not_shown() {
        let caret = Caret::from_corners(POINT { x: 100, y: 40 }, POINT { x: 102, y: 40 });

        assert_eq!(caret, None);
    }
}
