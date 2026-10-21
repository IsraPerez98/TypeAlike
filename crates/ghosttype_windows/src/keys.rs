//! Translates low-level keyboard events into core [`Key`]s.

use ghosttype_core::key::Key;
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

/// The modifier combination held during a key press, by what it means for typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chord {
    /// No modifier.
    Plain,
    /// Shift only: still typing.
    Shift,
    /// Control, possibly with Shift: an editing shortcut.
    Control,
    /// Control and Alt together, which layouts use as `AltGr` for extra characters.
    AltGraph,
    /// Alt without Control: menu access keys.
    Alt,
    /// The Windows key: shell shortcuts.
    Windows,
}

impl Chord {
    /// Classifies the modifiers that `is_held` reports as down.
    fn from_held(is_held: impl Fn(VIRTUAL_KEY) -> bool) -> Self {
        let has_control = is_held(VK_CONTROL);
        let has_alt = is_held(VK_MENU);
        if is_held(VK_LWIN) || is_held(VK_RWIN) {
            Self::Windows
        } else if has_alt && has_control {
            Self::AltGraph
        } else if has_alt {
            Self::Alt
        } else if has_control {
            Self::Control
        } else if is_held(VK_SHIFT) {
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
    /// No character yet: a dead key, whose accent comes with the next key.
    Dead,
    /// No character, or several.
    Other,
}

/// Maps a key press to a core key; `None` for presses the engine must not see (modifiers alone).
///
/// `typed` is what the press types in the active keyboard layout.
pub(crate) fn translate(key: VIRTUAL_KEY, chord: Chord, typed: Typed) -> Option<Key> {
    if is_modifier(key) {
        return None;
    }
    Some(match key {
        VK_TAB if chord == Chord::Plain => Key::Tab,
        VK_ESCAPE => Key::Escape,
        VK_RETURN if matches!(chord, Chord::Plain | Chord::Shift) => Key::Enter,
        VK_BACK if matches!(chord, Chord::Plain | Chord::Shift) => Key::Backspace,
        _ if matches!(chord, Chord::Alt | Chord::Windows) => Key::Other,
        _ => match typed {
            Typed::Char(character) => Key::Char(character),
            Typed::Dead => Key::Dead,
            Typed::Other => Key::Other,
        },
    })
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
        _ => Typed::Other,
    }
}

/// Reads the modifier combination held right now.
pub(crate) fn current_chord() -> Chord {
    Chord::from_held(is_held)
}

/// Returns what `key` types in the foreground window's keyboard layout.
pub(crate) fn typed(key: VIRTUAL_KEY, scan_code: u32) -> Typed {
    let keyboard_state = current_keyboard_state();
    let mut units = [0_u16; MAX_TRANSLATED_UNITS];
    // SAFETY: plain call; thread 0, for no foreground window, means the current layout.
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
    // SAFETY: plain call.
    let caps_lock = unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) };
    if caps_lock & i16::from(KEY_TOGGLED_BIT) != 0 {
        state[usize::from(VK_CAPITAL.0)] = KEY_TOGGLED_BIT;
    }
    state
}

