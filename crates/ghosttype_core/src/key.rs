//! Keys as the engine understands them, independent of any keyboard layout or OS key code, and
//! what happens to the key events of the platform.

/// A key press relevant to suggestions, already translated by the platform layer.
///
/// Modifier-only presses are not keys; platforms drop them before they reach the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A key that types this character into the focused text.
    Char(char),
    /// Deletes the character before the caret.
    Backspace,
    /// Plain Tab without modifiers; accepts a visible suggestion.
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

/// What happens to an input event after GhostType handles it.
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
/// use ghosttype_core::key::{Disposition, ReleaseFilter};
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
    /// Records how the press of `key` was handled.
    pub fn on_press(&mut self, key: Code, disposition: Disposition) {
        match disposition {
            Disposition::Swallow => self.swallowed = Some(key),
            Disposition::Pass if self.swallowed == Some(key) => self.swallowed = None,
            Disposition::Pass => {}
        }
    }

    /// Returns how the release of `key` must be handled.
    pub fn on_release(&mut self, key: Code) -> Disposition {
        if self.swallowed != Some(key) {
            return Disposition::Pass;
        }
        self.swallowed = None;
        Disposition::Swallow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
