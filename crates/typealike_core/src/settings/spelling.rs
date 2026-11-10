//! The languages that the user writes.

/// The languages that the user writes, whose dictionaries check the words of suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpellingSettings {
    /// The languages that the user writes, tags of
    /// [`WRITING_LANGUAGES`](crate::spelling::WRITING_LANGUAGES), in the order that the user added
    /// them. `None` follows the languages of the user in the settings of the OS, until the user
    /// changes the list.
    pub languages: Option<Vec<String>>,
}
