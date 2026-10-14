//! Translates key-down events from the event tap into core [`Key`]s.

use ghosttype_core::key::Key;
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGKeyCode};

const TAB_KEY_CODE: CGKeyCode = 48;
const DELETE_KEY_CODE: CGKeyCode = 51;
const ESCAPE_KEY_CODE: CGKeyCode = 53;
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

/// What the event tap does with a keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Disposition {
    /// The focused app receives the event.
    Pass,
    /// The event is dropped before any app sees it.
    Swallow,
}

/// Swallows the release of a key whose press was swallowed, so the focused app sees neither.
#[derive(Debug, Default)]
pub(crate) struct ReleaseFilter {
    swallowed: Option<CGKeyCode>,
}

impl ReleaseFilter {
    /// Records how the press of `key` was handled.
    pub(crate) fn on_press(&mut self, key: CGKeyCode, disposition: Disposition) {
        match disposition {
            Disposition::Swallow => self.swallowed = Some(key),
            Disposition::Pass if self.swallowed == Some(key) => self.swallowed = None,
            Disposition::Pass => {}
        }
    }

    /// Returns how the release of `key` must be handled.
    pub(crate) fn on_release(&mut self, key: CGKeyCode) -> Disposition {
        if self.swallowed != Some(key) {
            return Disposition::Pass;
        }
        self.swallowed = None;
        Disposition::Swallow
    }
}

/// Maps a key press to a core key.
///
/// `typed` is the character the press produces in the active keyboard layout, if exactly one.
/// Modifier keys never reach this: macOS reports them as flag changes, not key presses.
pub(crate) fn translate(key: CGKeyCode, chord: Chord, typed: Option<char>) -> Key {
    match key {
        TAB_KEY_CODE if chord == Chord::Plain => Key::Tab,
        ESCAPE_KEY_CODE => Key::Escape,
        DELETE_KEY_CODE if matches!(chord, Chord::Plain | Chord::Shift) => Key::Backspace,
        _ if chord == Chord::Shortcut => Key::Other,
        _ => typed
            .filter(|character| !FUNCTION_KEY_CHARS.contains(character))
            .map_or(Key::Other, Key::Char),
    }
}

/// Returns the only character in `units`, or `None` when they decode to zero or several.
pub(crate) fn single_char(units: &[u16]) -> Option<char> {
    let mut characters = char::decode_utf16(units.iter().copied());
    match (characters.next(), characters.next()) {
        (Some(Ok(character)), None) => Some(character),
        _ => None,
    }
}

/// Reads the hardware key code of a keyboard `event`.
pub(crate) fn key_code(event: &CGEvent) -> Option<CGKeyCode> {
    let code = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode);
    CGKeyCode::try_from(code).ok()
}

/// Returns the character a keyboard `event` types in the active layout, if exactly one.
pub(crate) fn typed_char(event: &CGEvent) -> Option<char> {
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
    let written = usize::try_from(written).ok()?;
    single_char(units.get(..written)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_KEY_CODE: CGKeyCode = 0;
    const RETURN_KEY_CODE: CGKeyCode = 36;
    const LEFT_ARROW_KEY_CODE: CGKeyCode = 123;

    #[test]
    fn plain_tab_is_tab() {
        assert_eq!(translate(TAB_KEY_CODE, Chord::Plain, Some('\t')), Key::Tab);
    }

    #[test]
    fn shift_tab_is_not_tab() {
        assert_eq!(translate(TAB_KEY_CODE, Chord::Shift, Some('\t')), Key::Char('\t'));
    }

    #[test]
    fn command_tab_is_other() {
        assert_eq!(translate(TAB_KEY_CODE, Chord::Shortcut, Some('\t')), Key::Other);
    }

    #[test]
    fn escape_is_escape() {
        assert_eq!(
            translate(ESCAPE_KEY_CODE, Chord::Plain, Some('\u{1b}')),
            Key::Escape
        );
    }

    #[test]
    fn delete_is_backspace() {
        assert_eq!(
            translate(DELETE_KEY_CODE, Chord::Plain, Some('\u{7f}')),
            Key::Backspace
        );
    }

    #[test]
    fn option_delete_is_not_backspace() {
        assert_eq!(
            translate(DELETE_KEY_CODE, Chord::Option, Some('\u{7f}')),
            Key::Char('\u{7f}')
        );
    }

    #[test]
    fn command_delete_is_other() {
        assert_eq!(
            translate(DELETE_KEY_CODE, Chord::Shortcut, Some('\u{7f}')),
            Key::Other
        );
    }

    #[test]
    fn typed_letter_is_char() {
        assert_eq!(translate(A_KEY_CODE, Chord::Shift, Some('A')), Key::Char('A'));
    }

    #[test]
    fn option_character_is_char() {
        assert_eq!(translate(A_KEY_CODE, Chord::Option, Some('å')), Key::Char('å'));
    }

    #[test]
    fn command_letter_is_other() {
        assert_eq!(translate(A_KEY_CODE, Chord::Shortcut, Some('a')), Key::Other);
    }

    #[test]
    fn return_passes_its_control_character() {
        assert_eq!(
            translate(RETURN_KEY_CODE, Chord::Plain, Some('\r')),
            Key::Char('\r')
        );
    }

    #[test]
    fn arrow_function_key_character_is_other() {
        assert_eq!(
            translate(LEFT_ARROW_KEY_CODE, Chord::Plain, Some('\u{F702}')),
            Key::Other
        );
    }

    #[test]
    fn key_without_character_is_other() {
        assert_eq!(translate(A_KEY_CODE, Chord::Plain, None), Key::Other);
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
    fn swallowed_press_swallows_its_release() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB_KEY_CODE, Disposition::Swallow);

        assert_eq!(filter.on_release(TAB_KEY_CODE), Disposition::Swallow);
    }

    #[test]
    fn passed_press_passes_its_release() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB_KEY_CODE, Disposition::Pass);

        assert_eq!(filter.on_release(TAB_KEY_CODE), Disposition::Pass);
    }

    #[test]
    fn release_of_other_key_passes() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB_KEY_CODE, Disposition::Swallow);

        assert_eq!(filter.on_release(A_KEY_CODE), Disposition::Pass);
    }

    #[test]
    fn release_is_swallowed_once() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB_KEY_CODE, Disposition::Swallow);
        filter.on_release(TAB_KEY_CODE);

        assert_eq!(filter.on_release(TAB_KEY_CODE), Disposition::Pass);
    }

    #[test]
    fn release_passes_after_repeated_press_passed() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB_KEY_CODE, Disposition::Swallow);
        filter.on_press(TAB_KEY_CODE, Disposition::Pass);

        assert_eq!(filter.on_release(TAB_KEY_CODE), Disposition::Pass);
    }

    #[test]
    fn surrogate_pair_is_single_char() {
        assert_eq!(single_char(&[0xD83D, 0xDE00]), Some('😀'));
    }

    #[test]
    fn two_characters_are_not_single_char() {
        assert_eq!(single_char(&[0x0060, 0x0061]), None);
    }

    #[test]
    fn no_units_are_not_single_char() {
        assert_eq!(single_char(&[]), None);
    }
}
