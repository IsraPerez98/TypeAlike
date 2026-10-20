//! What has keyboard focus: whether GhostType may suggest there, where its caret is, the text
//! around the caret of a classic edit control, and which app and window it is in.

use std::ops::Range;
use std::ptr;

use ghosttype_core::app::Field;
use ghosttype_core::controller::FocusChange;
use ghosttype_core::desktop::{FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET, Surroundings};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetObjectW, HGDIOBJ, LOGFONTW};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{IAccessible, ObjectFromLresult};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, ES_PASSWORD, GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow,
    GetGUIThreadInfo, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, OBJID_CARET,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETFONT, WM_GETOBJECT, WM_GETTEXT,
};
use windows::core::{Interface, Owned, PWSTR, Result};

const EDIT_CLASS_NAME: &str = "Edit";
const MAX_CLASS_NAME_UNITS: usize = 16;
const REPLY_TIMEOUT_MS: u32 = 50;
/// The nul that a window writes after the text that it copies.
const NUL_UNITS: usize = 1;
/// The message that asks an edit control for its selection. It is in `Win32_UI_Controls`, a large
/// feature of the windows crate that GhostType does not enable for one message.
const EM_GETSEL: u32 = 0x00B0;
/// The state of an accessible object that does not show. It is in `Win32_UI_Controls`, for the
/// same reason as [`EM_GETSEL`].
const STATE_SYSTEM_INVISIBLE: i32 = 0x8000;
const MAX_WINDOW_TITLE_UNITS: usize = 256;
const MAX_PATH_UNITS: usize = 1024;

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

