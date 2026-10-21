//! The caret of the focused control in the foreground window.
//!
//! It is the system caret, else the caret that the caret object of the focused window shows. It
//! also has the font face of the control.

use std::ptr;

use ghosttype_core::desktop::TextColor;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetObjectW, HGDIOBJ, LOGFONTW};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{IAccessible, ObjectFromLresult};
use windows::Win32::UI::WindowsAndMessaging::{CHILDID_SELF, OBJID_CARET, WM_GETFONT, WM_GETOBJECT};
use windows::core::Interface;

use crate::focus::{ask, foreground_thread_info};

/// The state of an accessible object that does not show. It is in `Win32_UI_Controls`, a large
/// feature of the windows crate that GhostType does not enable for one constant.
const STATE_SYSTEM_INVISIBLE: i32 = 0x8000;

/// Where ghost text starts, just right of the caret in physical screen pixels, and how it looks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Caret {
    pub(crate) origin: POINT,
    pub(crate) height: i32,
    /// The font face of the control that owns the caret, when the control reports one.
    pub(crate) font_face: Option<String>,
    /// The color of the text before the caret, when the control tells it.
    pub(crate) text_color: Option<TextColor>,
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
            text_color: None,
        })
    }
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
                text_color: None,
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
                text_color: None,
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
    fn empty_face_name_is_none() {
        let units = [0, 0x78, 0];

        let face = face_name(&units);

        assert_eq!(face, None);
    }
}
