//! The dictionary of the OS for the app of each case: the answers of a file, or one or more live
//! dictionaries.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use anyhow::{Context, anyhow, bail};
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
/// word, or `none`. A text that the file does not hold gets the answers of the live dictionaries,
/// which know a text when any of them knows it, else the answers of a dictionary that cannot check
/// it: a known word, and no start of a word.
#[derive(Clone, Default)]
pub struct FileDictionary {
    answers: Rc<HashMap<String, Knowledge>>,
    live: Vec<Rc<dyn SpellChecker>>,
}

impl FileDictionary {
    /// Reads the dictionary that `spec` names: a file of answers, or live dictionaries joined by
    /// `+`.
    /// - `macos` and `windows` give the dictionary of that OS that the app uses, for the preferred
    ///   languages of the user.
    /// - `windows:es-CL,en-US` gives the dictionaries of Windows for those languages.
    /// - `hunspell:PATH` gives the Hunspell dictionary in `PATH.aff` and `PATH.dic`.
    pub fn read(spec: &str) -> anyhow::Result<Self> {
        let live = spec
            .split('+')
            .map(live_dictionary)
            .collect::<Option<anyhow::Result<Vec<_>>>>();
        if let Some(live) = live {
            return Ok(Self {
                answers: Rc::default(),
                live: live?,
            });
        }
        let text = fs::read_to_string(spec).with_context(|| format!("cannot read {spec}"))?;
        Self::parse(&text).with_context(|| format!("in {spec}"))
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
            live: Vec::new(),
        })
    }
}

/// Opens the live dictionary that `part` of a `--dictionary` value names, or returns `None` when
/// `part` names no live dictionary.
fn live_dictionary(part: &str) -> Option<anyhow::Result<Rc<dyn SpellChecker>>> {
    match part.split_once(':').unwrap_or((part, "")) {
        ("macos", "") => Some(macos()),
        ("windows", "") => Some(windows(None)),
        ("windows", languages) => Some(windows(Some(languages))),
        ("hunspell", path) if !path.is_empty() => Some(hunspell(Path::new(path))),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
#[expect(clippy::unnecessary_wraps, reason = "the other OSes have no dictionary")]
fn macos() -> anyhow::Result<Rc<dyn SpellChecker>> {
    Ok(Rc::new(typealike_macos::SpellingDictionary::new()))
}

#[cfg(not(target_os = "macos"))]
fn macos() -> anyhow::Result<Rc<dyn SpellChecker>> {
    bail!("the macOS dictionary is only on macOS")
}

/// Opens the dictionaries of Windows for the `languages` joined by commas, else for the preferred
/// languages of the user.
#[cfg(windows)]
fn windows(languages: Option<&str>) -> anyhow::Result<Rc<dyn SpellChecker>> {
    let dictionary = match languages {
        Some(languages) => typealike_windows::SpellingDictionary::with_languages(
            &languages.split(',').map(str::to_owned).collect::<Vec<_>>(),
        ),
        None => typealike_windows::SpellingDictionary::new(),
    };
    if dictionary.languages().is_empty() {
        bail!(
            "Windows has no dictionary for {}",
            languages.unwrap_or("the languages of the user")
        );
    }
    tracing::info!(languages = ?dictionary.languages(), "the dictionaries of Windows");
    Ok(Rc::new(dictionary))
}

#[cfg(not(windows))]
fn windows(_languages: Option<&str>) -> anyhow::Result<Rc<dyn SpellChecker>> {
    bail!("the Windows dictionary is only on Windows")
}

/// Reads the Hunspell dictionary in `path` with the extensions `aff` and `dic`.
fn hunspell(path: &Path) -> anyhow::Result<Rc<dyn SpellChecker>> {
    let read = |extension| {
        let file = path.with_extension(extension);
        fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))
    };
    // The error of spellbook is not a `std::error::Error`, because spellbook has no `std`.
    let dictionary = spellbook::Dictionary::new(&read("aff")?, &read("dic")?)
        .map_err(|error| anyhow!("cannot parse the Hunspell dictionary {}: {error}", path.display()))?;
    Ok(Rc::new(HunspellDictionary(dictionary)))
}

/// A Hunspell dictionary.
struct HunspellDictionary(spellbook::Dictionary);

impl SpellChecker for HunspellDictionary {
    fn is_known_word(&self, word: &str) -> bool {
        self.0.check(word)
    }

    fn is_word_start(&self, letters: &str) -> bool {
        // Hunspell gives no completions, so only letters that are a whole word count, as on Windows.
        self.0.check(letters)
    }
}

impl std::fmt::Debug for FileDictionary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileDictionary")
            .field("answers", &self.answers.len())
            .field("live", &self.live.len())
            .finish()
    }
}

impl SpellChecker for FileDictionary {
    fn is_known_word(&self, word: &str) -> bool {
        match self.answers.get(word) {
            Some(knowledge) => *knowledge == Knowledge::Word,
            None => self.live.is_empty() || self.live.iter().any(|live| live.is_known_word(word)),
        }
    }

    fn is_word_start(&self, letters: &str) -> bool {
        match self.answers.get(letters) {
            Some(knowledge) => *knowledge != Knowledge::Nothing,
            None => self.live.iter().any(|live| live.is_word_start(letters)),
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

    #[test]
    fn joined_hunspell_dictionaries_know_the_words_of_each() {
        let folder = std::env::temp_dir().join(format!("prompt_eval_hunspell_{}", std::process::id()));
        fs::create_dir_all(&folder).expect("temp folder");
        for (name, word) in [("one", "casa"), ("two", "house")] {
            fs::write(folder.join(format!("{name}.aff")), "SET UTF-8\n").expect("aff");
            fs::write(folder.join(format!("{name}.dic")), format!("1\n{word}\n")).expect("dic");
        }
        let spec = format!(
            "hunspell:{}+hunspell:{}",
            folder.join("one").display(),
            folder.join("two").display()
        );

        let dictionary = FileDictionary::read(&spec).expect("valid");
        let answers = ["casa", "house", "zorbly"].map(|word| dictionary.is_known_word(word));

        fs::remove_dir_all(&folder).expect("remove temp folder");
        assert_eq!(answers, [true, true, false]);
    }
}