/// Locates the system caret of the focused control in the foreground window, when it has one.
pub(crate) fn system_caret() -> Option<Caret> {
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

/// Locates the caret that the caret object of the focused window in the foreground shows. Chromium
/// apps (Chrome, Edge and Electron apps such as VS Code) have no system caret, but a caret object.
/// `None` when no window has the focus, when the window has no caret object, and when the caret
/// object hides its caret, as Chromium does while the focused element takes no text. The thread
/// must be in a COM apartment.
pub(crate) fn accessible_caret() -> Option<Caret> {
    let window = foreground_thread_info()?.hwndFocus;
    if window.is_invalid() {
        return None;
    }
    let caret_object = caret_object(window)?;
    let child = VARIANT::from(CHILDID_SELF.cast_signed());
    // SAFETY: plain call on an interface of this thread.
    let state = unsafe { caret_object.get_accState(&child) }.ok()?;
    if is_hidden(i32::try_from(&state).ok()?) {
        return None;
    }
    let mut location = ObjectLocation::default();
    // SAFETY: the output pointers refer to the fields of a local.
    unsafe {
        caret_object.accLocation(
            &raw mut location.left,
            &raw mut location.top,
            &raw mut location.width,
            &raw mut location.height,
            &child,
        )
    }
    .ok()?;
    location.caret()
}

/// Gets the caret object that `window` gives, when it answers in time. `None` for a window without
/// a caret object of its own. The standard caret object of the system tells only the system caret.
fn caret_object(window: HWND) -> Option<IAccessible> {
    // `AccessibleObjectFromWindow` waits without a limit for a busy window, so ask with a timeout.
    // The id goes in the low 32 bits, as `AccessibleObjectFromWindow` sends it.
    let object_id = isize::try_from(OBJID_CARET.0.cast_unsigned()).ok()?;
    let reply = ask(window, WM_GETOBJECT, LPARAM(object_id)).filter(|&reply| reply != 0)?;
    let mut object = ptr::null_mut();
    // SAFETY: the reply comes from WM_GETOBJECT, sent with the WPARAM 0 that is passed here. The
    // IID names the interface that `object` receives, and `object` is a local.
    unsafe {
        ObjectFromLresult(
            LRESULT(reply.cast_signed()),
            &IAccessible::IID,
            WPARAM(0),
            &raw mut object,
        )
    }
    .ok()?;
    // SAFETY: a non-null `object` is an IAccessible with a reference that nothing else owns.
    (!object.is_null()).then(|| unsafe { IAccessible::from_raw(object) })
}

/// Where an accessible object is, as `accLocation` gives it, in physical screen pixels.
#[derive(Debug, Clone, Copy, Default)]
struct ObjectLocation {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl ObjectLocation {
    /// The caret that a caret object at this location shows.
    fn caret(self) -> Option<Caret> {
        let bottom_right = POINT {
            x: self.left.saturating_add(self.width),
            y: self.top.saturating_add(self.height),
        };
        Caret::from_corners(
            POINT {
                x: self.left,
                y: self.top,
            },
            bottom_right,
            None,
        )
    }
}

/// Whether `state`, the state of an accessible object, tells that the object does not show.
fn is_hidden(state: i32) -> bool {
    state & STATE_SYSTEM_INVISIBLE != 0
}

/// Reads the font face that `window` draws its text in, when the window reports one in time.
fn font_face(window: HWND) -> Option<String> {
    let font = ask(window, WM_GETFONT, LPARAM(0)).filter(|&font| font != 0)?;
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

/// Reads the text around the selection of the focused classic edit control. `None` for another
/// control, for a password field and for a control that does not answer in time.
pub(crate) fn edit_field_text() -> Option<FieldText> {
    let window = focused_edit_control().filter(|&window| !is_password_edit(window))?;
    let selection = selection_from_reply(ask(window, EM_GETSEL, LPARAM(0))?)?;
    let units = copy_text(window, selection.end.saturating_add(MAX_CHARS_AFTER_CARET))?;
    Some(cut_at_selection(&units, selection))
}

/// Reads the selection from the reply to `EM_GETSEL`: its start in the low word and its end in
/// the high word. `None` for the reply to a selection past the limit of a word.
fn selection_from_reply(reply: usize) -> Option<Range<usize>> {
    let [start_low, start_high, end_low, end_high, ..] = reply.to_le_bytes();
    let start = u16::from_le_bytes([start_low, start_high]);
    let end = u16::from_le_bytes([end_low, end_high]);
    let is_past_limit = start == u16::MAX && end == u16::MAX;
    (!is_past_limit).then(|| usize::from(start)..usize::from(end))
}

/// Cuts the text of a field, as UTF-16 `units`, around `selection`, a range of units. The text
/// before has at most [`MAX_CHARS_BEFORE_CARET`] units and the text after at most
/// [`MAX_CHARS_AFTER_CARET`].
fn cut_at_selection(units: &[u16], selection: Range<usize>) -> FieldText {
    let start = selection.start.min(units.len());
    let end = selection.end.clamp(start, units.len());
    let before_start = start.saturating_sub(MAX_CHARS_BEFORE_CARET);
    let after_end = end.saturating_add(MAX_CHARS_AFTER_CARET).min(units.len());
    FieldText {
        before_caret: whole_chars(&units[before_start..start]),
        after_caret: whole_chars(&units[end..after_end]),
    }
}

/// Decodes UTF-16 `units`, without the halves of the characters that a cut divided.
pub(crate) fn whole_chars(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .filter_map(std::result::Result::ok)
        .collect()
}

/// Sends `message` to `window`, with `lparam` as its only parameter. Returns the reply when the
/// window answers in time.
fn ask(window: HWND, message: u32, lparam: LPARAM) -> Option<usize> {
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

/// Copies at most the first `max_units` UTF-16 units of the text of `window`, when the window
/// answers in time.
fn copy_text(window: HWND, max_units: usize) -> Option<Vec<u16>> {
    let mut buffer = vec![0; max_units.saturating_add(NUL_UNITS)];
    let mut copied = 0;
    // SAFETY: the message gives the length of the buffer, which is valid for writes during the
    // call. The system copies the text across processes. The result pointer refers to a local.
    let replied = unsafe {
        SendMessageTimeoutW(
            window,
            WM_GETTEXT,
            WPARAM(buffer.len()),
            LPARAM(buffer.as_mut_ptr().expose_provenance().cast_signed()),
            SMTO_ABORTIFHUNG,
            REPLY_TIMEOUT_MS,
            Some(&raw mut copied),
        )
    };
    if replied.0 == 0 {
        return None;
    }
    buffer.truncate(copied.min(max_units));
    Some(buffer)
}

fn is_password_focused() -> bool {
    focused_edit_control().is_some_and(is_password_edit)
}

/// The focused control of the foreground window, when it is a classic edit control.
fn focused_edit_control() -> Option<HWND> {
    let window = foreground_thread_info()?.hwndFocus;
    (!window.is_invalid() && is_edit_control(window)).then_some(window)
}

fn is_password_edit(window: HWND) -> bool {
    // SAFETY: plain call.
    let style = unsafe { GetWindowLongW(window, GWL_STYLE) };
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
    fn caret_object_caret_starts_right_of_its_location() {
        let location = ObjectLocation {
            left: 636,
            top: 106,
            width: 1,
            height: 19,
        };

        let caret = location.caret();

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 637, y: 106 },
                height: 19,
                font_face: None,
            })
        );
    }

    #[test]
    fn invisible_state_hides_the_caret_object() {
        const FOCUSED: i32 = 0x4;
        let cases = [
            (0, false),
            (FOCUSED, false),
            (STATE_SYSTEM_INVISIBLE, true),
            (STATE_SYSTEM_INVISIBLE | FOCUSED, true),
        ];

        let hidden = cases.map(|(state, _)| is_hidden(state));

        assert_eq!(hidden, cases.map(|(_, is_hidden)| is_hidden));
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

    #[test]
    fn empty_face_name_is_none() {
        let units = [0, 0x78, 0];

        let face = face_name(&units);

        assert_eq!(face, None);
    }

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn edit_selection_is_read_from_low_and_high_words() {
        let selection = selection_from_reply(0x0007_0003);

        assert_eq!(selection, Some(3..7));
    }

    #[test]
    fn edit_selection_past_the_word_limit_is_unknown() {
        let replies = [usize::MAX, 0xFFFF_FFFF];

        let selections = replies.map(selection_from_reply);

        assert_eq!(selections, [None, None]);
    }

    #[test]
    fn field_text_is_cut_around_the_selection() {
        let field_text = cut_at_selection(&units("Hello big world"), 6..9);

        assert_eq!(
            field_text,
            FieldText {
                before_caret: "Hello ".to_owned(),
                after_caret: " world".to_owned(),
            }
        );
    }

    #[test]
    fn text_before_the_selection_keeps_its_end() {
        let text = format!("start{}", "a".repeat(MAX_CHARS_BEFORE_CARET));
        let caret = text.len();

        let field_text = cut_at_selection(&units(&text), caret..caret);

        assert_eq!(field_text.before_caret, "a".repeat(MAX_CHARS_BEFORE_CARET));
    }

    #[test]
    fn text_after_the_selection_keeps_its_start() {
        let text = format!("{}end", "a".repeat(MAX_CHARS_AFTER_CARET));

        let field_text = cut_at_selection(&units(&text), 0..0);

        assert_eq!(field_text.after_caret, "a".repeat(MAX_CHARS_AFTER_CARET));
    }

    #[test]
    fn selection_past_the_text_cuts_at_its_end() {
        let field_text = cut_at_selection(&units("Hello"), 8..12);

        assert_eq!(
            field_text,
            FieldText {
                before_caret: "Hello".to_owned(),
                after_caret: String::new(),
            }
        );
    }

    #[test]
    fn cut_inside_a_character_drops_its_half() {
        let text = format!("😀{}", "a".repeat(MAX_CHARS_BEFORE_CARET - 1));
        let caret = units(&text).len();

        let field_text = cut_at_selection(&units(&text), caret..caret);

        assert_eq!(field_text.before_caret, "a".repeat(MAX_CHARS_BEFORE_CARET - 1));
    }
}
