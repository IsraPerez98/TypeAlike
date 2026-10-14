//! Types accepted text into the focused app.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    SendInput, VIRTUAL_KEY,
};
use windows::core::{Error, Result};

/// Marks input sent by GhostType, so its own hook can let it through untouched.
pub(crate) const INJECTION_TAG: usize = 0x4754_5950;

/// Sends `text` as Unicode key presses to whatever has keyboard focus.
pub(crate) fn type_text(text: &str) -> Result<()> {
    let inputs = unicode_inputs(text);
    let input_size = size_of::<INPUT>().try_into().expect("input size fits in i32");
    // SAFETY: the inputs are fully initialized keyboard events.
    let sent = unsafe { SendInput(&inputs, input_size) };
    if usize::try_from(sent) == Ok(inputs.len()) {
        Ok(())
    } else {
        Err(Error::from_thread())
    }
}

fn unicode_inputs(text: &str) -> Vec<INPUT> {
    text.encode_utf16()
        .flat_map(|unit| {
            [
                unicode_input(unit, KEYEVENTF_UNICODE),
                unicode_input(unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        })
        .collect()
}

fn unicode_input(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
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

    #[test]
    fn each_utf16_unit_is_pressed_and_released() {
        let inputs = unicode_inputs("é😀");

        assert_eq!(inputs.len(), 6);
    }
}
