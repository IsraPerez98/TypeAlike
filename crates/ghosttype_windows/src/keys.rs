//! Translates low-level keyboard events into core [`Key`]s.

use ghosttype_core::key::{Chord, Key, KeyKind, Typed};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx, VIRTUAL_KEY, VK_BACK, VK_CAPITAL,
    VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NUMLOCK, VK_RCONTROL,
    VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SCROLL, VK_SHIFT, VK_TAB,
};

use crate::focus;

const KEY_DOWN_BIT: u8 = 0x80;
const KEY_TOGGLED_BIT: u8 = 0x01;
const PRESERVE_DEAD_KEY_STATE: u32 = 0x4;
const VIRTUAL_KEY_COUNT: usize = 256;
const MAX_TRANSLATED_UNITS: usize = 8;

/// Classifies the modifiers that `is_held` reports as down.
///
/// The Windows key and Alt without Control are for shortcuts. Control types a control character,
/// such as a line feed for Control and Enter. Control with Alt is `AltGr`. Both modify the character.
fn chord_from_held(is_held: impl Fn(VIRTUAL_KEY) -> bool) -> Chord {
    let has_control = is_held(VK_CONTROL);
    let has_alt = is_held(VK_MENU);
    if is_held(VK_LWIN) || is_held(VK_RWIN) {
        Chord::Shortcut
    } else if has_alt && has_control {
        Chord::CharacterModifier
    } else if has_alt {
        Chord::Shortcut
    } else if has_control {
        Chord::CharacterModifier
    } else if is_held(VK_SHIFT) {
        Chord::Shift
    } else {
        Chord::Plain
    }
}

/// Maps a key press to a core key, or gives `None` for a modifier alone, which the engine must not see.
///
/// `typed` is what the press types in the active keyboard layout.
pub(crate) fn translate(key: VIRTUAL_KEY, chord: Chord, typed: Typed) -> Option<Key> {
    if is_modifier(key) {
        return None;
    }
    Some(Key::from_press(kind_of(key), chord, typed))
}

fn kind_of(key: VIRTUAL_KEY) -> KeyKind {
    match key {
        VK_TAB => KeyKind::Tab,
        VK_ESCAPE => KeyKind::Escape,
        VK_RETURN => KeyKind::Enter,
        VK_BACK => KeyKind::Backspace,
        _ => KeyKind::Other,
    }
}

fn is_modifier(key: VIRTUAL_KEY) -> bool {
    matches!(
        key,
        VK_SHIFT
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_CONTROL
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_MENU
            | VK_LMENU
            | VK_RMENU
            | VK_LWIN
            | VK_RWIN
            | VK_CAPITAL
            | VK_NUMLOCK
            | VK_SCROLL
    )
}

/// Returns what a key press types from the answer of `ToUnicodeEx`: `written`, its count, which
/// is negative for a dead key, and `units`, its UTF-16 text.
fn typed_from(written: i32, units: &[u16]) -> Typed {
    let Ok(written) = usize::try_from(written) else {
        return Typed::Dead;
    };
    let mut characters = char::decode_utf16(units.iter().take(written).copied());
    match (characters.next(), characters.next()) {
        (Some(Ok(character)), None) => Typed::Char(character),
        _ => Typed::NoSingleChar,
    }
}

/// Reads the modifier combination held right now.
pub(crate) fn current_chord() -> Chord {
    chord_from_held(is_held)
}

/// Returns what `key` types in the foreground window's keyboard layout.
pub(crate) fn typed(key: VIRTUAL_KEY, scan_code: u32) -> Typed {
    let keyboard_state = current_keyboard_state();
    let mut units = [0_u16; MAX_TRANSLATED_UNITS];
    // SAFETY: the call takes no pointer, and thread 0, for no foreground window, means the current layout.
    let layout = unsafe { GetKeyboardLayout(focus::foreground_thread()) };
    // SAFETY: buffers are valid for the call.
    let written = unsafe {
        ToUnicodeEx(
            u32::from(key.0),
            scan_code,
            &keyboard_state,
            &mut units,
            PRESERVE_DEAD_KEY_STATE,
            Some(layout),
        )
    };
    typed_from(written, &units)
}

