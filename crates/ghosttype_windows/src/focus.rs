//! What has keyboard focus: whether GhostType may suggest there, and where its caret is.

use ghosttype_core::app::Field;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetObjectW, HGDIOBJ, LOGFONTW};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ES_PASSWORD, GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowLongW, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_GETFONT,
};
use windows::core::{Owned, Result};

const EDIT_CLASS_NAME: &str = "Edit";
const MAX_CLASS_NAME_UNITS: usize = 16;
const FONT_REPLY_TIMEOUT_MS: u32 = 50;

/// Where ghost text starts, just right of the caret in physical screen pixels, and how it looks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Caret {
    pub(crate) origin: POINT,
    pub(crate) height: i32,
    /// The font face of the control that owns the caret, when the control reports one.
    pub(crate) font_face: Option<String>,
}

impl Caret {
    /// Builds a caret from its screen-space corners; `None` when it has no height.
    pub(crate) fn from_corners(
        top_left: POINT,
        bottom_right: POINT,
        font_face: Option<String>,
    ) -> Option<Self> {
        let height = bottom_right.y.checked_sub(top_left.y)?;
        (height > 0).then_some(Self {
            origin: POINT {
                x: bottom_right.x,
                y: top_left.y,
            },
            height,
            font_face,
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
        .then(|| Caret::from_corners(top_left, bottom_right, font_face(info.hwndCaret)))
        .flatten()
}

/// Reads the font face that `window` draws its text in, when the window reports one in time.
fn font_face(window: HWND) -> Option<String> {
    let mut font = 0;
    // SMTO_BLOCK keeps hook events out until the reply, so the busy runtime never drops a key.
    // SAFETY: the result pointer refers to a local.
    let replied = unsafe {
        SendMessageTimeoutW(
            window,
            WM_GETFONT,
            WPARAM(0),
            LPARAM(0),
            SMTO_BLOCK | SMTO_ABORTIFHUNG,
            FONT_REPLY_TIMEOUT_MS,
            Some(&raw mut font),
        )
    };
    if replied.0 == 0 || font == 0 {
        return None;
    }
    let mut description = LOGFONTW::default();
    // SAFETY: the output buffer is a local of exactly the size passed; a stale handle only fails.
    let written = unsafe {
        GetObjectW(
            HGDIOBJ(std::ptr::with_exposed_provenance_mut(font)),
            size_of::<LOGFONTW>()
                .try_into()
                .expect("font description size fits in i32"),
            Some((&raw mut description).cast()),
        )
    };
    if written == 0 {
        return None;
    }
    face_name(&description.lfFaceName)
}

/// Reads a face name that ends at its first nul; `None` when it is empty.
fn face_name(units: &[u16]) -> Option<String> {
    let name = units.split(|&unit| unit == 0).next().unwrap_or_default();
    (!name.is_empty()).then(|| String::from_utf16_lossy(name))
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
        let caret = Caret::from_corners(POINT { x: 100, y: 40 }, POINT { x: 102, y: 60 }, None);

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 102, y: 40 },
                height: 20,
                font_face: None,
            })
        );
    }

    #[test]
    fn caret_without_height_is_not_shown() {
        let caret = Caret::from_corners(POINT { x: 100, y: 40 }, POINT { x: 102, y: 40 }, None);

        assert_eq!(caret, None);
    }

    #[test]
    fn face_name_ends_at_first_nul() {
        let units = [0x43, 0x6F, 0x6E, 0, 0x78, 0];

        let face = face_name(&units);

        assert_eq!(face.as_deref(), Some("Con"));
    }

    #[test]
    fn empty_face_name_is_none() {
        let units = [0, 0x78, 0];

        let face = face_name(&units);

        assert_eq!(face, None);
    }
}
