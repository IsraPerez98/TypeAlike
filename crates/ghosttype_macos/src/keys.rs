//! Translates key-down events from the event tap into core [`Key`]s.

use ghosttype_core::key::Key;
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGKeyCode};

const RETURN_KEY_CODE: CGKeyCode = 36;
const TAB_KEY_CODE: CGKeyCode = 48;
const DELETE_KEY_CODE: CGKeyCode = 51;
const ESCAPE_KEY_CODE: CGKeyCode = 53;
const KEYPAD_ENTER_KEY_CODE: CGKeyCode = 76;
const FUNCTION_KEY_CHARS: std::ops::RangeInclusive<char> = '\u{F700}'..='\u{F8FF}';
const MAX_TYPED_UNITS: usize = 8;

/// The modifier combination held during a key press, by what it means for typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chord {
    /// No modifier.
    Plain,
    /// Shift only: still typing.
    Shift,
    /// Option without Command or Control, which layouts use for extra characters.
    Option,
    /// Command or Control: a shortcut or an editing command.
    Shortcut,
}

impl Chord {
    /// Classifies the modifier `flags` of a key event; Caps Lock and Fn don't count.
    pub(crate) fn from_flags(flags: CGEventFlags) -> Self {
        if flags.intersects(CGEventFlags::MaskCommand | CGEventFlags::MaskControl) {
            Self::Shortcut
        } else if flags.contains(CGEventFlags::MaskAlternate) {
            Self::Option
        } else if flags.contains(CGEventFlags::MaskShift) {
            Self::Shift
        } else {
            Self::Plain
        }
    }
}

/// What a key press types in the active keyboard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Typed {
    /// Exactly one character.
    Char(char),
    /// No character: a dead key, whose accent comes with the next key.
    Dead,
    /// Several characters.
    Several,
}

/// Maps a key press to a core key.
///
/// `typed` is what the press types in the active keyboard layout.
/// Modifier keys never reach this: macOS reports them as flag changes, not key presses.
pub(crate) fn translate(key: CGKeyCode, chord: Chord, typed: Typed) -> Key {
    match key {
        TAB_KEY_CODE if chord == Chord::Plain => Key::Tab,
        ESCAPE_KEY_CODE => Key::Escape,
        RETURN_KEY_CODE | KEYPAD_ENTER_KEY_CODE if matches!(chord, Chord::Plain | Chord::Shift) => Key::Enter,
        DELETE_KEY_CODE if matches!(chord, Chord::Plain | Chord::Shift) => Key::Backspace,
        _ if chord == Chord::Shortcut => Key::Other,
        _ => match typed {
            Typed::Char(character) if !FUNCTION_KEY_CHARS.contains(&character) => Key::Char(character),
            Typed::Dead => Key::Dead,
            Typed::Char(_) | Typed::Several => Key::Other,
        },
    }
}

