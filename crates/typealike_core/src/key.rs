//! Keys as the engine understands them, independent of any keyboard layout or OS key code, and
//! what happens to the key events of the platform.

use std::mem;

/// A key press relevant to suggestions. [`Key::from_press`] gives it from what the platform reads.
///
/// Modifier-only presses are not keys. Platforms drop them before they reach the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A key that types this character into the focused text.
    Char(char),
    /// Deletes the character before the caret.
    Backspace,
    /// Plain Tab without modifiers. It accepts a visible suggestion.
    Tab,
    /// Dismisses a visible suggestion.
    Escape,
    /// Return or Enter, alone or with Shift. Ends a line or submits the field. The typed context
    /// needs a new read.
    Enter,
    /// A dead key: it types nothing, and the next key types an accented letter, such as ´ and then
    /// e for é. The text of the field can then differ from the typed keys.
    Dead,
    /// Any other key: navigation, shortcuts. The typed context is no longer reliable.
    Other,
}

impl Key {
    /// Gives the key of a press of a key of `kind`, with the modifiers of `chord` held, that types
    /// `typed`.
    ///
    /// Tab counts only without a modifier, and Enter and Backspace only alone or with Shift. A
    /// shortcut gives [`Key::Other`]. Else the press gives what it types.
    ///
    /// ```
    /// use typealike_core::key::{Chord, Key, KeyKind, Typed};
    ///
    /// let key = Key::from_press(KeyKind::Enter, Chord::Shortcut, Typed::Char('\r'));
    ///
    /// assert_eq!(key, Key::Other);
    /// ```
    #[must_use]
    pub fn from_press(kind: KeyKind, chord: Chord, typed: Typed) -> Self {
        let is_plain_or_shift = matches!(chord, Chord::Plain | Chord::Shift);
        match kind {
            KeyKind::Tab if chord == Chord::Plain => Self::Tab,
            KeyKind::Escape => Self::Escape,
            KeyKind::Enter if is_plain_or_shift => Self::Enter,
            KeyKind::Backspace if is_plain_or_shift => Self::Backspace,
            _ if chord == Chord::Shortcut => Self::Other,
            _ => match typed {
                Typed::Char(character) => Self::Char(character),
                Typed::Dead => Self::Dead,
                Typed::NoSingleChar => Self::Other,
            },
        }
    }
}

/// A pressed key by the rule that it has, whatever the key code of the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// The Tab key.
    Tab,
    /// The Escape key.
    Escape,
    /// Return or Enter, also on the keypad.
    Enter,
    /// The key that deletes the character before the caret.
    Backspace,
    /// Any other key. What it types gives the key.
    Other,
}

/// The modifiers held during a key press, by what they mean for typing. Each platform maps its
/// modifiers to one chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chord {
    /// No modifier.
    Plain,
    /// Shift only: still typing.
    Shift,
    /// A modifier that changes the typed character, such as Option on macOS, or Control and `AltGr`
    /// on Windows. What the press types gives the key.
    CharacterModifier,
    /// A modifier for shortcuts, such as Command on macOS or Alt on Windows.
    Shortcut,
}

/// What a key press types in the active keyboard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typed {
    /// Exactly one character.
    Char(char),
    /// No character yet: a dead key, whose accent comes with the next key.
    Dead,
    /// No character, several characters, or a character that does not type text.
    NoSingleChar,
}

/// What happens to an input event after TypeAlike handles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The focused app receives the event.
    Pass,
    /// The event is dropped before any app sees it.
    Swallow,
}

/// Swallows the release of a key whose press was swallowed, so the focused app sees neither.
/// `Code` is the key code of the platform.
///
/// ```
/// use typealike_core::key::{Disposition, ReleaseFilter};
///
/// const TAB: u16 = 48;
/// let mut filter = ReleaseFilter::default();
/// filter.on_press(TAB, Disposition::Swallow);
///
/// assert_eq!(filter.on_release(TAB), Disposition::Swallow);
/// ```
#[derive(Debug)]
pub struct ReleaseFilter<Code> {
    swallowed: Option<Code>,
}

