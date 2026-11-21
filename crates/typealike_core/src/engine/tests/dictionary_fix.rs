//! Tests the fixes of a typo that come from the guesses of the dictionary.

use crate::engine::{Engine, Reaction, WordFix};
use crate::inference::{Completion, DictionaryGuesses, InferenceError, LogProbability, TypoCheck};
use crate::key::Key;
use crate::settings::{Settings, TypoFix};
use crate::test_support::{ExpectedRequest, FakeSpellChecker, completion};

use super::support::{engine_with, press, request, type_waiting, word_checking_request_for};

/// A dictionary that does not know "comi" and "cancion", and offers the accented words for them.
fn spanish_dictionary() -> FakeSpellChecker {
    FakeSpellChecker {
        unknown: vec!["comi", "cancion"],
        guesses: vec![
            ("comi", vec!["como", "comí", "comió"]),
            ("cancion", vec!["canción", "canciones"]),
        ],
        ..FakeSpellChecker::default()
    }
}

/// Returns an engine with `settings` and the Spanish dictionary that typed `typed` and waits for
/// the completion of its last key.
fn engine_typed(settings: &Settings, typed: &str) -> Engine {
    let mut engine = engine_with(settings);
    engine.use_spell_checkers(vec![Box::new(spanish_dictionary())]);
    type_waiting(&mut engine, typed);
    engine
}

/// The request that continues `text_before`, in which the dictionary fixed the accent of the last
/// word, without the typo rule.
fn request_with_accent_fix(text_before: &str) -> Reaction {
    ExpectedRequest {
        typo_check: TypoCheck::Off,
        ..ExpectedRequest::framed(&[], text_before)
    }
    .into()
}

#[test]
fn space_after_an_unknown_word_with_one_accented_guess_asks_for_the_accented_word() {
    let mut engine = engine_typed(&Settings::default(), "Ayer comi");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, request_with_accent_fix("Ayer comí "));
}

#[test]
fn sign_and_space_after_an_unknown_word_keep_the_accent_fix() {
    let mut engine = engine_typed(&Settings::default(), "Me encanta esa cancion,");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, request_with_accent_fix("Me encanta esa canción, "));
}

#[test]
fn completion_of_the_accented_word_shows_the_fix() {
    let mut engine = engine_typed(&Settings::default(), "Ayer comi ");

    let reaction = engine.on_completion(completion("con mis primos"));

    assert_eq!(
        reaction,
        Reaction::ShowFix(WordFix {
            typo: "comi ".to_owned(),
            word: "comí ".to_owned(),
            rest: "con mis primos".to_owned(),
        })
    );
}

#[test]
fn parts_of_the_completion_of_the_accented_word_do_not_show() {
    let mut engine = engine_typed(&Settings::default(), "Ayer comi ");

    let reaction = engine.on_partial_completion("con mis ");

    assert_eq!(reaction, None);
}

#[test]
fn known_word_unfinished_word_or_two_spaces_get_no_accent_fix() {
    let cases = [("Ayer como", ' '), ("Ayer com", 'i'), ("Ayer comi ", ' ')];

    let reactions = cases.map(|(typed, key)| {
        let mut engine = engine_typed(&Settings::default(), typed);
        press(&mut engine, Key::Char(key))
    });

    assert_eq!(
        reactions,
        [
            word_checking_request_for("Ayer como "),
            request("Ayer comi"),
            request("Ayer comi  ")
        ]
    );
}

#[test]
fn space_after_an_unknown_word_with_typo_fixes_off_gets_no_accent_fix() {
    let settings = Settings {
        typo_fix: TypoFix::Off,
        ..Settings::default()
    };
    let mut engine = engine_typed(&settings, "Ayer comi");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(
        reaction,
        ExpectedRequest {
            typo_check: settings.typo_check(),
            ..ExpectedRequest::framed(&[], "Ayer comi ")
        }
        .into()
    );
}

/// A dictionary that knows neither "goverment", "docuemnto" nor "pge", nor any word that starts
/// with "goverment" or "pge", and offers guesses for them.
fn dictionary_with_guesses() -> FakeSpellChecker {
    FakeSpellChecker {
        unknown: vec!["goverment", "docuemnto", "pge"],
        unknown_starts: vec!["goverment", "pge"],
        guesses: vec![
            ("goverment", vec!["government", "governed", "garment"]),
            ("docuemnto", vec!["documento", "documentó"]),
            ("pge", vec!["pre", "pie"]),
        ],
        ..FakeSpellChecker::default()
    }
}

/// Returns an engine with the dictionary of guesses that typed `typed` and waits for the
/// completion of its last key.
fn engine_with_guesses(typed: &str) -> Engine {
    let mut engine = engine_with(&Settings::default());
    engine.use_spell_checkers(vec![Box::new(dictionary_with_guesses())]);
    type_waiting(&mut engine, typed);
    engine
}

/// The request for a suggestion after `text_before` with the `guesses` of the dictionary.
fn request_with_guesses(text_before: &str, guesses: &[&str]) -> Reaction {
    ExpectedRequest {
        dictionary_guesses: DictionaryGuesses(guesses.iter().map(|&guess| guess.to_owned()).collect()),
        ..ExpectedRequest::framed(&[], text_before)
    }
    .into()
}

/// A completion that stopped for a typo without a fix: no text, and typed letters less likely
/// than the cut of the typo rule.
#[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
fn typo_stop() -> Result<Completion, InferenceError> {
    Ok(Completion {
        typed_log_probability: LogProbability(-15.0),
        ..completion("").expect("the completion has text")
    })
}

#[test]
fn typo_stop_without_a_fix_asks_again_with_the_guesses_one_edit_away() {
    let mut engine = engine_with_guesses("talk to the goverment");

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(
        reaction,
        request_with_guesses("talk to the goverment", &["government"])
    );
}

#[test]
fn typo_stop_without_a_fix_in_fewer_than_five_letters_gets_no_guesses() {
    let mut engine = engine_with_guesses("tengo harta pge");

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn typo_stop_without_a_fix_after_the_guesses_hides_the_suggestion() {
    let mut engine = engine_with_guesses("talk to the goverment");
    engine.on_completion(typo_stop());

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn space_after_an_unknown_word_without_an_accented_guess_gives_no_guesses() {
    let mut engine = engine_with_guesses("Te mando el docuemnto");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, word_checking_request_for("Te mando el docuemnto "));
}
