//! Tests the fix of a typo before the caret and how Tab accepts it.

use crate::engine::{AfterAccept, Engine, Reaction, WordFix};
use crate::inference::{Completion, Correction, InferenceError};
use crate::key::Key;
use crate::settings::{Settings, TypoFix};

use super::support::{completion, engine_waiting_for, engine_with, press, request, type_text};

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

fn word_fix(typo: &str, word: &str, rest: &str) -> WordFix {
    WordFix {
        typo: typo.to_owned(),
        word: word.to_owned(),
        rest: rest.to_owned(),
    }
}

fn engine_showing_a_fix() -> Engine {
    let mut engine = engine_waiting_for("the dlei");
    engine.on_completion(correction(" dlei", " deli", "very soon"));
    engine
}

#[test]
fn completion_that_fixes_a_typo_shows_the_fix() {
    let mut engine = engine_waiting_for("the dlei");

    let reaction = engine.on_completion(correction(" dlei", " deli", "very soon"));

    assert_eq!(reaction, Reaction::ShowFix(word_fix("dlei", "delivery", " soon")));
}

#[test]
fn fix_that_finishes_its_word_shows_the_rest_after_it() {
    let mut engine = engine_waiting_for("I will recieve");

    let reaction = engine.on_completion(correction(" recieve", " receive", " the parcel"));

    assert_eq!(
        reaction,
        Reaction::ShowFix(word_fix("recieve", "receive", " the parcel"))
    );
}

#[test]
fn tab_on_a_fix_replaces_the_typo_with_the_fixed_word() {
    let mut engine = engine_showing_a_fix();

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 4,
            text: "delivery".to_owned(),
            then: AfterAccept::Rest(" soon".to_owned()),
        }
    );
}

#[test]
fn tab_on_a_fix_without_a_rest_requests_the_next_suggestion() {
    let mut engine = engine_waiting_for("the dlei");
    engine.on_completion(correction(" dlei", " deli", "very"));

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 4,
            text: "delivery".to_owned(),
            then: AfterAccept::Next(Box::new(request("the delivery"))),
        }
    );
}

#[test]
fn tab_on_a_fix_leaves_the_closing_punctuation_for_the_next_tab() {
    let mut engine = engine_waiting_for("the dlei");
    engine.on_completion(correction(" dlei", " deli", "very, soon"));

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 4,
            text: "delivery".to_owned(),
            then: AfterAccept::Rest(", soon".to_owned()),
        }
    );
}

#[test]
fn text_before_the_caret_holds_the_fixed_word_after_tab() {
    let mut engine = engine_showing_a_fix();
    press(&mut engine, Key::Tab);
    press(&mut engine, Key::Escape);

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, request("the deliver"));
}

#[test]
fn typing_while_a_fix_shows_asks_again() {
    let mut engine = engine_showing_a_fix();

    let reaction = press(&mut engine, Key::Char('v'));

    assert_eq!(reaction, request("the dleiv"));
}

#[test]
fn escape_hides_a_fix() {
    let mut engine = engine_showing_a_fix();

    let reaction = press(&mut engine, Key::Escape);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_with_a_bad_suggestion_after_it_hides() {
    let mut engine = engine_waiting_for("the dlei");

    let reaction = engine.on_completion(correction(" dlei", " deli", "very very very very"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_of_text_that_does_not_end_the_text_before_the_caret_hides() {
    let mut engine = engine_waiting_for("the dlei");

    let reaction = engine.on_completion(correction(" plea", " please", " help"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_with_typo_fixes_off_hides() {
    let mut engine = engine_with(&Settings {
        typo_fix: TypoFix::Off,
        ..Settings::default()
    });
    type_text(&mut engine, "the dlei");

    let reaction = engine.on_completion(correction(" dlei", " deli", "very soon"));

    assert_eq!(reaction, Reaction::Hide);
}
