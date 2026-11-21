//! Tests the fixes of a typo that come from the guesses of the dictionary.

use crate::engine::{Engine, Reaction, WordFix};
use crate::inference::TypoCheck;
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
