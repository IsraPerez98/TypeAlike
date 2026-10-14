//! Keys as the engine understands them, independent of any keyboard layout or OS key code.

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
    /// Any other key: navigation, Enter, shortcuts. The typed context is no longer reliable.
    Other,
}
