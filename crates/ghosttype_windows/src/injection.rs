//! Types accepted text into the focused app.

use std::fmt;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    SendInput, VIRTUAL_KEY, VK_BACK,
};
use windows::core::{Error, Result};

/// Marks input sent by GhostType, so its own hook can let it through untouched.
pub(crate) const INJECTION_TAG: usize = 0x4754_5950;

/// Text to type at the caret, after the deletion of characters before it. `Debug` shows only the
/// length of the text, so logs never hold the text.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Replacement {
    /// The characters to delete before the caret.
    pub(crate) deleted: usize,
    /// The text to type after the deletion.
    pub(crate) text: String,
}

impl fmt::Debug for Replacement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Replacement")
            .field("deleted", &self.deleted)
            .field("text_chars", &self.text.chars().count())
            .finish()
    }
}

/// Sends a Backspace key press for each deleted character, then the text as Unicode key presses,
/// to whatever has keyboard focus.
pub(crate) fn replace_text(replacement: &Replacement) -> Result<()> {
    let inputs = replacement_inputs(replacement);
    let input_size = size_of::<INPUT>().try_into().expect("input size fits in i32");
    // SAFETY: the inputs are fully initialized keyboard events.
    let sent = unsafe { SendInput(&inputs, input_size) };
    if usize::try_from(sent) == Ok(inputs.len()) {
        Ok(())
    } else {
        Err(Error::from_thread())
    }
}

fn replacement_inputs(replacement: &Replacement) -> Vec<INPUT> {
    let deletions = (0..replacement.deleted).flat_map(|_| {
        [
            key_input(VK_BACK, 0, KEYBD_EVENT_FLAGS(0)),
            key_input(VK_BACK, 0, KEYEVENTF_KEYUP),
        ]
    });
    let units = replacement.text.encode_utf16().flat_map(|unit| {
        [
            key_input(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE),
            key_input(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
        ]
    });
    deletions.chain(units).collect()
}

fn key_input(key: VIRTUAL_KEY, unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTION_TAG,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacement(deleted: usize, text: &str) -> Replacement {
        Replacement {
            deleted,
            text: text.to_owned(),
        }
    }

    #[test]
    fn each_utf16_unit_is_pressed_and_released() {
        let inputs = replacement_inputs(&replacement(0, "é😀"));

        assert_eq!(inputs.len(), 6);
    }

    #[test]
    fn deletions_press_and_release_backspace_before_the_text() {
        let inputs = replacement_inputs(&replacement(2, "ab"));

        let keys = inputs
            .iter()
            // SAFETY: each input is a keyboard input, so `ki` is the field that holds it.
            .map(|input| unsafe { input.Anonymous.ki.wVk })
            .collect::<Vec<_>>();
        let unicode = VIRTUAL_KEY(0);
        assert_eq!(
            keys,
            [
                VK_BACK, VK_BACK, VK_BACK, VK_BACK, unicode, unicode, unicode, unicode
            ]
        );
    }
}