impl<Code> Default for ReleaseFilter<Code> {
    fn default() -> Self {
        Self { swallowed: None }
    }
}

impl<Code: Copy + PartialEq> ReleaseFilter<Code> {
    /// Records how the press of `code` was handled.
    pub fn on_press(&mut self, code: Code, disposition: Disposition) {
        match disposition {
            Disposition::Swallow => self.swallowed = Some(code),
            Disposition::Pass if self.swallowed == Some(code) => self.swallowed = None,
            Disposition::Pass => {}
        }
    }

    /// Returns how the release of `code` must be handled.
    pub fn on_release(&mut self, code: Code) -> Disposition {
        if self.swallowed != Some(code) {
            return Disposition::Pass;
        }
        self.swallowed = None;
        Disposition::Swallow
    }
}

/// Swallows a left click on the badge: the press of the left button and its release, so the app
/// under the badge gets neither.
///
/// ```
/// use typealike_core::key::{BadgeClickFilter, Disposition};
///
/// let mut filter = BadgeClickFilter::default();
/// filter.on_badge_press();
///
/// assert_eq!(filter.on_release(), Disposition::Swallow);
/// ```
#[derive(Debug, Default)]
pub struct BadgeClickFilter {
    is_pressed: bool,
}

impl BadgeClickFilter {
    /// Records a press of the left button on the badge, and returns how the press must be handled.
    pub fn on_badge_press(&mut self) -> Disposition {
        self.is_pressed = true;
        Disposition::Swallow
    }

    /// Returns how a release of the left button must be handled.
    pub fn on_release(&mut self) -> Disposition {
        if mem::take(&mut self.is_pressed) {
            Disposition::Swallow
        } else {
            Disposition::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_key_without_a_modifier_keeps_its_role() {
        let cases = [
            (KeyKind::Tab, Chord::Plain, Typed::Char('\t'), Key::Tab),
            (KeyKind::Escape, Chord::Plain, Typed::Char('\u{1b}'), Key::Escape),
            (KeyKind::Enter, Chord::Plain, Typed::Char('\r'), Key::Enter),
            (KeyKind::Enter, Chord::Plain, Typed::Char('\u{3}'), Key::Enter),
            (
                KeyKind::Backspace,
                Chord::Plain,
                Typed::Char('\u{8}'),
                Key::Backspace,
            ),
            (
                KeyKind::Backspace,
                Chord::Plain,
                Typed::Char('\u{7f}'),
                Key::Backspace,
            ),
        ];

        let keys = cases.map(|(kind, chord, typed, _)| Key::from_press(kind, chord, typed));

        assert_eq!(keys, cases.map(|(.., key)| key));
    }

    #[test]
    fn shift_keeps_enter_and_backspace_but_not_tab() {
        let cases = [
            (KeyKind::Enter, Chord::Shift, Typed::Char('\r'), Key::Enter),
            (
                KeyKind::Backspace,
                Chord::Shift,
                Typed::Char('\u{8}'),
                Key::Backspace,
            ),
            (KeyKind::Tab, Chord::Shift, Typed::NoSingleChar, Key::Other),
            (KeyKind::Tab, Chord::Shift, Typed::Char('\t'), Key::Char('\t')),
        ];

        let keys = cases.map(|(kind, chord, typed, _)| Key::from_press(kind, chord, typed));

        assert_eq!(keys, cases.map(|(.., key)| key));
    }

    #[test]
    fn character_modifier_gives_the_typed_text_of_an_editing_key() {
        let cases = [
            (
                KeyKind::Enter,
                Chord::CharacterModifier,
                Typed::Char('\n'),
                Key::Char('\n'),
            ),
            (
                KeyKind::Enter,
                Chord::CharacterModifier,
                Typed::Char('\r'),
                Key::Char('\r'),
            ),
            (
                KeyKind::Backspace,
                Chord::CharacterModifier,
                Typed::NoSingleChar,
                Key::Other,
            ),
            (
                KeyKind::Backspace,
                Chord::CharacterModifier,
                Typed::Char('\u{7f}'),
                Key::Char('\u{7f}'),
            ),
        ];

        let keys = cases.map(|(kind, chord, typed, _)| Key::from_press(kind, chord, typed));

        assert_eq!(keys, cases.map(|(.., key)| key));
    }

    #[test]
    fn shortcut_gives_other_whatever_the_key_types() {
        let cases = [
            (KeyKind::Tab, Chord::Shortcut, Typed::NoSingleChar, Key::Other),
            (KeyKind::Tab, Chord::Shortcut, Typed::Char('\t'), Key::Other),
            (KeyKind::Enter, Chord::Shortcut, Typed::Char('\r'), Key::Other),
            (
                KeyKind::Backspace,
                Chord::Shortcut,
                Typed::Char('\u{7f}'),
                Key::Other,
            ),
            (KeyKind::Other, Chord::Shortcut, Typed::Char('a'), Key::Other),
            (KeyKind::Other, Chord::Shortcut, Typed::Dead, Key::Other),
        ];

        let keys = cases.map(|(kind, chord, typed, _)| Key::from_press(kind, chord, typed));

        assert_eq!(keys, cases.map(|(.., key)| key));
    }

    #[test]
    fn other_key_gives_what_it_types() {
        let cases = [
            (KeyKind::Other, Chord::Shift, Typed::Char('A'), Key::Char('A')),
            (
                KeyKind::Other,
                Chord::CharacterModifier,
                Typed::Char('@'),
                Key::Char('@'),
            ),
            (
                KeyKind::Other,
                Chord::CharacterModifier,
                Typed::Char('å'),
                Key::Char('å'),
            ),
            (KeyKind::Other, Chord::Plain, Typed::Dead, Key::Dead),
            (KeyKind::Other, Chord::CharacterModifier, Typed::Dead, Key::Dead),
            (KeyKind::Other, Chord::Plain, Typed::NoSingleChar, Key::Other),
        ];

        let keys = cases.map(|(kind, chord, typed, _)| Key::from_press(kind, chord, typed));

        assert_eq!(keys, cases.map(|(.., key)| key));
    }

    const TAB: u16 = 48;
    const LETTER_A: u16 = 0;

    #[test]
    fn swallowed_press_swallows_its_release() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB, Disposition::Swallow);

        assert_eq!(filter.on_release(TAB), Disposition::Swallow);
    }

