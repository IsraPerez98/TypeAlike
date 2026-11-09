//! Tests how the dictionary of the OS cuts a suggestion before a word that the model made up.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use crate::engine::{Engine, Reaction, WordFix};
use crate::inference::{Completion, Correction, InferenceError, LogProbability, TypoCheck};
use crate::key::{Key, Press};
use crate::settings::{ClipboardAccess, HistoryAccess, HistoryApp, HistoryScope, Settings, WordChoice};
use crate::test_support::{
    ExpectedRequest, FakeSpellChecker, completion, mail_entry, mail_history_app, on_own_days,
};

use super::support::{
    FakeDesktop, engine_waiting_for, engine_waiting_with, engine_with, engine_with_history, history_settings,
    show,
};

/// Returns an engine with `settings` that typed `typed`, and a dictionary without the `unknown`
/// words.
fn engine_typed(settings: &Settings, typed: &str, unknown: &[&'static str]) -> Engine {
    let mut engine = engine_waiting_with(settings, typed);
    engine.use_spell_checker(Box::new(FakeSpellChecker::without(unknown)));
    engine
}

/// Types `typed` with a dictionary without the `unknown` words, and returns the reaction to the
/// completion `text`.
fn reaction_after(typed: &str, unknown: &[&'static str], text: &str) -> Reaction {
    engine_typed(&Settings::default(), typed, unknown).on_completion(completion(text))
}

#[test]
fn suggestion_ends_before_a_word_that_the_dictionary_does_not_know() {
    let reaction = reaction_after("Ya ", &["prostu"], "vamos prostu nie");

    assert_eq!(reaction, show("vamos"));
}

#[test]
fn unknown_word_that_finishes_the_typed_letters_hides_the_suggestion() {
    let reaction = reaction_after("Call Tu", &["Turjal"], "rjal today.");

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn unknown_word_that_the_text_before_the_caret_holds_stays() {
    let reaction = reaction_after("turjal is open, so ", &["Turjal"], "Turjal it is");

    assert_eq!(reaction, show("Turjal it is"));
}

#[test]
fn unknown_word_that_the_preface_holds_stays() {
    let settings = Settings {
        clipboard_access: ClipboardAccess::Allowed,
        ..Settings::default()
    };
    let mut engine = engine_with(&settings);
    engine.use_spell_checker(Box::new(FakeSpellChecker::without(&["Turjal"])));
    let desktop = FakeDesktop::with_field_text("").with_copy("Turjal Road 12", Some(Duration::from_secs(1)));
    engine.on_key(Key::Char('I'), Press::First, &desktop);

    let reaction = engine.on_completion(completion(" live on Turjal Road"));

    assert_eq!(reaction, show(" live on Turjal Road"));
}

#[test]
fn words_with_digits_signs_inside_or_fewer_than_three_letters_need_no_dictionary() {
    let reaction = reaction_after(
        "Ya ",
        &["th", "po", "example", "useState"],
        "po, see example.com and useState on the 15th",
    );

    assert_eq!(reaction, show("po, see example.com and useState on the 15th"));
}

const CHAT_APP: &str = "com.tinyspeck.slackmacgap";

/// The settings of a pool of the mail app and a chat app with `word_choice`.
fn shared_pool_settings(word_choice: WordChoice) -> Settings {
    let mut settings = history_settings(
        HistoryAccess::On,
        vec![
            mail_history_app(HistoryScope::AllApps),
            HistoryApp {
                app_id: CHAT_APP.to_owned(),
                app_name: "Chat".to_owned(),
                app_path: None,
                scope: HistoryScope::AllApps,
            },
        ],
    );
    settings.personalization.word_choice = word_choice;
    settings
}

#[test]
fn unknown_word_stays_only_when_the_word_choice_prefers_it() {
    let word_choices = [WordChoice::Medium, WordChoice::Off];

    let reactions = word_choices.map(|word_choice| {
        let mut engine = engine_with_history(
            &shared_pool_settings(word_choice),
            on_own_days(vec![mail_entry("Cachai the plan, cachai the time"); 3]),
        );
        engine.use_spell_checker(Box::new(FakeSpellChecker::without(&["cachai"])));
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_field_text("Ok").in_app(CHAT_APP),
        );
        engine.on_completion(completion("cachai el plan"))
    });

    assert_eq!(reactions, [show("cachai el plan"), Reaction::Hide]);
}

#[test]
fn dictionary_checks_each_word_once_in_a_session() {
    let checked_words = Rc::new(Cell::new(0));
    let mut engine = engine_waiting_for("We ");
    engine.use_spell_checker(Box::new(FakeSpellChecker {
        checked_words: Rc::clone(&checked_words),
        ..FakeSpellChecker::default()
    }));
    engine.on_partial_completion("meet ");
    engine.on_partial_completion("meet today ");

    engine.on_completion(completion("meet today"));

    assert_eq!(checked_words.get(), 2);
}

#[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
fn correction(typed: &str, fixed: &str, text: &str) -> Result<Completion, InferenceError> {
    Ok(Completion {
        correction: Some(Correction {
            typed: typed.to_owned(),
            fixed: fixed.to_owned(),
        }),
        ..completion(text).expect("the completion has text")
    })
}

/// A completion that stopped for a typo: no text, and typed letters less likely than the cut of
/// the typo rule.
#[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
fn typo_stop() -> Result<Completion, InferenceError> {
    Ok(Completion {
        typed_log_probability: LogProbability(-15.0),
        ..completion("").expect("the completion has text")
    })
}

/// The request for a suggestion after `text_before` without the typo rule.
fn request_without_typo_rule(text_before: &str) -> Reaction {
    ExpectedRequest {
        typo_check: TypoCheck::Off,
        ..ExpectedRequest::framed(&[], text_before)
    }
    .into()
}

#[test]
fn typo_stop_in_the_start_of_a_known_word_asks_again_without_the_typo_rule() {
    let mut engine = engine_typed(&Settings::default(), "before Frid", &[]);

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, request_without_typo_rule("before Frid"));
}

#[test]
fn typo_stop_in_letters_that_start_no_known_word_hides_the_suggestion() {
    let mut engine = engine_waiting_for("the dlei");
    engine.use_spell_checker(Box::new(FakeSpellChecker {
        unknown_starts: vec!["dlei"],
        ..FakeSpellChecker::default()
    }));

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn typo_stop_in_fewer_than_three_letters_asks_again_without_the_typo_rule() {
    let mut engine = engine_waiting_for("before Fr");
    engine.use_spell_checker(Box::new(FakeSpellChecker {
        unknown_starts: vec!["Fr"],
        ..FakeSpellChecker::default()
    }));

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, request_without_typo_rule("before Fr"));
}

#[test]
fn typo_stop_without_a_dictionary_hides_the_suggestion() {
    let mut engine = engine_waiting_for("before Frid");

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn typo_stop_after_asking_without_the_typo_rule_hides_the_suggestion() {
    let mut engine = engine_typed(&Settings::default(), "before Frid", &[]);
    engine.on_completion(typo_stop());

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_of_a_known_word_asks_again_without_the_typo_rule() {
    let mut engine = engine_typed(&Settings::default(), "the ope", &[]);

    let reaction = engine.on_completion(correction(" ope", " ove", "rall"));

    assert_eq!(reaction, request_without_typo_rule("the ope"));
}

#[test]
fn fix_of_a_known_word_shows_when_the_answer_without_the_typo_rule_makes_an_unknown_word_of_it() {
    let mut engine = engine_typed(&Settings::default(), "a day trip to Snit", &["Snitra"]);
    engine.on_completion(correction(" Snit", " Sint", "ra for the palaces"));

    let reaction = engine.on_completion(completion("ra for the palaces"));

    assert_eq!(
        reaction,
        Reaction::ShowFix(WordFix {
            typo: "Snit".to_owned(),
            word: "Sintra".to_owned(),
            rest: " for the palaces".to_owned(),
        })
    );
}

#[test]
fn fix_of_a_known_word_does_not_show_when_the_answer_without_the_typo_rule_keeps_the_word() {
    let cases = [
        ("n the door", show("n the door")),
        (" today", show(" today")),
        ("", Reaction::Hide),
    ];

    let reactions = cases.clone().map(|(text, _)| {
        let mut engine = engine_typed(&Settings::default(), "the ope", &[]);
        engine.on_completion(correction(" ope", " ove", "rall"));
        engine.on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|(_, reaction)| reaction));
}

#[test]
fn fix_of_letters_that_only_start_a_known_word_shows() {
    let mut engine = engine_typed(&Settings::default(), "los premi", &["premi"]);

    let reaction = engine.on_completion(correction(" premi", " permi", "sos"));

    assert_eq!(
        reaction,
        Reaction::ShowFix(WordFix {
            typo: "premi".to_owned(),
            word: "permisos".to_owned(),
            rest: String::new(),
        })
    );
}

#[test]
fn rest_after_a_typo_fix_ends_before_an_unknown_word() {
    let mut engine = engine_typed(&Settings::default(), "the dlei", &["dlei", "zorbly"]);

    let reaction = engine.on_completion(correction(" dlei", " deli", "very soon zorbly"));

    assert_eq!(
        reaction,
        Reaction::ShowFix(WordFix {
            typo: "dlei".to_owned(),
            word: "delivery".to_owned(),
            rest: " soon".to_owned(),
        })
    );
}

#[test]
fn fix_to_a_word_that_the_dictionary_does_not_know_hides() {
    let cases = [(&["sbaa", "sabado"][..], "do o no"), (&["sbaa", "saba"], " o no")];

    let reactions = cases.map(|(unknown, text)| {
        engine_typed(&Settings::default(), "el sbaa", unknown)
            .on_completion(correction(" sbaa", " saba", text))
    });

    assert_eq!(reactions, [Reaction::Hide, Reaction::Hide]);
}
