//! Types accepted text into the focused app.

use std::fmt;
use std::iter;

use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGKeyCode};

use crate::keys::DELETE_KEY_CODE;

/// Marks events posted by TypeAlike, so its own event tap lets them through untouched.
const INJECTION_TAG: i64 = 0x4754_5950;

const MAX_UTF16_UNITS_APPS_READ_PER_EVENT: usize = 20;
const UNUSED_KEY_CODE: CGKeyCode = 0;

/// A keyboard event for accepted text could not be created.
#[derive(Debug, thiserror::Error)]
#[error("failed to create a keyboard event")]
pub(crate) struct InjectionError;

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

/// Posts a delete key press for each deleted character, then the text as Unicode key presses, to
/// whatever has keyboard focus. It creates all the events before it posts the first, so a failure
/// posts nothing.
pub(crate) fn replace_text(replacement: &Replacement) -> Result<(), InjectionError> {
    let chunks = utf16_chunks(&replacement.text);
    let deletions = iter::repeat_n((DELETE_KEY_CODE, &[][..]), replacement.deleted);
    let typing = chunks.iter().map(|chunk| (UNUSED_KEY_CODE, chunk.as_slice()));
    let presses = deletions
        .chain(typing)
        .map(|(key_code, units)| key_press(key_code, units))
        .collect::<Result<Vec<_>, _>>()?;
    for event in presses.iter().flatten() {
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
    }
    Ok(())
}

/// Whether TypeAlike posted `event`.
pub(crate) fn is_injected(event: &CGEvent) -> bool {
    CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData) == INJECTION_TAG
}

/// Creates a press and a release of `key_code`, which type `units` when they are not empty.
fn key_press(key_code: CGKeyCode, units: &[u16]) -> Result<[CFRetained<CGEvent>; 2], InjectionError> {
    let unit_count = units.len().try_into().expect("chunk length fits in UniCharCount");
    let key_event = |is_key_down| {
        let event = CGEvent::new_keyboard_event(None, key_code, is_key_down).ok_or(InjectionError)?;
        CGEvent::set_flags(Some(&event), CGEventFlags::empty());
        CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, INJECTION_TAG);
        if !units.is_empty() {
            // SAFETY: `units` is valid for the length passed.
            unsafe { CGEvent::keyboard_set_unicode_string(Some(&event), unit_count, units.as_ptr()) };
        }
        Ok(event)
    };
    Ok([key_event(true)?, key_event(false)?])
}

fn utf16_chunks(text: &str) -> Vec<Vec<u16>> {
    let mut chunks: Vec<Vec<u16>> = Vec::new();
    for character in text.chars() {
        let mut buffer = [0_u16; 2];
        let units = character.encode_utf16(&mut buffer);
        match chunks.last_mut() {
            Some(chunk) if chunk.len() + units.len() <= MAX_UTF16_UNITS_APPS_READ_PER_EVENT => {
                chunk.extend_from_slice(units);
            }
            _ => chunks.push(units.to_vec()),
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_of_a_replacement_hides_the_text() {
        let replacement = Replacement {
            deleted: 2,
            text: "secret".to_owned(),
        };

        let debug = format!("{replacement:?}");

        assert_eq!(debug, "Replacement { deleted: 2, text_chars: 6 }");
    }

    #[test]
    fn short_text_fits_one_event() {
        let chunks = utf16_chunks("egards");

        assert_eq!(chunks, ["egards".encode_utf16().collect::<Vec<_>>()]);
    }

    #[test]
    fn long_text_is_split_at_event_limit() {
        let text = "a".repeat(MAX_UTF16_UNITS_APPS_READ_PER_EVENT + 1);

        let chunk_lengths: Vec<usize> = utf16_chunks(&text).iter().map(Vec::len).collect();

        assert_eq!(chunk_lengths, [MAX_UTF16_UNITS_APPS_READ_PER_EVENT, 1]);
    }

    #[test]
    fn surrogate_pair_is_never_split() {
        let text = format!("{}😀", "a".repeat(MAX_UTF16_UNITS_APPS_READ_PER_EVENT - 1));

        let chunk_lengths: Vec<usize> = utf16_chunks(&text).iter().map(Vec::len).collect();

        assert_eq!(chunk_lengths, [MAX_UTF16_UNITS_APPS_READ_PER_EVENT - 1, 2]);
    }
}
