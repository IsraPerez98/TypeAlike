//! Translates key-down events from the event tap into core [`Key`](typealike_core::key::Key)s.

use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGKeyCode};
use typealike_core::key::{Chord, KeyKind, Typed};

const RETURN_KEY_CODE: CGKeyCode = 36;
const TAB_KEY_CODE: CGKeyCode = 48;
/// The key that deletes the character before the caret (`kVK_Delete`).
pub(crate) const DELETE_KEY_CODE: CGKeyCode = 51;
const ESCAPE_KEY_CODE: CGKeyCode = 53;
const KEYPAD_ENTER_KEY_CODE: CGKeyCode = 76;
const FUNCTION_KEY_CHARS: std::ops::RangeInclusive<char> = '\u{F700}'..='\u{F8FF}';
const MAX_TYPED_UNITS: usize = 8;

/// Classifies the modifier `flags` of a key event. Caps Lock and Fn do not count.
///
/// Command and Control are for shortcuts and editing commands. Option types extra characters.
pub(crate) fn chord_from_flags(flags: CGEventFlags) -> Chord {
    if flags.intersects(CGEventFlags::MaskCommand | CGEventFlags::MaskControl) {
        Chord::Shortcut
    } else if flags.contains(CGEventFlags::MaskAlternate) {
        Chord::CharacterModifier
    } else if flags.contains(CGEventFlags::MaskShift) {
        Chord::Shift
    } else {
        Chord::Plain
    }
}

/// Gives the kind of the key with the hardware key code `key`.
/// Modifier keys never reach this: macOS reports them as flag changes, not key presses.
pub(crate) fn kind_of(key: CGKeyCode) -> KeyKind {
    match key {
        TAB_KEY_CODE => KeyKind::Tab,
        ESCAPE_KEY_CODE => KeyKind::Escape,
        RETURN_KEY_CODE | KEYPAD_ENTER_KEY_CODE => KeyKind::Enter,
        DELETE_KEY_CODE => KeyKind::Backspace,
        _ => KeyKind::Other,
    }
}

/// Returns what a key press types from `units`, the UTF-16 text of the press. macOS gives no
/// text for a dead key, and a character of a private range for a function key such as an arrow.
pub(crate) fn typed_from(units: &[u16]) -> Typed {
    let mut characters = char::decode_utf16(units.iter().copied());
    match (characters.next(), characters.next()) {
        (None, _) => Typed::Dead,
        (Some(Ok(character)), None) if !FUNCTION_KEY_CHARS.contains(&character) => Typed::Char(character),
        _ => Typed::NoSingleChar,
    }
}

/// Reads the hardware key code of a keyboard `event`.
pub(crate) fn key_code(event: &CGEvent) -> Option<CGKeyCode> {
    let code = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode);
    CGKeyCode::try_from(code).ok()
}

/// Returns what a keyboard `event` types in the active layout.
pub(crate) fn typed(event: &CGEvent) -> Typed {
    let mut units = [0_u16; MAX_TYPED_UNITS];
    let mut written = 0;
    // SAFETY: both output pointers refer to locals; the buffer holds the maximum length passed.
    unsafe {
        CGEvent::keyboard_get_unicode_string(
            Some(event),
            units
                .len()
                .try_into()
                .expect("buffer length fits in UniCharCount"),
            &raw mut written,
            units.as_mut_ptr(),
        );
    }
    let written = usize::try_from(written).unwrap_or_default();
    typed_from(units.get(..written).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_KEY_CODE: CGKeyCode = 0;
    const LEFT_ARROW_KEY_CODE: CGKeyCode = 123;

    #[test]
    fn key_codes_map_to_their_kind() {
        let cases = [
            (TAB_KEY_CODE, KeyKind::Tab),
            (ESCAPE_KEY_CODE, KeyKind::Escape),
            (RETURN_KEY_CODE, KeyKind::Enter),
            (KEYPAD_ENTER_KEY_CODE, KeyKind::Enter),
            (DELETE_KEY_CODE, KeyKind::Backspace),
            (A_KEY_CODE, KeyKind::Other),
            (LEFT_ARROW_KEY_CODE, KeyKind::Other),
        ];

        let kinds = cases.map(|(key, _)| kind_of(key));

        assert_eq!(kinds, cases.map(|(_, kind)| kind));
    }

    #[test]
    fn no_flags_are_plain() {
        assert_eq!(chord_from_flags(CGEventFlags::empty()), Chord::Plain);
    }

    #[test]
    fn caps_lock_and_fn_are_plain() {
        assert_eq!(
            chord_from_flags(CGEventFlags::MaskAlphaShift | CGEventFlags::MaskSecondaryFn),
            Chord::Plain
        );
    }

    #[test]
    fn shift_is_shift() {
        assert_eq!(chord_from_flags(CGEventFlags::MaskShift), Chord::Shift);
    }

    #[test]
    fn shift_and_option_modify_the_character() {
        assert_eq!(
            chord_from_flags(CGEventFlags::MaskShift | CGEventFlags::MaskAlternate),
            Chord::CharacterModifier
        );
    }

    #[test]
    fn control_is_shortcut() {
        assert_eq!(chord_from_flags(CGEventFlags::MaskControl), Chord::Shortcut);
    }

    #[test]
    fn command_and_option_are_shortcut() {
        assert_eq!(
            chord_from_flags(CGEventFlags::MaskCommand | CGEventFlags::MaskAlternate),
            Chord::Shortcut
        );
    }

    #[test]
    fn surrogate_pair_is_one_character() {
        assert_eq!(typed_from(&[0xD83D, 0xDE00]), Typed::Char('😀'));
    }

    #[test]
    fn two_characters_are_no_single_character() {
        assert_eq!(typed_from(&[0x0060, 0x0061]), Typed::NoSingleChar);
    }

    #[test]
    fn arrow_function_key_character_is_no_single_character() {
        assert_eq!(typed_from(&[0xF702]), Typed::NoSingleChar);
    }

    #[test]
    fn no_units_are_a_dead_key() {
        assert_eq!(typed_from(&[]), Typed::Dead);
    }
}
