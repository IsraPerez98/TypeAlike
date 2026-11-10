//! Measures the dictionaries that the cut of unknown words can use, with no model.
//!
//! ```sh
//! cargo run --release -p typealike_llm --example spelling_eval -- \
//!     --dictionary windows --dictionary windows+hunspell:PATH \
//!     --words tuning=crates/typealike_llm/examples/prompt_eval/cases/tuning.jsonl --words slang=slang.txt
//! ```
//!
//! Each `--dictionary` takes the value of `--dictionary` of `prompt_eval`. Each `--words NAME=FILE`
//! gives sets of words: a `.jsonl` file of cases gives a set for each language of the case ids
//! (the part before the first `-`), with the words of `before` and `next`, and of `after`. Another
//! file gives one set with all its words. Only the words that the engine checks count
//! ([`checked_words`]).
//!
//! For each dictionary, the report gives:
//! - the time to read it, and the time of the first check of each word,
//! - for each set, the part of its words that the dictionary knows, and its most frequent unknown
//!   words, which the cut removes from a suggestion although the writer uses them,
//! - for each set, the part of its made-up words that the dictionary knows. A made-up word joins
//!   the first half of a word and the second half of the next word of the set, as a model makes up
//!   a word from the tokens of two words. The cut keeps each made-up word that the dictionary
//!   knows.

#[path = "prompt_eval/dictionary.rs"]
mod dictionary;

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use typealike_core::spelling::{SpellChecker, checked_words};

use crate::dictionary::FileDictionary;

const USAGE: &str = "usage: spelling_eval --dictionary DICTIONARIES [--dictionary DICTIONARIES]... \
                     --words NAME=FILE [--words NAME=FILE]...";
/// The most unknown words that the report lists for each set.
const LISTED_UNKNOWN_WORDS: usize = 12;
/// The fewest letters of each of the two words that a made-up word joins.
const MIN_MADE_UP_PART_LETTERS: usize = 4;

/// The words of one set, in the order of the text, each time that the text has it.
#[derive(Debug, PartialEq, Eq)]
struct WordSet {
    name: String,
    words: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let mut dictionaries = Vec::new();
    let mut sets = Vec::new();
    for option in arguments.chunks(2) {
        match option {
            [name, spec] if name == "--dictionary" => dictionaries.push(spec.clone()),
            [name, words] if name == "--words" => sets.extend(read_word_sets(words)?),
            _ => bail!("{USAGE}"),
        }
    }
    if dictionaries.is_empty() || sets.is_empty() {
        bail!("{USAGE}");
    }
    let made_up = sets
        .iter()
        .map(|set| WordSet {
            name: format!("made-up {}", set.name),
            words: made_up_words(&set.words),
        })
        .collect::<Vec<_>>();
    let mut output = io::stdout().lock();
    for spec in &dictionaries {
        let started = Instant::now();
        let dictionary = FileDictionary::read(spec)?;
        let load_time = started.elapsed();
        let measured = Measured {
            dictionary: &dictionary,
            load_time,
        };
        report(&mut output, spec, &measured, &sets, &made_up)?;
    }
    Ok(())
}

/// Reads the sets of words of `option`, a `NAME=FILE` value of `--words`.
fn read_word_sets(option: &str) -> anyhow::Result<Vec<WordSet>> {
    let Some((name, path)) = option.split_once('=') else {
        bail!("{USAGE}");
    };
    let text = fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
    let is_case_file = Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"));
    if !is_case_file {
        return Ok(vec![WordSet {
            name: name.to_owned(),
            words: words_of(&text),
        }]);
    }
    let mut sets = Vec::<WordSet>::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let case = serde_json::from_str::<serde_json::Value>(line).with_context(|| format!("in {path}"))?;
        let field = |key| case[key].as_str().unwrap_or_default();
        let language = field("id").split('-').next().unwrap_or_default();
        let set_name = format!("{name}.{language}");
        let mut words = words_of(&format!("{}{}", field("before"), field("next")));
        words.extend(words_of(field("after")));
        match sets.iter_mut().find(|set| set.name == set_name) {
            Some(set) => set.words.extend(words),
            None => sets.push(WordSet {
                name: set_name,
                words,
            }),
        }
    }
    Ok(sets)
}

