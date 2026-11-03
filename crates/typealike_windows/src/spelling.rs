//! Asks the spelling dictionaries of Windows about the words of a suggestion and the typed letters
//! of a word.

use typealike_core::language::closest_languages;
use typealike_core::spelling::SpellChecker;
use windows::Win32::Foundation::S_OK;
use windows::Win32::Globalization::{ISpellChecker, ISpellCheckerFactory, SpellCheckerFactory};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IEnumString};
use windows::core::{HSTRING, PWSTR, Result};

use crate::language::preferred_languages;
use crate::uia::ComApartment;

/// The spelling dictionaries of Windows for the languages of the user, in Time & language >
/// Language & region. They also know the words that the user added to the dictionary.
#[derive(Debug)]
pub struct SpellingDictionary {
    checkers: Vec<ISpellChecker>,
    /// Declared after `checkers`, so that COM ends on this thread after the last dictionary goes.
    _apartment: Option<ComApartment>,
}

impl SpellingDictionary {
    /// Opens a dictionary for each language of the user that Windows can check. Call it on the
    /// thread that checks the words. Without a dictionary, it knows each word.
    #[must_use]
    pub fn new() -> Self {
        let opened = ComApartment::join().and_then(|apartment| Ok((open_checkers()?, apartment)));
        match opened {
            Ok((checkers, apartment)) => Self {
                checkers,
                _apartment: Some(apartment),
            },
            Err(error) => {
                tracing::warn!(%error, "the spelling dictionaries are unavailable");
                Self {
                    checkers: Vec::new(),
                    _apartment: None,
                }
            }
        }
    }
}

impl Default for SpellingDictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl SpellChecker for SpellingDictionary {
    fn is_known_word(&self, word: &str) -> bool {
        let word = HSTRING::from(word);
        self.checkers.is_empty() || self.checkers.iter().any(|checker| has_no_error(checker, &word))
    }

    fn is_word_start(&self, letters: &str) -> bool {
        // ISpellChecker gives no completions, so only letters that are a whole word count.
        !self.checkers.is_empty() && self.is_known_word(letters)
    }
}

/// Whether `checker` finds no error in `word`. A check that fails finds none.
fn has_no_error(checker: &ISpellChecker, word: &HSTRING) -> bool {
    // SAFETY: COM is initialized on this thread, and `word` lives for the call.
    let Ok(errors) = (unsafe { checker.Check(word) }) else {
        return true;
    };
    let mut error = None;
    // SAFETY: `error` is a valid place for one error.
    let next = unsafe { errors.Next(&raw mut error) };
    next.is_err() || error.is_none()
}

/// Opens a dictionary for each language of the user that has one.
fn open_checkers() -> Result<Vec<ISpellChecker>> {
    // SAFETY: plain call; COM is initialized on this thread.
    let factory: ISpellCheckerFactory =
        unsafe { CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER) }?;
    let preferred = preferred_languages();
    // SAFETY: plain call on a live factory.
    let available = strings_of(&unsafe { factory.SupportedLanguages() }?);
    closest_languages(&preferred, &available)
        .iter()
        // SAFETY: plain call on a live factory, with a language that it supports.
        .map(|language| unsafe { factory.CreateSpellChecker(&HSTRING::from(language)) })
        .collect()
}

/// Returns each string of `strings`, and frees it.
fn strings_of(strings: &IEnumString) -> Vec<String> {
    let mut texts = Vec::new();
    loop {
        let mut text = [PWSTR::null()];
        let mut fetched = 0;
        // SAFETY: `text` has room for one string, and `fetched` is a valid place for the count.
        let next = unsafe { strings.Next(&mut text, Some(&raw mut fetched)) };
        if next != S_OK || fetched == 0 {
            return texts;
        }
        // SAFETY: the enumerator gave a string with a null at its end, which the caller owns.
        texts.push(unsafe { text[0].to_string() }.unwrap_or_default());
        // SAFETY: frees the string that the enumerator gave, one time.
        unsafe { CoTaskMemFree(Some(text[0].0.cast_const().cast())) };
    }
}
