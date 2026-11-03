//! The spelling dictionary of the OS, which tells which words a suggestion may write.

/// The spelling dictionary of the OS for the languages that the user writes. The platform
/// implements it, and core asks it about each word that a suggestion writes, so that a suggestion
/// never writes a word that the model made up.
pub trait SpellChecker {
    /// Whether the dictionary knows `word`, a run of letters with no whitespace, in any language
    /// that the user writes. A platform that cannot check `word` answers `true`.
    fn is_known_word(&self, word: &str) -> bool;

    /// Whether the dictionary knows `letters`, a run of letters with no whitespace, as a word or
    /// as the start of a word, in any language that the user writes. A platform that cannot check
    /// `letters` answers `false`.
    fn is_word_start(&self, letters: &str) -> bool;
}