/// Returns the words of `text` that the engine checks.
fn words_of(text: &str) -> Vec<String> {
    text.split_whitespace()
        .flat_map(checked_words)
        .map(str::to_owned)
        .collect()
}

/// Returns a made-up word for each two neighbor words of `words` with enough letters: the first
/// half of the first word and the second half of the next word, each one time.
fn made_up_words(words: &[String]) -> Vec<String> {
    let mut made_up = Vec::<String>::new();
    for pair in words.windows(2) {
        let [first, next] = pair else { continue };
        let (first, next) = (
            first.chars().collect::<Vec<_>>(),
            next.chars().collect::<Vec<_>>(),
        );
        if first.len() < MIN_MADE_UP_PART_LETTERS || next.len() < MIN_MADE_UP_PART_LETTERS {
            continue;
        }
        let word = first[..first.len() / 2]
            .iter()
            .chain(&next[next.len() / 2..])
            .collect::<String>();
        if !made_up.contains(&word) && !words.contains(&word) {
            made_up.push(word);
        }
    }
    made_up
}

/// A dictionary and the time to read it.
struct Measured<'dictionary> {
    dictionary: &'dictionary FileDictionary,
    load_time: Duration,
}

/// Writes to `output` what the `measured` dictionary knows of each of the `sets` and of the
/// `made_up` words.
fn report(
    output: &mut impl Write,
    spec: &str,
    measured: &Measured<'_>,
    sets: &[WordSet],
    made_up: &[WordSet],
) -> io::Result<()> {
    let mut answers = HashMap::<String, bool>::new();
    let mut check_time = Duration::ZERO;
    let mut known = |word: &str| -> bool {
        if let Some(answer) = answers.get(word) {
            return *answer;
        }
        let started = Instant::now();
        let answer = measured.dictionary.is_known_word(word);
        check_time += started.elapsed();
        answers.insert(word.to_owned(), answer);
        answer
    };
    writeln!(output, "{spec}")?;
    writeln!(
        output,
        "  set                        words   known   most frequent unknown words"
    )?;
    for set in sets.iter().chain(made_up) {
        let mut unknown = HashMap::<&str, usize>::new();
        for word in set.words.iter().filter(|word| !known(word)) {
            *unknown.entry(word).or_default() += 1;
        }
        let unknown_count = unknown.values().sum::<usize>();
        let mut listed = unknown.into_iter().collect::<Vec<_>>();
        listed.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
        let listed = listed
            .iter()
            .take(LISTED_UNKNOWN_WORDS)
            .map(|(word, count)| format!("{word} ({count})"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            output,
            "  {:26} {:5}  {:5.1}%  {listed}",
            set.name,
            set.words.len(),
            percent(set.words.len() - unknown_count, set.words.len()),
        )?;
    }
    let checks = u32::try_from(answers.len()).unwrap_or(u32::MAX).max(1);
    writeln!(
        output,
        "  read: {} ms, first check: {:.1} µs for each word ({} words)
",
        measured.load_time.as_millis(),
        (check_time / checks).as_secs_f64() * 1e6,
        answers.len()
    )
}

fn percent(part: usize, whole: usize) -> f64 {
    let as_float = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    100.0 * as_float(part) / as_float(whole.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| (*text).to_owned()).collect()
    }

    #[test]
    fn made_up_word_joins_the_halves_of_two_neighbor_words_with_enough_letters() {
        let words = strings(&["mañana", "compadre", "the", "tomorrow", "tomorrow", "mañana"]);

        let made_up = made_up_words(&words);

        assert_eq!(made_up, strings(&["mañadre", "tomoana"]));
    }

    #[test]
    fn words_of_a_text_are_the_words_that_the_engine_checks() {
        let words = words_of("Hola po, ¿cómo estás? Nos vemos el 15th en example.com");

        assert_eq!(words, strings(&["Hola", "cómo", "estás", "Nos", "vemos"]));
    }
}
