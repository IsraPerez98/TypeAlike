//! Translates low-level keyboard events into core [`Key`]s.

use ghosttype_core::key::Key;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx, VIRTUAL_KEY, VK_BACK, VK_CAPITAL,
    VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NUMLOCK, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SCROLL, VK_SHIFT, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

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
    pub(crate) fn from_held(is_held: impl Fn(VIRTUAL_KEY) -> bool) -> Self {
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

/// What a hook does with a keyboard event.
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
    swallowed: Option<VIRTUAL_KEY>,
}

impl ReleaseFilter {
    /// Records how the press of `key` was handled.
    pub(crate) fn on_press(&mut self, key: VIRTUAL_KEY, disposition: Disposition) {
        match disposition {
            Disposition::Swallow => self.swallowed = Some(key),
            Disposition::Pass if self.swallowed == Some(key) => self.swallowed = None,
            Disposition::Pass => {}
        }
    }

    /// Returns how the release of `key` must be handled.
    pub(crate) fn on_release(&mut self, key: VIRTUAL_KEY) -> Disposition {
        if self.swallowed != Some(key) {
            return Disposition::Pass;
        }
        self.swallowed = None;
        Disposition::Swallow
    }
}

/// Maps a key press to a core key; `None` for presses the engine must not see (modifiers alone).
///
/// `typed` is the character the press produces in the active keyboard layout, if exactly one.
pub(crate) fn translate(key: VIRTUAL_KEY, chord: Chord, typed: Option<char>) -> Option<Key> {
    if is_modifier(key) {
        return None;
    }
    Some(match key {
        VK_TAB if chord == Chord::Plain => Key::Tab,
        VK_ESCAPE => Key::Escape,
        VK_BACK if matches!(chord, Chord::Plain | Chord::Shift) => Key::Backspace,
        _ if matches!(chord, Chord::Alt | Chord::Windows) => Key::Other,
        _ => typed.map_or(Key::Other, Key::Char),
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

/// Returns the only character in `units`, or `None` when they decode to zero or several.
pub(crate) fn single_char(units: &[u16]) -> Option<char> {
    let mut characters = char::decode_utf16(units.iter().copied());
    match (characters.next(), characters.next()) {
        (Some(Ok(character)), None) => Some(character),
        _ => None,
    }
}

/// Reads the modifier combination held right now.
pub(crate) fn current_chord() -> Chord {
    Chord::from_held(is_held)
}

/// Returns the character `key` types in the foreground window's keyboard layout, if exactly one.
pub(crate) fn typed_char(key: VIRTUAL_KEY, scan_code: u32) -> Option<char> {
    let keyboard_state = current_keyboard_state();
    let mut units = [0_u16; MAX_TRANSLATED_UNITS];
    // SAFETY: plain calls; a null foreground window yields thread 0, which means the current layout.
    let layout = unsafe { GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None)) };
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
    let written = usize::try_from(written).ok()?;
    single_char(units.get(..written)?)
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
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_A, VK_LEFT, VK_RETURN};

    use super::*;

    #[test]
    fn modifier_alone_is_not_a_key() {
        assert_eq!(translate(VK_LSHIFT, Chord::Shift, None), None);
    }

    #[test]
    fn plain_tab_is_tab() {
        assert_eq!(translate(VK_TAB, Chord::Plain, Some('\t')), Some(Key::Tab));
    }

    #[test]
    fn shift_tab_is_other() {
        assert_eq!(translate(VK_TAB, Chord::Shift, None), Some(Key::Other));
    }

    #[test]
    fn alt_tab_is_other() {
        assert_eq!(translate(VK_TAB, Chord::Alt, None), Some(Key::Other));
    }

    #[test]
    fn escape_is_escape() {
        assert_eq!(
            translate(VK_ESCAPE, Chord::Plain, Some('\u{1b}')),
            Some(Key::Escape)
        );
    }

    #[test]
    fn backspace_is_backspace() {
        assert_eq!(
            translate(VK_BACK, Chord::Plain, Some('\u{8}')),
            Some(Key::Backspace)
        );
    }

    #[test]
    fn control_backspace_is_other() {
        assert_eq!(translate(VK_BACK, Chord::Control, None), Some(Key::Other));
    }

    #[test]
    fn typed_letter_is_char() {
        assert_eq!(translate(VK_A, Chord::Shift, Some('A')), Some(Key::Char('A')));
    }

    #[test]
    fn alt_graph_character_is_char() {
        assert_eq!(translate(VK_A, Chord::AltGraph, Some('@')), Some(Key::Char('@')));
    }

    #[test]
    fn alt_letter_is_other() {
        assert_eq!(translate(VK_A, Chord::Alt, Some('a')), Some(Key::Other));
    }

    #[test]
    fn windows_letter_is_other() {
        assert_eq!(translate(VK_A, Chord::Windows, Some('a')), Some(Key::Other));
    }

    #[test]
    fn enter_passes_its_control_character() {
        assert_eq!(
            translate(VK_RETURN, Chord::Plain, Some('\r')),
            Some(Key::Char('\r'))
        );
    }

    #[test]
    fn key_without_character_is_other() {
        assert_eq!(translate(VK_LEFT, Chord::Plain, None), Some(Key::Other));
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
    fn swallowed_press_swallows_its_release() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(VK_TAB, Disposition::Swallow);

        assert_eq!(filter.on_release(VK_TAB), Disposition::Swallow);
    }

    #[test]
    fn passed_press_passes_its_release() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(VK_TAB, Disposition::Pass);

        assert_eq!(filter.on_release(VK_TAB), Disposition::Pass);
    }

    #[test]
    fn release_of_other_key_passes() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(VK_TAB, Disposition::Swallow);

        assert_eq!(filter.on_release(VK_A), Disposition::Pass);
    }

    #[test]
    fn release_is_swallowed_once() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(VK_TAB, Disposition::Swallow);
        filter.on_release(VK_TAB);

        assert_eq!(filter.on_release(VK_TAB), Disposition::Pass);
    }

    #[test]
    fn release_passes_after_repeated_press_passed() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(VK_TAB, Disposition::Swallow);
        filter.on_press(VK_TAB, Disposition::Pass);

        assert_eq!(filter.on_release(VK_TAB), Disposition::Pass);
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
