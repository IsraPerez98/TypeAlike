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
    press, request, show, type_waiting,
};

/// Returns an engine with `settings` that typed `typed`, and a dictionary without the `unknown`
/// words.
fn engine_typed(settings: &Settings, typed: &str, unknown: &[&'static str]) -> Engine {
    let mut engine = engine_waiting_with(settings, typed);
    engine.use_spell_checkers(vec![Box::new(FakeSpellChecker::without(unknown))]);
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
    engine.use_spell_checkers(vec![Box::new(FakeSpellChecker::without(&["Turjal"]))]);
    let desktop = FakeDesktop::with_field_text("").with_copy("Turjal Road 12", Some(Duration::from_secs(1)));
    engine.on_key(Key::Char('I'), Press::First, &desktop);

    let reaction = engine.on_completion(completion(" live on Turjal Road"));

    assert_eq!(reaction, show(" live on Turjal Road"));
}

#[test]
fn dictionary_that_does_not_know_most_typed_words_cuts_no_word() {
    let unknown = [
        "Hola",
        "equipo",
        "escribo",
        "para",
        "contarles",
        "lunes",
        "reunión",
    ];

    let reaction = reaction_after(
        "Hola equipo, les escribo para ",
        &unknown,
        "contarles que el lunes hay reunión",
    );

    assert_eq!(reaction, show("contarles que el lunes hay reunión"));
}

#[test]
fn dictionary_that_knows_most_typed_words_cuts_an_unknown_word() {
    let reaction = reaction_after("We will meet the team ", &["prostu"], "today prostu nie");

    assert_eq!(reaction, show("today"));
}

#[test]
fn field_with_few_typed_words_keeps_the_fit_of_the_last_text() {
    let unknown = [
        "Hola",
        "equipo",
        "escribo",
        "para",
        "contarles",
        "Wena",
        "compadre",
    ];
    let mut engine = engine_typed(&Settings::default(), "Hola equipo, les escribo para ", &unknown);
    engine.on_completion(completion("contarles"));
    engine.reset();
    type_waiting(&mut engine, "Wena ");

    let reaction = engine.on_completion(completion("compadre, ¿cómo estás?"));

    assert_eq!(reaction, show("compadre, ¿cómo estás?"));
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

#[test]
fn command_options_need_no_dictionary() {
    let reaction = reaction_after("git log ", &["oneline", "xzf"], "--oneline -xzf --graph");

    assert_eq!(reaction, show("--oneline -xzf --graph"));
}

#[test]
fn option_that_finishes_the_typed_letters_needs_no_dictionary() {
    let reaction = reaction_after("git log --one", &["oneline"], "line --graph");

    assert_eq!(reaction, show("line --graph"));
}

#[test]
fn path_that_finishes_the_typed_letters_needs_no_dictionary() {
    let reaction = reaction_after("cat ~/.bash", &["bashrc"], "rc");

    assert_eq!(reaction, show("rc"));
}

#[test]
fn unknown_word_after_a_command_option_ends_the_suggestion() {
    let reaction = reaction_after("git log ", &["prostu"], "--oneline prostu");

    assert_eq!(reaction, show("--oneline"));
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
fn unknown_word_of_the_earlier_writing_stays_with_any_word_choice() {
    let word_choices = [WordChoice::Medium, WordChoice::Off];

    let reactions = word_choices.map(|word_choice| {
        let mut engine = engine_with_history(
            &shared_pool_settings(word_choice),
            on_own_days(vec![mail_entry("Cachai the plan, cachai the time"); 3]),
        );
        engine.use_spell_checkers(vec![Box::new(FakeSpellChecker::without(&["cachai"]))]);
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_field_text("Ok").in_app(CHAT_APP),
        );
        engine.on_completion(completion("cachai el plan"))
    });

    assert_eq!(reactions, [show("cachai el plan"), show("cachai el plan")]);
}

#[test]
fn unknown_word_that_the_earlier_writing_holds_once_stays() {
    let mut engine = engine_with_history(
        &shared_pool_settings(WordChoice::Medium),
        on_own_days(vec![mail_entry("Porfa send me the file")]),
    );
    engine.use_spell_checkers(vec![Box::new(FakeSpellChecker::without(&["porfa"]))]);
    engine.on_key(
        Key::Char(' '),
        Press::First,
        &FakeDesktop::with_field_text("Hi").in_app(CHAT_APP),
    );

    let reaction = engine.on_completion(completion("porfa call me"));

    assert_eq!(reaction, show("porfa call me"));
}

#[test]
fn dictionary_checks_each_word_once_in_a_session() {
    let checked_words = Rc::new(Cell::new(0));
    let mut engine = engine_waiting_for("We ");
    engine.use_spell_checkers(vec![Box::new(FakeSpellChecker {
        checked_words: Rc::clone(&checked_words),
        ..FakeSpellChecker::default()
    })]);
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
    engine.use_spell_checkers(vec![Box::new(FakeSpellChecker {
        unknown_starts: vec!["dlei"],
        ..FakeSpellChecker::default()
    })]);

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn typo_stop_in_fewer_than_three_letters_asks_again_without_the_typo_rule() {
    let mut engine = engine_waiting_for("before Fr");
    engine.use_spell_checkers(vec![Box::new(FakeSpellChecker {
        unknown_starts: vec!["Fr"],
        ..FakeSpellChecker::default()
    })]);

    let reaction = engine.on_completion(typo_stop());

    assert_eq!(reaction, request_without_typo_rule("before Fr"));
}

/// Types `typed` in an engine with the `dictionary`, if any, answers each request before the next
/// key with an empty completion, and returns the reaction to the last key.
fn reaction_to_the_last_key(typed: &str, dictionary: Option<FakeSpellChecker>) -> Reaction {
    let mut characters = typed.chars();
    let last = characters.next_back().expect("the typed text is not empty");
    let mut engine = engine_with(&Settings::default());
    if let Some(dictionary) = dictionary {
        engine.use_spell_checkers(vec![Box::new(dictionary)]);
    }
    type_waiting(&mut engine, characters.as_str());
    engine.on_completion(completion(""));
    press(&mut engine, Key::Char(last))
}

#[test]
fn request_for_fewer_than_three_typed_characters_with_a_dictionary_has_no_typo_rule() {
    let texts = ["before F", "before Fr", "before (F"];

    let reactions = texts.map(|text| reaction_to_the_last_key(text, Some(FakeSpellChecker::default())));

    assert_eq!(reactions, texts.map(request_without_typo_rule));
}

#[test]
fn request_for_three_unchecked_characters_or_after_a_typed_space_keeps_the_typo_rule() {
    let texts = ["see you on WEd", "see you on 2O2", "before "];

    let reactions = texts.map(|text| reaction_to_the_last_key(text, Some(FakeSpellChecker::default())));

    assert_eq!(reactions, texts.map(request));
}

#[test]
fn request_for_fewer_than_three_typed_characters_without_a_dictionary_keeps_the_typo_rule() {
    let reaction = reaction_to_the_last_key("before Fr", None);

    assert_eq!(reaction, request("before Fr"));
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
fn fix_of_a_known_word_before_a_typed_space_asks_again_without_the_typo_rule() {
    let mut engine = engine_typed(&Settings::default(), "fill in the form ", &[]);

    let reaction = engine.on_completion(correction(" form ", " from ", "the bank"));

    assert_eq!(reaction, request_without_typo_rule("fill in the form "));
}

#[test]
fn fix_of_a_known_word_before_a_typed_space_does_not_show_when_the_answer_without_the_typo_rule_shows_nothing()
 {
    let mut engine = engine_typed(&Settings::default(), "fill in the form ", &["zorbly"]);
    engine.on_completion(correction(" form ", " from ", "the bank"));

    let reaction = engine.on_completion(completion("zorbly today"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_to_an_unknown_word_before_a_typed_space_asks_again_without_the_typo_rule() {
    let mut engine = engine_typed(&Settings::default(), "una zorbla ", &["zorbla", "zarbla"]);

    let reaction = engine.on_completion(correction(" zorbla ", " zarbla ", "y shorts"));

    assert_eq!(reaction, request_without_typo_rule("una zorbla "));
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

/// Returns an engine that typed `typed` with two dictionaries: one that does not know "reunion"
/// and "hart", and one that does not know the Spanish words "les", "recuerdo", "que", "mañana"
/// and "tenemos".
fn engine_with_two_languages(typed: &str) -> Engine {
    let mut engine = engine_waiting_for(typed);
    engine.use_spell_checkers(vec![
        Box::new(FakeSpellChecker::without(&["reunion", "hart"])),
        Box::new(FakeSpellChecker::without(&[
            "les", "recuerdo", "que", "mañana", "tenemos",
        ])),
    ]);
    engine
}

fn reunion_fix() -> Reaction {
    Reaction::ShowFix(WordFix {
        typo: "reunion".to_owned(),
        word: "reunión".to_owned(),
        rest: " de las nueve".to_owned(),
    })
}

#[test]
fn fix_of_a_word_that_only_the_dictionary_of_another_language_knows_shows() {
    let mut engine = engine_with_two_languages("les recuerdo que mañana tenemos la reunion");

    let reaction = engine.on_completion(correction(" reunion", " reunión", " de las nueve"));

    assert_eq!(reaction, reunion_fix());
}

#[test]
fn fix_of_a_word_that_a_dictionary_of_the_text_knows_asks_again_without_the_typo_rule() {
    let mut engine = engine_with_two_languages("we will all meet at the reunion");

    let reaction = engine.on_completion(correction(" reunion", " reunión", " de las nueve"));

    assert_eq!(
        reaction,
        request_without_typo_rule("we will all meet at the reunion")
    );
}

#[test]
fn fix_of_fewer_than_five_letters_that_another_language_knows_asks_again_without_the_typo_rule() {
    let mut engine = engine_with_two_languages("les recuerdo que mañana tenemos que esperar hart");

    let reaction = engine.on_completion(correction(" hart", " hast", "a el lunes"));

    assert_eq!(
        reaction,
        request_without_typo_rule("les recuerdo que mañana tenemos que esperar hart")
    );
}

#[test]
fn fix_of_a_word_after_too_few_words_for_a_language_asks_again_without_the_typo_rule() {
    let mut engine = engine_with_two_languages("la reunion");

    let reaction = engine.on_completion(correction(" reunion", " reunión", " de las nueve"));

    assert_eq!(reaction, request_without_typo_rule("la reunion"));
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