/// Returns what a key press types from `units`, the UTF-16 text of the press. macOS gives no
/// text for a dead key.
pub(crate) fn typed_from(units: &[u16]) -> Typed {
    let mut characters = char::decode_utf16(units.iter().copied());
    match (characters.next(), characters.next()) {
        (None, _) => Typed::Dead,
        (Some(Ok(character)), None) => Typed::Char(character),
        _ => Typed::Several,
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
    const E_KEY_CODE: CGKeyCode = 14;
    const ACCENT_KEY_CODE: CGKeyCode = 33;
    const LEFT_ARROW_KEY_CODE: CGKeyCode = 123;

    #[test]
    fn plain_tab_is_tab() {
        assert_eq!(translate(TAB_KEY_CODE, Chord::Plain, Typed::Char('\t')), Key::Tab);
    }

    #[test]
    fn shift_tab_is_not_tab() {
        assert_eq!(
            translate(TAB_KEY_CODE, Chord::Shift, Typed::Char('\t')),
            Key::Char('\t')
        );
    }

    #[test]
    fn command_tab_is_other() {
        assert_eq!(
            translate(TAB_KEY_CODE, Chord::Shortcut, Typed::Char('\t')),
            Key::Other
        );
    }

    #[test]
    fn escape_is_escape() {
        assert_eq!(
            translate(ESCAPE_KEY_CODE, Chord::Plain, Typed::Char('\u{1b}')),
            Key::Escape
        );
    }

    #[test]
    fn delete_is_backspace() {
        assert_eq!(
            translate(DELETE_KEY_CODE, Chord::Plain, Typed::Char('\u{7f}')),
            Key::Backspace
        );
    }

    #[test]
    fn option_delete_is_not_backspace() {
        assert_eq!(
            translate(DELETE_KEY_CODE, Chord::Option, Typed::Char('\u{7f}')),
            Key::Char('\u{7f}')
        );
    }

    #[test]
    fn command_delete_is_other() {
        assert_eq!(
            translate(DELETE_KEY_CODE, Chord::Shortcut, Typed::Char('\u{7f}')),
            Key::Other
        );
    }

    #[test]
    fn typed_letter_is_char() {
        assert_eq!(
            translate(A_KEY_CODE, Chord::Shift, Typed::Char('A')),
            Key::Char('A')
        );
    }

    #[test]
    fn option_character_is_char() {
        assert_eq!(
            translate(A_KEY_CODE, Chord::Option, Typed::Char('å')),
            Key::Char('å')
        );
    }

    #[test]
    fn command_letter_is_other() {
        assert_eq!(
            translate(A_KEY_CODE, Chord::Shortcut, Typed::Char('a')),
            Key::Other
        );
    }

    #[test]
    fn return_is_enter() {
        assert_eq!(
            translate(RETURN_KEY_CODE, Chord::Plain, Typed::Char('\r')),
            Key::Enter
        );
    }

    #[test]
    fn shift_return_is_enter() {
        assert_eq!(
            translate(RETURN_KEY_CODE, Chord::Shift, Typed::Char('\r')),
            Key::Enter
        );
    }

    #[test]
    fn keypad_enter_is_enter() {
        assert_eq!(
            translate(KEYPAD_ENTER_KEY_CODE, Chord::Plain, Typed::Char('\u{3}')),
            Key::Enter
        );
    }

    #[test]
    fn option_return_passes_its_control_character() {
        assert_eq!(
            translate(RETURN_KEY_CODE, Chord::Option, Typed::Char('\r')),
            Key::Char('\r')
        );
    }

    #[test]
    fn command_return_is_other() {
        assert_eq!(
            translate(RETURN_KEY_CODE, Chord::Shortcut, Typed::Char('\r')),
            Key::Other
        );
    }

    #[test]
    fn arrow_function_key_character_is_other() {
        assert_eq!(
            translate(LEFT_ARROW_KEY_CODE, Chord::Plain, Typed::Char('\u{F702}')),
            Key::Other
        );
    }

    #[test]
    fn dead_key_is_dead() {
        assert_eq!(translate(ACCENT_KEY_CODE, Chord::Plain, Typed::Dead), Key::Dead);
    }

    #[test]
    fn option_dead_key_is_dead() {
        assert_eq!(translate(E_KEY_CODE, Chord::Option, Typed::Dead), Key::Dead);
    }

    #[test]
    fn command_dead_key_is_other() {
        assert_eq!(
            translate(ACCENT_KEY_CODE, Chord::Shortcut, Typed::Dead),
            Key::Other
        );
    }

    #[test]
    fn key_with_several_characters_is_other() {
        assert_eq!(translate(A_KEY_CODE, Chord::Plain, Typed::Several), Key::Other);
    }

    #[test]
    fn no_flags_are_plain() {
        assert_eq!(Chord::from_flags(CGEventFlags::empty()), Chord::Plain);
    }

    #[test]
    fn caps_lock_and_fn_are_plain() {
        assert_eq!(
            Chord::from_flags(CGEventFlags::MaskAlphaShift | CGEventFlags::MaskSecondaryFn),
            Chord::Plain
        );
    }

    #[test]
    fn shift_is_shift() {
        assert_eq!(Chord::from_flags(CGEventFlags::MaskShift), Chord::Shift);
    }

    #[test]
    fn shift_and_option_are_option() {
        assert_eq!(
            Chord::from_flags(CGEventFlags::MaskShift | CGEventFlags::MaskAlternate),
            Chord::Option
        );
    }

    #[test]
    fn control_is_shortcut() {
        assert_eq!(Chord::from_flags(CGEventFlags::MaskControl), Chord::Shortcut);
    }

    #[test]
    fn command_and_option_are_shortcut() {
        assert_eq!(
            Chord::from_flags(CGEventFlags::MaskCommand | CGEventFlags::MaskAlternate),
            Chord::Shortcut
        );
    }

    #[test]
    fn surrogate_pair_is_one_character() {
        assert_eq!(typed_from(&[0xD83D, 0xDE00]), Typed::Char('😀'));
    }

    #[test]
    fn two_characters_are_several() {
        assert_eq!(typed_from(&[0x0060, 0x0061]), Typed::Several);
    }

    #[test]
    fn no_units_are_a_dead_key() {
        assert_eq!(typed_from(&[]), Typed::Dead);
    }
}
