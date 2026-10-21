//! What has keyboard focus: whether GhostType may suggest there, the text around the caret of a
//! classic edit control, and which app and window it is in.

mod edit_control;

use ghosttype_core::controller::FocusChange;
use ghosttype_core::desktop::{Field, Surroundings};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowTextW, GetWindowThreadProcessId,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW,
};
use windows::core::{Owned, PWSTR, Result};

pub(crate) use self::edit_control::edit_field_text;
use self::edit_control::is_password_focused;

const REPLY_TIMEOUT_MS: u32 = 50;
const MAX_WINDOW_TITLE_UNITS: usize = 256;
const MAX_PATH_UNITS: usize = 1024;

/// A window, as a value that can go to another thread, which an `HWND` cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowId(isize);

impl From<HWND> for WindowId {
    fn from(window: HWND) -> Self {
        Self(window.0.expose_provenance().cast_signed())
    }
}

/// Follows the foreground window and classifies whether GhostType may observe and type into it.
#[derive(Debug)]
pub(crate) struct FocusTracker {
    foreground: HWND,
    foreground_app_name: Option<String>,
    foreground_app_id: Option<String>,
    foreground_app_path: Option<String>,
    is_foreground_eligible: bool,
    is_self_elevated: bool,
}

impl FocusTracker {
    pub(crate) fn new() -> Self {
        // SAFETY: plain call returning a pseudo handle that needs no closing.
        let is_self_elevated = is_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false);
        Self {
            foreground: HWND::default(),
            foreground_app_name: None,
            foreground_app_id: None,
            foreground_app_path: None,
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
        let process_id = process_id(foreground);
        self.is_foreground_eligible = self.is_eligible_process(process_id);
        let image_path = process_image_path(process_id).ok();
        self.foreground_app_name = image_path.as_deref().and_then(app_name);
        self.foreground_app_id = image_path
            .as_deref()
            .and_then(program_file_name)
            .map(str::to_owned);
        self.foreground_app_path = image_path;
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

    /// The foreground window when [`refresh`](Self::refresh) looked last.
    pub(crate) fn foreground(&self) -> WindowId {
        self.foreground.into()
    }

    /// Reads the app and the title of the foreground window. The focused field is for the caller
    /// to read.
    pub(crate) fn surroundings(&self) -> Surroundings {
        Surroundings {
            app_name: self.foreground_app_name.clone(),
            app_id: self.foreground_app_id.clone(),
            app_path: self.foreground_app_path.clone(),
            window_title: window_title(self.foreground),
            ..Surroundings::default()
        }
    }

    fn is_eligible_process(&self, process_id: u32) -> bool {
        // SAFETY: plain call.
        let is_own_process = process_id == unsafe { GetCurrentProcessId() };
        let has_owner = process_id != 0;
        if !has_owner || is_own_process {
            return false;
        }
        self.is_self_elevated || !is_process_elevated(process_id).unwrap_or(true)
    }
}

/// The window in the foreground now.
pub(crate) fn foreground_window() -> WindowId {
    // SAFETY: plain call.
    unsafe { GetForegroundWindow() }.into()
}

fn process_id(window: HWND) -> u32 {
    let mut process_id = 0;
    // SAFETY: the output pointer refers to a local.
    unsafe { GetWindowThreadProcessId(window, Some(&raw mut process_id)) };
    process_id
}

/// Reads the title of `window`. For a window of another process, the system reads the title
/// itself and sends no message to that process.
fn window_title(window: HWND) -> Option<String> {
    let mut units = [0_u16; MAX_WINDOW_TITLE_UNITS];
    // SAFETY: the buffer is valid for the call.
    let written = unsafe { GetWindowTextW(window, &mut units) };
    let title = units.get(..usize::try_from(written).ok()?)?;
    (!title.is_empty()).then(|| String::from_utf16_lossy(title))
}

fn process_image_path(process_id: u32) -> Result<String> {
    // SAFETY: plain call. The handle is owned right after.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }?;
    // SAFETY: OpenProcess returned a valid handle that nothing else owns.
    let process = unsafe { Owned::new(process) };
    let mut units = [0_u16; MAX_PATH_UNITS];
    let mut length = u32::try_from(units.len()).expect("the path buffer size fits in u32");
    // SAFETY: the buffer and its length are locals of the length passed.
    unsafe {
        QueryFullProcessImageNameW(
            *process,
            PROCESS_NAME_WIN32,
            PWSTR(units.as_mut_ptr()),
            &raw mut length,
        )
    }?;
    let path = units
        .get(..usize::try_from(length).unwrap_or_default())
        .unwrap_or_default();
    Ok(String::from_utf16_lossy(path))
}

/// Names an app by the file name of its program, without the extension.
fn app_name(image_path: &str) -> Option<String> {
    let file_name = program_file_name(image_path)?;
    let name = file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem);
    (!name.is_empty()).then(|| name.to_owned())
}

fn program_file_name(image_path: &str) -> Option<&str> {
    image_path
        .rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
}

/// Decodes UTF-16 `units`, without the halves of the characters that a cut divided.
pub(crate) fn whole_chars(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .filter_map(std::result::Result::ok)
        .collect()
}

/// Sends `message` to `window`, with `lparam` as its only parameter. Returns the reply when the
/// window answers in time.
pub(crate) fn ask(window: HWND, message: u32, lparam: LPARAM) -> Option<usize> {
    let mut reply = 0;
    // SAFETY: the message carries no pointer, and the result pointer refers to a local.
    let replied = unsafe {
        SendMessageTimeoutW(
            window,
            message,
            WPARAM(0),
            lparam,
            SMTO_ABORTIFHUNG,
            REPLY_TIMEOUT_MS,
            Some(&raw mut reply),
        )
    };
    (replied.0 != 0).then_some(reply)
}

pub(crate) fn foreground_thread_info() -> Option<GUITHREADINFO> {
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
    fn app_is_named_by_its_program_file() {
        let cases = [
            (r"C:\Program Files\Slack\slack.exe", Some("slack")),
            (r"C:\Windows\notepad.EXE", Some("notepad")),
            ("WINWORD", Some("WINWORD")),
            (r"C:\Apps\", None),
        ];

        let names = cases.map(|(path, _)| app_name(path));

        assert_eq!(names, cases.map(|(_, name)| name.map(str::to_owned)));
    }

    #[test]
    fn app_id_is_the_file_name_of_its_program() {
        let cases = [
            (
                r"C:\Program Files\Microsoft Office\root\Office16\OUTLOOK.EXE",
                Some("OUTLOOK.EXE"),
            ),
            ("slack.exe", Some("slack.exe")),
            (r"C:\Apps\", None),
        ];

        let ids = cases.map(|(path, _)| program_file_name(path));

        assert_eq!(ids, cases.map(|(_, id)| id));
    }
}