    #[test]
    fn passed_press_passes_its_release() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB, Disposition::Pass);

        assert_eq!(filter.on_release(TAB), Disposition::Pass);
    }

    #[test]
    fn release_of_other_key_passes() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB, Disposition::Swallow);

        assert_eq!(filter.on_release(LETTER_A), Disposition::Pass);
    }

    #[test]
    fn release_is_swallowed_once() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB, Disposition::Swallow);
        filter.on_release(TAB);

        assert_eq!(filter.on_release(TAB), Disposition::Pass);
    }

    #[test]
    fn release_passes_after_repeated_press_passed() {
        let mut filter = ReleaseFilter::default();
        filter.on_press(TAB, Disposition::Swallow);
        filter.on_press(TAB, Disposition::Pass);

        assert_eq!(filter.on_release(TAB), Disposition::Pass);
    }

    #[test]
    fn press_on_the_badge_is_swallowed() {
        let mut filter = BadgeClickFilter::default();

        assert_eq!(filter.on_badge_press(), Disposition::Swallow);
    }

    #[test]
    fn release_after_a_press_on_the_badge_is_swallowed() {
        let mut filter = BadgeClickFilter::default();
        filter.on_badge_press();

        assert_eq!(filter.on_release(), Disposition::Swallow);
    }

    #[test]
    fn release_without_a_press_on_the_badge_passes() {
        let mut filter = BadgeClickFilter::default();

        assert_eq!(filter.on_release(), Disposition::Pass);
    }

    #[test]
    fn badge_release_is_swallowed_once() {
        let mut filter = BadgeClickFilter::default();
        filter.on_badge_press();
        filter.on_release();

        assert_eq!(filter.on_release(), Disposition::Pass);
    }
}
