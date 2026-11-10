//! The answers of the dictionary of the OS from a file, so that the app of each case uses them.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use anyhow::{Context, bail};
use typealike_core::spelling::SpellChecker;

/// What the dictionary knows of a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Knowledge {
    Word,
    StartOfWord,
    Nothing,
}

/// The answers of the dictionary of the OS. Each line of its file has a text, a tab, and `word`
/// when the dictionary knows the text as a word, `start` when it knows it only as the start of a
/// word, or `none`. A text that the file does not hold gets the answers of the live dictionary of
/// the OS, else the answers of a dictionary that cannot check it: a known word, and no start of a
/// word.
#[derive(Clone, Default)]
pub struct FileDictionary {
    answers: Rc<HashMap<String, Knowledge>>,
    live: Option<Rc<dyn SpellChecker>>,
}

impl FileDictionary {
    /// Reads the answers in the file at `path`. The paths `macos` and `windows` give the
    /// dictionary of that OS that the app uses, for the preferred languages of the user.
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        if path == Path::new("macos") {
            return Self::macos();
        }
        if path == Path::new("windows") {
            return Self::windows();
        }
        let text = fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    fn parse(text: &str) -> anyhow::Result<Self> {
        let answers = text
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let Some((letters, knowledge)) = line.split_once('\t') else {
                    bail!("a line has no tab");
                };
                let knowledge = match knowledge {
                    "word" => Knowledge::Word,
                    "start" => Knowledge::StartOfWord,
                    "none" => Knowledge::Nothing,
                    _ => bail!("a line has an unknown answer"),
                };
                Ok((letters.to_owned(), knowledge))
            })
            .collect::<anyhow::Result<HashMap<_, _>>>()?;
        Ok(Self {
            answers: Rc::new(answers),
            live: None,
        })
    }

    #[cfg(target_os = "macos")]
    #[expect(clippy::unnecessary_wraps, reason = "the other OSes have no dictionary")]
    fn macos() -> anyhow::Result<Self> {
        Ok(Self {
            answers: Rc::default(),
            live: Some(Rc::new(typealike_macos::SpellingDictionary::new())),
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn macos() -> anyhow::Result<Self> {
        bail!("the macOS dictionary is only on macOS")
    }

    #[cfg(windows)]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the other OSes have no Windows dictionary"
    )]
    fn windows() -> anyhow::Result<Self> {
        Ok(Self {
            answers: Rc::default(),
            live: Some(Rc::new(typealike_windows::SpellingDictionary::new())),
        })
    }

    #[cfg(not(windows))]
    fn windows() -> anyhow::Result<Self> {
        bail!("the Windows dictionary is only on Windows")
    }
}

impl std::fmt::Debug for FileDictionary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileDictionary")
            .field("answers", &self.answers.len())
            .field("is_live", &self.live.is_some())
            .finish()
    }
}

impl SpellChecker for FileDictionary {
    fn is_known_word(&self, word: &str) -> bool {
        match (self.answers.get(word), &self.live) {
            (Some(knowledge), _) => *knowledge == Knowledge::Word,
            (None, Some(live)) => live.is_known_word(word),
            (None, None) => true,
        }
    }

    fn is_word_start(&self, letters: &str) -> bool {
        match (self.answers.get(letters), &self.live) {
            (Some(knowledge), _) => *knowledge != Knowledge::Nothing,
            (None, Some(live)) => live.is_word_start(letters),
            (None, None) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_come_from_the_file_and_a_missing_text_is_a_word_that_starts_no_word() {
        let dictionary = FileDictionary::parse("open\tword\nFrid\tstart\ndlei\tnone\n").expect("valid");
        let texts = ["open", "Frid", "dlei", "zorbly"];

        let answers = texts.map(|text| (dictionary.is_known_word(text), dictionary.is_word_start(text)));

        assert_eq!(
            answers,
            [(true, true), (false, true), (false, false), (true, false)]
        );
    }

    #[test]
    fn line_with_an_unknown_answer_is_an_error() {
        let parsed = FileDictionary::parse("open\tmaybe\n");

        assert!(parsed.is_err());
    }
}
