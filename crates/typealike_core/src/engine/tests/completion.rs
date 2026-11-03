//! Tests how completions and their parts become the ghost text.

use crate::engine::{Engine, Reaction};
use crate::inference::InferenceError;
use crate::key::{Key, Press};
use crate::settings::{Settings, WriterName, WriterNotes};
use crate::test_support::completion;

use super::support::{FakeDesktop, accept, engine_waiting_for, engine_waiting_with, press, request, show};

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

fn named_writer(notes: &str) -> Settings {
    Settings {
        writer_name: WriterName("Sara Novak".to_owned()),
        writer_notes: WriterNotes(notes.to_owned()),
        ..Settings::default()
    }
}

#[test]
fn completion_that_starts_its_line_as_a_line_of_the_prompt_frame_hides() {
    let cases = [
        ("The ", "part of my writing that I am typing"),
        ("My ", "name is Sara Novak."),
        ("I ", "type the text below on my computer"),
        ("N", "otes about me:"),
    ];

    let reactions = cases.map(|(context, text)| {
        engine_waiting_with(&named_writer("I am a nurse."), context).on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|_| Reaction::Hide));
}

#[test]
fn completion_that_holds_five_words_of_a_line_of_the_prompt_frame_in_a_row_hides() {
    let cases = [
        ("Ma the ", "text below on my computer, for example"),
        ("Thanks. The ", "part of my writing that I"),
        ("See you soon. ", "Best, Sara. The part of my writing that"),
    ];

    let reactions = cases.map(|(context, text)| {
        engine_waiting_with(&named_writer("I am a nurse."), context).on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|_| Reaction::Hide));
}

#[test]
fn completion_that_shares_less_than_the_start_of_a_frame_line_shows() {
    let cases = [
        ("The ", "part of my job is", "part of my job is"),
        ("Thanks. The ", "part of my writing is", "part of my writing is"),
        ("Hi, my ", "name is Sara Novak.", "name is Sara Novak."),
        ("My name is Sara Novak", " and I", " and I"),
        ("I ", "am a nurse.", "am a nurse."),
    ];

    let reactions = cases.map(|(context, text, _)| {
        engine_waiting_with(&named_writer("I am a nurse."), context).on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|(_, _, shown)| show(shown)));
}

#[test]
fn completion_that_starts_as_a_line_that_the_prompt_does_not_have_shows() {
    let cases = [
        ("My ", "name is Sara Novak."),
        ("Below ", "the field on my screen:"),
    ];

    let reactions = cases.map(|(context, text)| engine_waiting_for(context).on_completion(completion(text)));

    assert_eq!(reactions, cases.map(|(_, text)| show(text)));
}

#[test]
fn completion_that_opens_a_parenthesis_after_the_end_of_a_sentence_hides() {
    let cases = [
        ("I can pick you up at seven. ", "(This is a casual message"),
        ("Llamar al banco y pagar la luz. ", "(Call the bank and pay"),
        ("Are you coming?", " (Just checking"),
        ("明天见。", "（See you tomorrow"),
    ];

    let reactions = cases.map(|(context, text)| engine_waiting_for(context).on_completion(completion(text)));

    assert_eq!(reactions, cases.map(|_| Reaction::Hide));
}

#[test]
fn completion_with_a_parenthesis_inside_a_sentence_shows() {
    let cases = [
        ("I bought a new", " sweater (it might be"),
        ("The total is 40 euros", " (before tax)."),
        ("See you at 5. ", "I will bring snacks (and drinks)."),
    ];

    let reactions = cases.map(|(context, text)| engine_waiting_for(context).on_completion(completion(text)));

    assert_eq!(reactions, cases.map(|(_, text)| show(text)));
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
            Press::First,
            &FakeDesktop::with_text_around("Thanks for the", "update on the budget."),
        );
        engine.on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|_| Reaction::Hide));
}

#[test]
fn completion_that_shares_one_word_or_sign_with_the_text_after_the_caret_shows() {
    let cases = [
        ("Thanks for the", "update on the budget.", "update from Priya"),
        (
            "grow the team,",
            ", and open an office.",
            "increase revenue, improve onboarding",
        ),
    ];

    let reactions = cases.map(|(before, after, text)| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_text_around(before, after),
        );
        engine.on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|(_, _, text)| show(text)));
}

#[test]
fn completion_stops_where_it_writes_the_text_after_the_caret_again() {
    let cases = [
        (
            "I paid the",
            " yesterday, so you can see it.",
            "invoice yesterday, so you",
            "invoice",
        ),
        (
            "tell me the",
            " of the shade? I want",
            "dimensions of the shade?",
            "dimensions",
        ),
        (
            "people who",
            " sleep better.",
            "exercise “sleep better",
            "exercise",
        ),
        (
            "I will be there at",
            ". Order me a coffee!",
            "10:30. Order me",
            "10:30",
        ),
        (
            "grow the team,",
            ", and open an office.",
            "improve onboarding, and open",
            "improve onboarding",
        ),
        (
            "Ya subí los",
            " a la rama de pruebas, ¿alguien",
            "cambios a la rama principal.",
            "cambios",
        ),
        (
            "I will be there in",
            ". Order me a coffee!",
            "10 minutes. Order a tea",
            "10 minutes",
        ),
    ];

    let reactions = cases.map(|(before, after, text, _)| {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            Press::First,
            &FakeDesktop::with_text_around(before, after),
        );
        engine.on_completion(completion(text))
    });

    assert_eq!(reactions, cases.map(|(_, _, _, shown)| show(shown)));
}

#[test]
fn partial_completion_shows_up_to_the_text_after_the_caret() {
    let mut engine = Engine::default();
    engine.on_key(
        Key::Char(' '),
        Press::First,
        &FakeDesktop::with_text_around("I paid the", " yesterday, so you can see it."),
    );

    let reaction = engine.on_partial_completion("invoice yesterday, so ");

    assert_eq!(reaction, Some(show("invoice")));
}

#[test]
fn completion_keeps_the_words_of_the_text_on_the_next_lines() {
    let mut engine = Engine::default();
    engine.on_key(
        Key::Char(' '),
        Press::First,
        &FakeDesktop::with_text_around("I paid the", "\nyesterday was busy."),
    );

    let reaction = engine.on_completion(completion("invoice yesterday"));

    assert_eq!(reaction, show("invoice yesterday"));
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
        Press::First,
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
