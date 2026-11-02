//! Tests the pause of the suggestions in a field after Escape rejects the suggestion that shows.

use crate::engine::{Engine, Reaction};
use crate::key::{Key, Press};
use crate::test_support::completion;

use super::support::{FakeDesktop, engine_showing, press, request};

/// Returns an engine where Escape rejected `suggestion`, which showed after `context`.
fn engine_paused_after(context: &str, suggestion: &str) -> Engine {
    let mut engine = engine_showing(context, suggestion);
    press(&mut engine, Key::Escape);
    engine
}

#[test]
fn escape_on_a_shown_suggestion_pauses_the_field() {
    let mut engine = engine_showing("hel", "lo world");

    let reaction = press(&mut engine, Key::Escape);

    assert_eq!(reaction, Reaction::PauseField);
}

#[test]
fn typed_key_in_a_paused_field_gets_no_suggestion() {
    let mut engine = engine_paused_after("hel", "lo world");

    let reaction = press(&mut engine, Key::Char('p'));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn earlier_suggestion_does_not_show_in_a_paused_field() {
    let mut engine = engine_showing("he", "y there");
    press(&mut engine, Key::Char('l'));
    engine.on_completion(completion("lo world"));
    press(&mut engine, Key::Escape);

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn field_stays_paused_after_a_key_that_ends_the_session() {
    let mut engine = engine_paused_after("hel", "lo world");
    press(&mut engine, Key::Other);

    let reaction = engine.on_key(Key::Char('p'), Press::First, &FakeDesktop::with_field_text("hel"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn key_after_the_pause_requests_a_suggestion_with_the_text_typed_in_the_pause() {
    let mut engine = engine_paused_after("hel", "lo world");
    press(&mut engine, Key::Char('p'));
    engine.end_field_pause();

    let reaction = press(&mut engine, Key::Char('s'));

    assert_eq!(reaction, request("helps"));
}
