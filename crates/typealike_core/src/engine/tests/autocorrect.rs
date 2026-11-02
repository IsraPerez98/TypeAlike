//! Tests how a session takes the changes that the app makes to the typed text, such as an
//! autocorrect.

use crate::engine::{Engine, Reaction};
use crate::history::HistoryEntry;
use crate::key::{Key, Press};
use crate::settings::{HistoryAccess, KeptTexts, Settings};
use crate::test_support::{MAIL_APP, mail_entry};

use super::support::{FakeDesktop, engine_with, history_settings, request};

/// Types each character of `text` in `desktop`.
fn type_in(engine: &mut Engine, desktop: &FakeDesktop, text: &str) {
    for character in text.chars() {
        engine.on_key(Key::Char(character), Press::First, desktop);
    }
}

/// Returns an engine that typed `typed` in a field that held `field_before` before the caret.
fn engine_after_typing(field_before: &str, typed: &str) -> Engine {
    let mut engine = Engine::default();
    type_in(&mut engine, &FakeDesktop::with_field_text(field_before), typed);
    engine
}

/// Takes the field read `field_before` and returns the request for the next key `t`.
fn request_after_read(engine: &mut Engine, field_before: &str) -> Reaction {
    engine.follow_field(&FakeDesktop::with_field_text(field_before));
    engine.on_key(Key::Char('t'), Press::First, &FakeDesktop::default())
}

#[test]
fn next_request_continues_the_word_that_the_app_corrected() {
    let mut engine = engine_after_typing("", "I recieve ");

    let reaction = request_after_read(&mut engine, "I receive ");

    assert_eq!(reaction, request("I receive t"));
}

#[test]
fn text_that_the_field_held_before_the_session_stays_when_the_app_corrects_a_word() {
    let mut engine = engine_after_typing("Dear Ana,\n", "I recieve ");

    let reaction = request_after_read(&mut engine, "Dear Ana,\nI receive ");

    assert_eq!(reaction, request("Dear Ana,\nI receive t"));
}

#[test]
fn later_read_takes_the_next_word_that_the_app_corrected() {
    let mut engine = engine_after_typing("", "I recieve ");
    engine.follow_field(&FakeDesktop::with_field_text("I receive "));
    type_in(&mut engine, &FakeDesktop::default(), "teh ");

    let reaction = request_after_read(&mut engine, "I receive the ");

    assert_eq!(reaction, request("I receive the t"));
}

#[test]
fn later_read_takes_the_caret_word_of_an_earlier_read_that_the_app_corrected() {
    let mut engine = engine_after_typing("", "i");
    engine.follow_field(&FakeDesktop::with_field_text("i"));
    type_in(&mut engine, &FakeDesktop::default(), " recieve ");

    let reaction = request_after_read(&mut engine, "I receive ");

    assert_eq!(reaction, request("I receive t"));
}

#[test]
fn caret_word_of_the_field_at_the_start_takes_the_correction_of_the_app() {
    let mut engine = engine_after_typing("I recie", "ve ");

    let reaction = request_after_read(&mut engine, "I receive ");

    assert_eq!(reaction, request("I receive t"));
}

#[test]
fn typed_text_stays_when_the_field_shows_no_correction_of_finished_words() {
    let field_reads = [
        "I recieve th",
        "I recieve teh",
        "I recieve the big",
        "Something else entirely",
        "I received all of the",
    ];

    let reactions = field_reads.map(|field_before| {
        let mut engine = engine_after_typing("", "I recieve the");
        request_after_read(&mut engine, field_before)
    });

    assert_eq!(reactions, field_reads.map(|_| request("I recieve thet")));
}

#[test]
fn history_keeps_the_word_that_the_app_corrected() {
    let mut settings = history_settings(HistoryAccess::On, Vec::new());
    settings.personalization.kept_texts = KeptTexts::All;
    let mut engine = engine_with(&settings);
    let field = FakeDesktop::with_field_text("").in_app(MAIL_APP);
    type_in(&mut engine, &field, "I recieve the report ");
    engine.follow_field(&FakeDesktop::with_field_text("I receive the report ").in_app(MAIL_APP));

    engine.on_key(Key::Other, Press::First, &field);

    assert_eq!(
        engine.history_to_save().map(<[HistoryEntry]>::to_vec),
        Some(vec![mail_entry("I receive the report")])
    );
}

#[test]
fn field_read_without_a_session_starts_none() {
    let mut engine = engine_with(&Settings::default());

    engine.follow_field(&FakeDesktop::with_field_text("I receive "));

    assert!(!engine.has_session());
}
