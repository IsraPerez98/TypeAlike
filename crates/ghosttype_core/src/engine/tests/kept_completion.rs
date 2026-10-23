//! Tests how a running completion keeps its request while the typed keys continue its text.

use crate::engine::{Engine, Reaction};
use crate::history::HistoryEntry;
use crate::inference::{InferenceError, InferenceRequest};
use crate::key::Key;
use crate::settings::{HistoryAccess, WordChoice};
use crate::test_support::{MAIL_APP, completion, mail_entry};

use super::support::{
    FakeDesktop, engine_waiting_for, engine_with_history, history_settings, press, request, show, type_text,
};

fn engine_running(context: &str, added: &str) -> Engine {
    let mut engine = engine_waiting_for(context);
    engine.record_partial_completion(added);
    engine
}

#[test]
fn key_that_the_running_completion_added_keeps_its_request() {
    let mut engine = engine_running("hel", "lo big wor");

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn completion_after_kept_keys_shows_its_text_after_them() {
    let mut engine = engine_running("hel", "lo big wor");
    type_text(&mut engine, "lo b");

    let reaction = engine.on_completion(completion("lo big world"));

    assert_eq!(reaction, show("ig world"));
}

#[test]
fn key_that_differs_from_the_running_completion_requests_the_new_text() {
    let mut engine = engine_running("hel", "lo big wor");

    let reaction = press(&mut engine, Key::Char('p'));

    assert_eq!(reaction, request("help"));
}

#[test]
fn key_after_all_the_added_text_requests_the_new_text() {
    let mut engine = engine_running("hel", "lo");
    type_text(&mut engine, "lo");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, request("hello "));
}

#[test]
fn completion_without_text_after_the_kept_keys_requests_the_text_before_the_caret() {
    let final_texts = ["lo", "lo b", "p me", "lo big big big"];

    let reactions = final_texts.map(|text| {
        let mut engine = engine_running("hel", "lo big wor");
        type_text(&mut engine, "lo b");
        engine.on_completion(completion(text))
    });

    assert_eq!(reactions, final_texts.map(|_| request("hello b")));
}

#[test]
fn completion_without_text_after_the_accepted_part_requests_the_next_suggestion() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big");
    press(&mut engine, Key::Tab);

    let reaction = engine.on_completion(completion("lo"));

    assert_eq!(reaction, request("hello"));
}

#[test]
fn failed_completion_after_kept_keys_hides() {
    let mut engine = engine_running("hel", "lo big wor");
    type_text(&mut engine, "lo b");

    let reaction = engine.on_completion(Err(InferenceError::Failed));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn bad_running_completion_keeps_no_key() {
    let mut engine = engine_running("Thanks.", " Hi S. Hi S. Hi S. Hi");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, request("Thanks. "));
}

#[test]
fn part_of_a_replaced_request_keeps_no_key() {
    let mut engine = engine_waiting_for("he");
    press(&mut engine, Key::Char('l'));
    engine.record_partial_completion("llo big");

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, request("hell"));
}

const TEXT_AFTER_CARET: &str = " yesterday, so you can see it.";

fn engine_running_before_text_after_caret() -> Engine {
    let mut engine = Engine::default();
    engine.on_key(
        Key::Char(' '),
        &FakeDesktop::with_text_around("I paid the", TEXT_AFTER_CARET),
    );
    engine.record_partial_completion("invoice yesterday, so");
    engine
}

#[test]
fn keys_up_to_the_end_that_fits_the_text_after_the_caret_keep_the_request() {
    let mut engine = engine_running_before_text_after_caret();

    let reaction = type_text(&mut engine, "invoice");

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn key_past_the_end_that_fits_the_text_after_the_caret_requests_the_new_text() {
    let mut engine = engine_running_before_text_after_caret();
    type_text(&mut engine, "invoice");
    let new_request = Engine::default().on_key(
        Key::Char(' '),
        &FakeDesktop::with_text_around("I paid the invoice", TEXT_AFTER_CARET),
    );

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, new_request);
}

#[test]
fn typing_all_the_shown_words_keeps_the_request_for_its_next_words() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big wor");
    type_text(&mut engine, "lo big");

    let reaction = engine.on_partial_completion("lo big world and");

    assert_eq!(reaction, Some(show(" world")));
}

fn engine_with_writing(entries: Vec<HistoryEntry>) -> Engine {
    let mut settings = history_settings(HistoryAccess::On, Vec::new());
    settings.personalization.word_choice = WordChoice::Medium;
    engine_with_history(&settings, entries)
}

fn mail_field_with(before_caret: &str) -> FakeDesktop {
    FakeDesktop::with_field_text(before_caret).in_app(MAIL_APP)
}

#[test]
fn key_after_which_the_writer_repeats_a_phrase_shows_the_phrase() {
    let mut engine = engine_with_writing(vec![mail_entry("Let me know if it works."); 3]);
    engine.on_key(Key::Char('o'), &mail_field_with("Please let me kn"));
    engine.record_partial_completion("w if it");

    let reaction = press(&mut engine, Key::Char('w'));

    assert_eq!(reaction, show(" if"));
}

fn next_word_bias_of(reaction: Reaction) -> Option<Vec<(String, f32)>> {
    match reaction {
        Reaction::Request(InferenceRequest::Complete { next_word_bias, .. }) => Some(next_word_bias.0),
        _ => None,
    }
}

#[test]
fn key_after_which_the_writer_has_next_words_requests_them() {
    let mut engine = engine_with_writing(vec![
        mail_entry("Let me know if it works."),
        mail_entry("Let me know when you can."),
    ]);
    engine.on_key(Key::Char('w'), &mail_field_with("Please let me kno"));
    engine.record_partial_completion(" if it");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(
        next_word_bias_of(reaction),
        Some(vec![("if".to_owned(), 0.5), ("when".to_owned(), 0.5)])
    );
}
