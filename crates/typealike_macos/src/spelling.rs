//! Asks the spelling dictionary of macOS about the words of a suggestion.

use std::ptr;

use objc2::rc::Retained;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSLocale, NSNotFound, NSString};
use typealike_core::language::closest_languages;
use typealike_core::spelling::SpellChecker;

/// The spelling dictionary of macOS for the preferred languages of the user, in Language & Region.
/// It also knows the words that the user taught it with "Learn Spelling".
#[derive(Debug)]
pub struct SpellingDictionary {
    checker: Retained<NSSpellChecker>,
    languages: Vec<Retained<NSString>>,
}

impl SpellingDictionary {
    /// Connects to the spelling service of macOS. Call it on the main thread.
    #[must_use]
    pub fn new() -> Self {
        let checker = NSSpellChecker::sharedSpellChecker();
        let available = checker
            .availableLanguages()
            .iter()
            .map(|language| language.to_string())
            .collect::<Vec<_>>();
        let preferred = NSLocale::preferredLanguages()
            .iter()
            .map(|language| language.to_string())
            .collect::<Vec<_>>();
        let languages = closest_languages(&preferred, &available)
            .iter()
            .map(|language| NSString::from_str(language))
            .collect();
        Self { checker, languages }
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
}
