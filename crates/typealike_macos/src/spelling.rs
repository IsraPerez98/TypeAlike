//! Asks the spelling dictionary of macOS about the words of a suggestion and the typed letters of
//! a word.

use std::ptr;

use objc2::rc::Retained;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSNotFound, NSRange, NSString};
use typealike_core::language::closest_languages;
use typealike_core::spelling::{OsDictionaries, SpellChecker};

use crate::language::preferred_languages;

/// The spelling dictionary of macOS for the preferred languages of the user, in Language & Region.
/// It also knows the words that the user taught it with "Learn Spelling".
#[derive(Debug)]
pub struct SpellingDictionary {
    checker: Retained<NSSpellChecker>,
    languages: Vec<Retained<NSString>>,
}

impl SpellingDictionary {
    /// Connects to the spelling service of macOS for the preferred languages of the user. Call it
    /// on the main thread.
    #[must_use]
    pub fn new() -> Self {
        let languages = closest_languages(&preferred_languages(), &available_languages());
        Self::with_languages(&languages)
    }

    /// Connects to the spelling service of macOS for `languages`, each a language that macOS has a
    /// dictionary for. Call it on the main thread.
    #[must_use]
    pub fn with_languages(languages: &[String]) -> Self {
        Self {
            checker: NSSpellChecker::sharedSpellChecker(),
            languages: languages
                .iter()
                .map(|language| NSString::from_str(language))
                .collect(),
        }
    }
}

/// The languages that macOS has a spelling dictionary for, such as `en` or `en_GB`.
fn available_languages() -> Vec<String> {
    NSSpellChecker::sharedSpellChecker()
        .availableLanguages()
        .iter()
        .map(|language| language.to_string())
        .collect()
}

/// The spelling dictionaries of macOS, which the controller opens for the languages that the user
/// writes. macOS has a dictionary for many languages, so TypeAlike downloads none there.
#[derive(Debug, Default)]
pub struct MacDictionaries;

impl OsDictionaries for MacDictionaries {
    fn preferred_languages(&self) -> Vec<String> {
        preferred_languages()
    }

    fn available_languages(&self) -> Vec<String> {
        available_languages()
    }

    fn open(&self, languages: &[String]) -> Box<dyn SpellChecker> {
        Box::new(SpellingDictionary::with_languages(languages))
    }
}

impl Default for SpellingDictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl SpellChecker for SpellingDictionary {
    fn is_known_word(&self, word: &str) -> bool {
        if self.languages.is_empty() {
            return true;
        }
        let word = NSString::from_str(word);
        let not_found = usize::try_from(NSNotFound).expect("NSNotFound is positive");
        self.languages.iter().any(|language| {
            // SAFETY: a null word count tells the checker not to count the words.
            let misspelled = unsafe {
                self.checker
                    .checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                        &word,
                        0,
                        Some(language),
                        false,
                        0,
                        ptr::null_mut(),
                    )
            };
            misspelled.location == not_found
        })
    }

    fn is_word_start(&self, letters: &str) -> bool {
        if self.languages.is_empty() {
            return false;
        }
        if self.is_known_word(letters) {
            return true;
        }
        let letters = NSString::from_str(letters);
        let range = NSRange::new(0, letters.length());
        self.languages.iter().any(|language| {
            self.checker
                .completionsForPartialWordRange_inString_language_inSpellDocumentWithTag(
                    range,
                    &letters,
                    Some(language),
                    0,
                )
                .is_some_and(|completions| completions.count() > 0)
        })
    }
}