fn is_held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: plain call.
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
    state < 0
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_A, VK_LEFT, VK_OEM_4, VK_OEM_7};

    use super::*;

    #[test]
    fn modifier_alone_is_not_a_key() {
        assert_eq!(translate(VK_LSHIFT, Chord::Shift, Typed::Other), None);
    }

    #[test]
    fn plain_tab_is_tab() {
        assert_eq!(translate(VK_TAB, Chord::Plain, Typed::Char('\t')), Some(Key::Tab));
    }

    #[test]
    fn shift_tab_is_other() {
        assert_eq!(translate(VK_TAB, Chord::Shift, Typed::Other), Some(Key::Other));
    }

    #[test]
    fn alt_tab_is_other() {
        assert_eq!(translate(VK_TAB, Chord::Alt, Typed::Other), Some(Key::Other));
    }

    #[test]
    fn escape_is_escape() {
        assert_eq!(
            translate(VK_ESCAPE, Chord::Plain, Typed::Char('\u{1b}')),
            Some(Key::Escape)
        );
    }

    #[test]
    fn backspace_is_backspace() {
        assert_eq!(
            translate(VK_BACK, Chord::Plain, Typed::Char('\u{8}')),
            Some(Key::Backspace)
        );
    }

    #[test]
    fn control_backspace_is_other() {
        assert_eq!(translate(VK_BACK, Chord::Control, Typed::Other), Some(Key::Other));
    }

    #[test]
    fn typed_letter_is_char() {
        assert_eq!(
            translate(VK_A, Chord::Shift, Typed::Char('A')),
            Some(Key::Char('A'))
        );
    }

    #[test]
    fn alt_graph_character_is_char() {
        assert_eq!(
            translate(VK_A, Chord::AltGraph, Typed::Char('@')),
            Some(Key::Char('@'))
        );
    }

    #[test]
    fn alt_letter_is_other() {
        assert_eq!(translate(VK_A, Chord::Alt, Typed::Char('a')), Some(Key::Other));
    }

    #[test]
    fn windows_letter_is_other() {
        assert_eq!(
            translate(VK_A, Chord::Windows, Typed::Char('a')),
            Some(Key::Other)
        );
    }

    #[test]
    fn enter_is_enter() {
        assert_eq!(
            translate(VK_RETURN, Chord::Plain, Typed::Char('\r')),
            Some(Key::Enter)
        );
    }

    #[test]
    fn shift_enter_is_enter() {
        assert_eq!(
            translate(VK_RETURN, Chord::Shift, Typed::Char('\r')),
            Some(Key::Enter)
        );
    }

    #[test]
    fn control_enter_passes_its_control_character() {
        assert_eq!(
            translate(VK_RETURN, Chord::Control, Typed::Char('\n')),
            Some(Key::Char('\n'))
        );
    }

    #[test]
    fn alt_enter_is_other() {
        assert_eq!(
            translate(VK_RETURN, Chord::Alt, Typed::Char('\r')),
            Some(Key::Other)
        );
    }

    #[test]
    fn dead_key_is_dead() {
        assert_eq!(translate(VK_OEM_7, Chord::Plain, Typed::Dead), Some(Key::Dead));
    }

    #[test]
    fn alt_graph_dead_key_is_dead() {
        assert_eq!(translate(VK_OEM_4, Chord::AltGraph, Typed::Dead), Some(Key::Dead));
    }

    #[test]
    fn key_without_character_is_other() {
        assert_eq!(translate(VK_LEFT, Chord::Plain, Typed::Other), Some(Key::Other));
    }

    #[test]
    fn nothing_held_is_plain() {
        assert_eq!(Chord::from_held(|_| false), Chord::Plain);
    }

    #[test]
    fn control_and_alt_are_alt_graph() {
        assert_eq!(
            Chord::from_held(|key| key == VK_CONTROL || key == VK_MENU),
            Chord::AltGraph
        );
    }

    #[test]
    fn control_and_shift_are_control() {
        assert_eq!(
            Chord::from_held(|key| key == VK_CONTROL || key == VK_SHIFT),
            Chord::Control
        );
    }

    #[test]
    fn shift_and_alt_are_alt() {
        assert_eq!(
            Chord::from_held(|key| key == VK_SHIFT || key == VK_MENU),
            Chord::Alt
        );
    }

    #[test]
    fn windows_key_wins_over_others() {
        assert_eq!(
            Chord::from_held(|key| key == VK_RWIN || key == VK_CONTROL),
            Chord::Windows
        );
    }

    #[test]
    fn surrogate_pair_is_one_character() {
        assert_eq!(typed_from(2, &[0xD83D, 0xDE00]), Typed::Char('😀'));
    }

    #[test]
    fn two_characters_are_other() {
        assert_eq!(typed_from(2, &[0x0060, 0x0061]), Typed::Other);
    }

    #[test]
    fn no_units_are_other() {
        assert_eq!(typed_from(0, &[0x0061]), Typed::Other);
    }

    #[test]
    fn negative_count_is_a_dead_key() {
        assert_eq!(typed_from(-1, &[0x00B4]), Typed::Dead);
    }
}
