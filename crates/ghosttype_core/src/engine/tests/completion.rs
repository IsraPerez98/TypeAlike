//! Tests how completions and their parts become the ghost text.

use crate::engine::{Engine, Reaction};
use crate::inference::InferenceError;
use crate::key::Key;
use crate::test_support::completion;

use super::support::{FakeDesktop, accept, engine_waiting_for, press, request, show};

#[test]
fn typing_requests_completion_of_context() {
    let mut engine = Engine::default();

    let reaction = press(&mut engine, Key::Char('h'));

    assert_eq!(reaction, request("h"));
}

#[test]
fn typing_while_waiting_requests_the_new_context() {
    let mut engine = engine_waiting_for("h");

    let reaction = press(&mut engine, Key::Char('e'));

    assert_eq!(reaction, request("he"));
}

#[test]
fn whitespace_context_requests_nothing() {
    let mut engine = Engine::default();

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn completion_for_current_context_shows() {
    let mut engine = engine_waiting_for("hel");

    let reaction = engine.on_completion(completion("lo"));

    assert_eq!(reaction, show("lo"));
}

#[test]
fn completion_of_a_replaced_request_is_dropped() {
    let mut engine = engine_waiting_for("he");
    press(&mut engine, Key::Char('l'));

    let reaction = engine.on_completion(completion("llo"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn completion_of_the_newest_request_shows() {
    let mut engine = engine_waiting_for("he");
    press(&mut engine, Key::Char('l'));
    engine.on_completion(Err(InferenceError::Superseded));

    let reaction = engine.on_completion(completion("lo"));

    assert_eq!(reaction, show("lo"));
}

#[test]
fn failed_completion_hides() {
    let mut engine = engine_waiting_for("hel");

    let reaction = engine.on_completion(Err(InferenceError::NoModel));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn completion_without_text_on_first_line_hides() {
    let cases = ["", "  ", "\nnext line"];

    let reactions = cases.map(|text| engine_waiting_for("hel").on_completion(completion(text)));

    assert_eq!(reactions, cases.map(|_| Reaction::Hide));
}

#[test]
fn completion_stops_at_line_break() {
    let mut engine = engine_waiting_for("hel");

    let reaction = engine.on_completion(completion("lo there\nNext line"));

    assert_eq!(reaction, show("lo there"));
}

#[test]
fn completion_drops_trailing_whitespace() {
    let mut engine = engine_waiting_for("hel");

    let reaction = engine.on_completion(completion("lo  "));

    assert_eq!(reaction, show("lo"));
}

#[test]
fn completion_after_whitespace_drops_leading_whitespace() {
    let mut engine = engine_waiting_for("Hi ");

    let reaction = engine.on_completion(completion("  there"));

    assert_eq!(reaction, show("there"));
}

#[test]
fn completion_after_word_keeps_leading_space() {
    let mut engine = engine_waiting_for("Hi");

    let reaction = engine.on_completion(completion(" there"));

    assert_eq!(reaction, show(" there"));
}

#[test]
fn bad_completion_hides() {
    let cases = [
        ("Best", ","),
        ("Best regards", " …"),
        ("Best ", ", regards"),
        ("I think", " no no no"),
        ("Thanks.", " Hi S. Hi S. Hi S."),
        ("Hi Sarah, thanks for s", "ending. Hi Sarah, thanks for s"),
        ("My new address is", " [Your New Address]."),
        ("Send it to 14 Orchard Lane,", " [City"),
    ];

    let reactions = cases.map(|(context, text)| engine_waiting_for(context).on_completion(completion(text)));

    assert_eq!(reactions, cases.map(|_| Reaction::Hide));
}

#[test]
fn completion_stops_before_a_placeholder() {
    let cases = [
        ("Send it to", " 14 Orchard Lane, [City", " 14 Orchard Lane,"),
        ("See you at", " 5 on [day]", " 5 on"),
        ("See [the docs", "] for details", "] for details"),
    ];

    let reactions =
        cases.map(|(context, text, _)| engine_waiting_for(context).on_completion(completion(text)));

    assert_eq!(reactions, cases.map(|(_, _, shown)| show(shown)));
}

#[test]
fn completion_close_to_a_bad_one_shows() {
    let cases = [
        ("Best", ", regards", ", regards"),
        ("I think", " no no", " no no"),
        ("Thanks.", " Hi S. Hi S.", " Hi S. Hi S."),
        (
            "Hi Sarah, thanks for the",
            " thanks for the help",
            " thanks for the help",
        ),
    ];

    let reactions =
        cases.map(|(context, text, _)| engine_waiting_for(context).on_completion(completion(text)));

    assert_eq!(reactions, cases.map(|(_, _, shown)| show(shown)));
}

#[test]
fn completion_that_repeats_the_text_after_the_caret_hides() {
    let cases = ["Update on the budget.", "update", "update on the budget. See you"];

    let reactions = cases.map(|text| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            &FakeDesktop::with_text_around("Thanks for the", "update on the budget."),
        );
        engine.on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|_| Reaction::Hide));
}

#[test]
fn completion_that_starts_like_the_text_after_the_caret_shows() {
    let mut engine = Engine::default();
    engine.on_key(
        Key::Char(' '),
        &FakeDesktop::with_text_around("Thanks for the", "update on the budget."),
    );

    let reaction = engine.on_completion(completion("update from Priya"));

    assert_eq!(reaction, show("update from Priya"));
}

#[test]
fn bad_partial_completion_hides() {
    let mut engine = engine_waiting_for("Thanks.");
    engine.on_partial_completion(" Hi S. Hi S. Hi");

    let reaction = engine.on_partial_completion(" Hi S. Hi S. Hi S. Hi");

    assert_eq!(reaction, Some(Reaction::Hide));
}

#[test]
fn bad_completion_hides_the_shown_partial_completion() {
    let mut engine = engine_waiting_for("Thanks.");
    engine.on_partial_completion(" Hi S. Hi S. Hi");

    let reaction = engine.on_completion(completion(" Hi S. Hi S. Hi S."));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn good_partial_completion_after_a_bad_one_shows() {
    let mut engine = Engine::default();
    engine.on_key(
        Key::Char(' '),
        &FakeDesktop::with_text_around("I like it.", "The end."),
    );
    engine.on_partial_completion("The weat");

    let reaction = engine.on_partial_completion("The weather is");

    assert_eq!(reaction, Some(show("The weather")));
}

#[test]
fn partial_completion_shows_its_complete_words() {
    let mut engine = engine_waiting_for("hel");

    let reaction = engine.on_partial_completion("lo big wor");

    assert_eq!(reaction, Some(show("lo big")));
}

#[test]
fn partial_completion_without_a_complete_word_changes_nothing() {
    let cases = ["lo", " big"];

    let reactions = cases.map(|text| engine_waiting_for("hel").on_partial_completion(text));

    assert_eq!(reactions, cases.map(|_| None));
}

#[test]
fn partial_completion_that_adds_no_word_changes_nothing() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wo");

    let reaction = engine.on_partial_completion("lo big wor");

    assert_eq!(reaction, None);
}

#[test]
fn partial_completion_of_a_replaced_request_changes_nothing() {
    let mut engine = engine_waiting_for("he");
    press(&mut engine, Key::Char('l'));

    let reaction = engine.on_partial_completion("llo big world");

    assert_eq!(reaction, None);
}

#[test]
fn partial_completion_after_escape_changes_nothing() {
    let mut engine = engine_waiting_for("hel");
    press(&mut engine, Key::Escape);

    let reaction = engine.on_partial_completion("lo big world");

    assert_eq!(reaction, None);
}

#[test]
fn completion_after_partial_completion_shows_all_its_text() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");

    let reaction = engine.on_completion(completion("lo big world"));

    assert_eq!(reaction, show("lo big world"));
}

#[test]
fn failed_completion_keeps_the_shown_partial_completion() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");

    let reaction = engine.on_completion(Err(InferenceError::Failed));

    assert_eq!(reaction, show("lo big"));
}

#[test]
fn typed_character_of_a_partial_completion_stays_out_of_later_parts() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");
    press(&mut engine, Key::Char('l'));

    let reaction = engine.on_partial_completion("lo big world and");

    assert_eq!(reaction, Some(show("o big world")));
}

#[test]
fn typed_space_after_a_word_stays_out_of_later_parts() {
    let mut engine = engine_waiting_for("Hi");
    engine.on_partial_completion(" there my fri");
    press(&mut engine, Key::Char(' '));

    let reaction = engine.on_completion(completion(" there my friend"));

    assert_eq!(reaction, show("there my friend"));
}

#[test]
fn accepted_word_of_a_partial_completion_stays_out_of_later_parts() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");
    press(&mut engine, Key::Tab);

    let reaction = engine.on_partial_completion("lo big world and");

    assert_eq!(reaction, Some(show(" big world")));
}

#[test]
fn partial_completion_after_accepting_all_shown_words_shows_the_next_words() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big");
    press(&mut engine, Key::Tab);

    let reaction = engine.on_completion(completion("lo big world"));

    assert_eq!(reaction, show(" big world"));
}

#[test]
fn completion_that_differs_from_the_typed_text_keeps_the_shown_text() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");
    press(&mut engine, Key::Char('l'));

    let reaction = engine.on_completion(completion("p me"));

    assert_eq!(reaction, show("o big"));
}

#[test]
fn tab_on_a_partial_completion_accepts_its_first_word() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept("lo", " big"));
}
