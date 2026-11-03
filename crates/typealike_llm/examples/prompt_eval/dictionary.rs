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
/// word, or `none`. A text that the file does not hold gets the answers of a dictionary that cannot
/// check it: a known word, and no start of a word.
#[derive(Debug, Clone, Default)]
pub struct FileDictionary {
    answers: Rc<HashMap<String, Knowledge>>,
}

impl FileDictionary {
    /// Reads the answers in the file at `path`.
    pub fn read(path: &Path) -> anyhow::Result<Self> {
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
        })
    }
}

impl SpellChecker for FileDictionary {
    fn is_known_word(&self, word: &str) -> bool {
        self.answers
            .get(word)
            .is_none_or(|knowledge| *knowledge == Knowledge::Word)
    }

    fn is_word_start(&self, letters: &str) -> bool {
        self.answers
            .get(letters)
            .is_some_and(|knowledge| *knowledge != Knowledge::Nothing)
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
