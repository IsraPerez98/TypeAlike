//! The languages that the user writes, and the check of the spelling of suggestions.

/// Whether a suggestion stops before a word that no dictionary of the languages of the user knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpellCheck {
    /// A suggestion stops before a word that no dictionary knows and that the user did not write,
    /// such as a word that the model made up. Slang and rare names can also stop it.
    #[default]
    On,
    /// A suggestion shows all its words.
    Off,
}

/// The languages that the user writes, and what suggestions do with their dictionaries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpellingSettings {
    /// The languages that the user writes, tags of
    /// [`WRITING_LANGUAGES`](crate::spelling::WRITING_LANGUAGES), in the order that the user added
    /// them. `None` follows the languages of the user in the settings of the OS, until the user
    /// changes the list.
    pub languages: Option<Vec<String>>,
    /// Whether a suggestion stops before a word that no dictionary knows.
    pub check: SpellCheck,
}
