//! The seams to the dictionaries of the OS and to the dictionaries that TypeAlike downloads, and
//! the spell checker that joins them.

use std::fmt;
use std::sync::Arc;

use super::SpellChecker;
use crate::hub::{ByteSize, HubError};

/// The spelling dictionaries of the OS. The platform implements it.
pub trait OsDictionaries {
    /// The languages of the user in the settings of the OS, such as `es-CL`, most preferred
    /// first.
    fn preferred_languages(&self) -> Vec<String>;

    /// The languages that the OS has a dictionary for, such as `en-US` or `en_GB`.
    fn available_languages(&self) -> Vec<String>;

    /// Opens the dictionaries of `languages`, each one of [`Self::available_languages`]. The
    /// controller calls it on its own thread, which then checks the words.
    fn open(&self, languages: &[String]) -> Box<dyn SpellChecker>;
}

/// A dictionary that a [`DictionaryHub`] can download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadableDictionary {
    /// The language of the dictionary, a tag of [`WRITING_LANGUAGES`](super::WRITING_LANGUAGES).
    pub language: String,
    /// The size of the download.
    pub size: ByteSize,
}

/// Work for a [`DictionaryHub`]. Each request answers with the [`DictionaryEvent`]s named in its
/// variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictionaryRequest {
    /// Lists the languages of the downloaded dictionaries: [`DictionaryEvent::Installed`].
    ListInstalled,
    /// Reads the downloaded dictionary of `language`, so that [`DictionaryHub::open`] can check its
    /// words: [`DictionaryEvent::Loaded`].
    Load {
        /// The language of the dictionary.
        language: String,
    },
    /// Downloads the dictionary of `language`: [`DictionaryEvent::DownloadProgress`] while it
    /// runs, then [`DictionaryEvent::DownloadFinished`].
    Download {
        /// The language of the dictionary.
        language: String,
    },
    /// Stops a running download of the dictionary of `language`, which then finishes with
    /// [`HubError::Canceled`].
    CancelDownload {
        /// The language of the dictionary.
        language: String,
    },
    /// Deletes the downloaded dictionary of `language`, and stops to check its words. No event
    /// follows.
    Delete {
        /// The language of the dictionary.
        language: String,
    },
}

/// The outcome of a [`DictionaryRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictionaryEvent {
    /// The languages of the downloaded dictionaries.
    Installed(Vec<String>),
    /// A read of the dictionary of `language` ended.
    Loaded {
        /// The language of the dictionary.
        language: String,
        /// Why the read failed, if it failed.
        result: Result<(), HubError>,
    },
    /// A running download of the dictionary of `language` stored `received` bytes so far.
    DownloadProgress {
        /// The language of the dictionary.
        language: String,
        /// Bytes stored so far.
        received: ByteSize,
    },
    /// A download of the dictionary of `language` ended.
    DownloadFinished {
        /// The language of the dictionary.
        language: String,
        /// The stored size, or why the download stopped.
        result: Result<ByteSize, HubError>,
    },
}

/// Receives dictionary events. The hub may call it from any thread.
pub type DictionaryEventSink = Arc<dyn Fn(DictionaryEvent) + Send + Sync>;

/// Downloads, reads and deletes the dictionaries of the languages that the OS has no dictionary
/// for, without blocking the caller.
pub trait DictionaryHub {
    /// The dictionaries that the hub can download.
    fn catalog(&self) -> Vec<DownloadableDictionary>;

    /// Starts `request` and returns at once. Its events reach `sink` in the order they happen.
    fn submit(&mut self, request: DictionaryRequest, sink: &DictionaryEventSink);

    /// A spell checker for the dictionaries of `languages` that the hub read. It knows no word of
    /// another language.
    fn open(&self, languages: &[String]) -> Box<dyn SpellChecker>;
}

/// Spell checkers that know a word when any of them knows it. Without a spell checker, it knows
/// each word and no start of a word, as a platform that cannot check them.
pub(crate) struct JoinedSpellCheckers(pub(crate) Vec<Box<dyn SpellChecker>>);

impl fmt::Debug for JoinedSpellCheckers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("JoinedSpellCheckers")
            .field(&self.0.len())
            .finish()
    }
}

impl SpellChecker for JoinedSpellCheckers {
    fn is_known_word(&self, word: &str) -> bool {
        self.0.is_empty() || self.0.iter().any(|checker| checker.is_known_word(word))
    }

    fn is_word_start(&self, letters: &str) -> bool {
        self.0.iter().any(|checker| checker.is_word_start(letters))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeSpellChecker;

    #[test]
    fn joined_spell_checkers_know_a_word_that_any_of_them_knows() {
        let joined = JoinedSpellCheckers(vec![
            Box::new(FakeSpellChecker::without(&["casa", "zorbly"])),
            Box::new(FakeSpellChecker::without(&["house", "zorbly"])),
        ]);

        let answers = ["casa", "house", "zorbly"].map(|word| joined.is_known_word(word));

        assert_eq!(answers, [true, true, false]);
    }

    #[test]
    fn no_spell_checker_knows_each_word_and_no_start_of_a_word() {
        let joined = JoinedSpellCheckers(Vec::new());

        let answers = (joined.is_known_word("zorbly"), joined.is_word_start("zor"));

        assert_eq!(answers, (true, false));
    }
}
