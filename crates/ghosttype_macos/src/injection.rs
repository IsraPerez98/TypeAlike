//! Types accepted text into the focused app.

use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGKeyCode};

use crate::keys::DELETE_KEY_CODE;

/// Marks events posted by GhostType, so its own event tap lets them through untouched.
const INJECTION_TAG: i64 = 0x4754_5950;

const MAX_UTF16_UNITS_APPS_READ_PER_EVENT: usize = 20;
const UNUSED_KEY_CODE: CGKeyCode = 0;

/// A keyboard event for accepted text could not be created.
#[derive(Debug, thiserror::Error)]
#[error("failed to create a keyboard event")]
pub(crate) struct InjectionError;

/// Text to type at the caret, after the deletion of characters before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replacement {
    /// The characters to delete before the caret.
    pub(crate) deleted: usize,
    /// The text to type after the deletion.
    pub(crate) text: String,
}

/// Posts a delete key press for each deleted character, then the text as Unicode key presses, to
/// whatever has keyboard focus.
pub(crate) fn replace_text(replacement: &Replacement) -> Result<(), InjectionError> {
    (0..replacement.deleted).try_for_each(|_| post_key(DELETE_KEY_CODE, &[]))?;
    utf16_chunks(&replacement.text)
        .iter()
        .try_for_each(|chunk| post_key(UNUSED_KEY_CODE, chunk))
}

/// Whether GhostType posted `event`.
pub(crate) fn is_injected(event: &CGEvent) -> bool {
    CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData) == INJECTION_TAG
}

/// Posts a press and a release of `key_code`, which types `units` when they are not empty.
fn post_key(key_code: CGKeyCode, units: &[u16]) -> Result<(), InjectionError> {
    let unit_count = units.len().try_into().expect("chunk length fits in UniCharCount");
    for is_key_down in [true, false] {
        let event = CGEvent::new_keyboard_event(None, key_code, is_key_down).ok_or(InjectionError)?;
        CGEvent::set_flags(Some(&event), CGEventFlags::empty());
        CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, INJECTION_TAG);
        if !units.is_empty() {
            // SAFETY: `units` is valid for the length passed.
            unsafe { CGEvent::keyboard_set_unicode_string(Some(&event), unit_count, units.as_ptr()) };
        }
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
    Ok(())
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
