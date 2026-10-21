//! The text of a classic edit control.
//!
//! It gives the text around the selection, and whether the control hides its text as a password
//! field does.

use std::ops::Range;

use ghosttype_core::desktop::{FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    ES_PASSWORD, GWL_STYLE, GetClassNameW, GetWindowLongW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETTEXT,
};

use super::{REPLY_TIMEOUT_MS, ask, foreground_thread_state};
use crate::utf16::whole_chars;

const EDIT_CLASS_NAME: &str = "Edit";
const MAX_CLASS_NAME_UNITS: usize = 16;
/// The nul that a window writes after the text that it copies.
const NUL_UNITS: usize = 1;
/// The message that asks an edit control for its selection. It is in `Win32_UI_Controls`, a large
/// feature of the windows crate that GhostType does not enable for one message.
const EM_GETSEL: u32 = 0x00B0;

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

pub(super) fn is_password_focused() -> bool {
    focused_edit_control().is_some_and(is_password_edit)
}

/// The focused control of the foreground window, when it is a classic edit control.
fn focused_edit_control() -> Option<HWND> {
    let window = foreground_thread_state()?.hwndFocus;
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

#[cfg(test)]
mod tests {
    use super::*;

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