fn current_keyboard_state() -> [u8; VIRTUAL_KEY_COUNT] {
    let mut state = [0_u8; VIRTUAL_KEY_COUNT];
    for key in [VK_SHIFT, VK_CONTROL, VK_MENU] {
        if is_held(key) {
            state[usize::from(key.0)] = KEY_DOWN_BIT;
        }
    }
    // SAFETY: the call takes no pointer and has no precondition.
    let caps_lock = unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) };
    if caps_lock & i16::from(KEY_TOGGLED_BIT) != 0 {
        state[usize::from(VK_CAPITAL.0)] = KEY_TOGGLED_BIT;
    }
    state
}

fn is_held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: the call takes no pointer and has no precondition.
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
    state < 0
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_A, VK_LEFT};

    use super::*;

    #[test]
    fn modifier_alone_is_not_a_key() {
        assert_eq!(translate(VK_LSHIFT, Chord::Shift, Typed::NoSingleChar), None);
    }

    #[test]
    fn key_press_gets_the_key_of_its_kind() {
        assert_eq!(
            translate(VK_RETURN, Chord::Shift, Typed::Char('\r')),
            Some(Key::Enter)
        );
    }

    #[test]
    fn virtual_keys_map_to_their_kind() {
        let cases = [
            (VK_TAB, KeyKind::Tab),
            (VK_ESCAPE, KeyKind::Escape),
            (VK_RETURN, KeyKind::Enter),
            (VK_BACK, KeyKind::Backspace),
            (VK_A, KeyKind::Other),
            (VK_LEFT, KeyKind::Other),
        ];

        let kinds = cases.map(|(key, _)| kind_of(key));

        assert_eq!(kinds, cases.map(|(_, kind)| kind));
    }

    #[test]
    fn nothing_held_is_plain() {
        assert_eq!(chord_from_held(|_| false), Chord::Plain);
    }

    #[test]
    fn shift_alone_is_shift() {
        assert_eq!(chord_from_held(|key| key == VK_SHIFT), Chord::Shift);
    }

    #[test]
    fn control_and_alt_modify_the_character() {
        assert_eq!(
            chord_from_held(|key| key == VK_CONTROL || key == VK_MENU),
            Chord::CharacterModifier
        );
    }

    #[test]
    fn control_and_shift_modify_the_character() {
        assert_eq!(
            chord_from_held(|key| key == VK_CONTROL || key == VK_SHIFT),
            Chord::CharacterModifier
        );
    }

    #[test]
    fn shift_and_alt_are_a_shortcut() {
        assert_eq!(
            chord_from_held(|key| key == VK_SHIFT || key == VK_MENU),
            Chord::Shortcut
        );
    }

    #[test]
    fn windows_key_wins_over_others() {
        assert_eq!(
            chord_from_held(|key| key == VK_RWIN || key == VK_CONTROL),
            Chord::Shortcut
        );
    }

    #[test]
    fn surrogate_pair_is_one_character() {
        assert_eq!(typed_from(2, &[0xD83D, 0xDE00]), Typed::Char('😀'));
    }

    #[test]
    fn two_characters_are_no_single_character() {
        assert_eq!(typed_from(2, &[0x0060, 0x0061]), Typed::NoSingleChar);
    }

    #[test]
    fn no_units_are_no_single_character() {
        assert_eq!(typed_from(0, &[0x0061]), Typed::NoSingleChar);
    }

    #[test]
    fn negative_count_is_a_dead_key() {
        assert_eq!(typed_from(-1, &[0x00B4]), Typed::Dead);
    }
}
